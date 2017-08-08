//! The logger actor receives log messages and writes them to a log file.

use super::{ReducerId, WorkerId};
use error;
use git2;
use histo::Histogram;
use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::path;
use std::sync::mpsc;
use std::thread;
use test_case::{self, TestCaseMethods};

/// The different kinds of log messages that can be sent to the logger actor.
#[derive(Debug)]
enum LoggerMessage {
    SpawningWorker(WorkerId),
    SpawnedWorker(WorkerId),
    SpawningReducer(ReducerId),
    SpawnedReducer(ReducerId),
    ShutdownWorker(WorkerId),
    ShutdownReducer(ReducerId),
    WorkerPanicked(WorkerId, Box<Any + Send + 'static>),
    WorkerErrored(WorkerId, error::Error),
    ReducerPanicked(ReducerId, Box<Any + Send + 'static>),
    ReducerErrored(ReducerId, error::Error),
    BackingUpTestCase(String, String),
    StartJudgingInteresting(WorkerId, test_case::PotentialReduction),
    JudgedInteresting(WorkerId, test_case::Interesting),
    JudgedNotInteresting(WorkerId, test_case::PotentialReduction),
    NewSmallest(test_case::Interesting, u64),
    IsNotSmaller(test_case::Interesting),
    StartGeneratingNextReduction(ReducerId),
    FinishGeneratingNextReduction(ReducerId, test_case::PotentialReduction),
    NoMoreReductions(ReducerId),
    FinalReducedSize(u64, u64),
    TryMerge(WorkerId, git2::Oid, git2::Oid),
    FinishedMerging(WorkerId, u64, u64),
}

impl fmt::Display for LoggerMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            LoggerMessage::SpawningWorker(id) => write!(f, "Supervisor: Spawning worker {}", id),
            LoggerMessage::SpawnedWorker(id) => write!(f, "Worker {}: spawned", id),
            LoggerMessage::SpawningReducer(id) => write!(f, "Supervisor: Spawning reducer {}", id),
            LoggerMessage::SpawnedReducer(id) => write!(f, "Reducer {}: spawned", id),
            LoggerMessage::ShutdownWorker(id) => write!(f, "Worker {}: shutting down", id),
            LoggerMessage::ShutdownReducer(id) => write!(f, "Reducer {}: shutting down", id),
            LoggerMessage::WorkerErrored(id, ref err) => write!(f, "Worker {}: error: {}", id, err),
            LoggerMessage::WorkerPanicked(id, _) => write!(f, "Worker {}: panicked!", id),
            LoggerMessage::ReducerErrored(id, ref err) => {
                write!(f, "Reducer {}: error: {}", id, err)
            }
            LoggerMessage::ReducerPanicked(id, _) => write!(f, "Reducer {}: panicked!", id),
            LoggerMessage::BackingUpTestCase(ref from, ref to) => {
                write!(
                    f,
                    "Supervisor: backing up initial test case from {} to {}",
                    from,
                    to
                )
            }
            LoggerMessage::StartJudgingInteresting(id, ref reduction) => {
                write!(
                    f,
                    "Worker {}: judging if test case {} of size {} is interesting...",
                    id,
                    reduction.path().display(),
                    reduction.size()
                )
            }
            LoggerMessage::JudgedInteresting(id, ref interesting) => {
                write!(
                    f,
                    "Worker {}: found an interesting test case {} of size {} bytes",
                    id,
                    interesting.path().display(),
                    interesting.size()
                )
            }
            LoggerMessage::JudgedNotInteresting(id, ref reduction) => {
                write!(
                    f,
                    "Worker {}: found test case {}, generated by {}, not interesting",
                    id,
                    reduction.path().display(),
                    reduction.provenance()
                )
            }
            LoggerMessage::NewSmallest(ref interesting, orig_size) => {
                let new_size = interesting.size();
                assert!(new_size < orig_size);
                assert!(orig_size != 0);
                let percent = ((orig_size - new_size) as f64) / (orig_size as f64) * 100.0;
                write!(
                    f,
                    "Supervisor: new smallest interesting test case {}: {} bytes ({:.2}% reduced) -- generated by {}",
                    interesting.path().display(),
                    new_size,
                    percent,
                    interesting.provenance()
                )
            }
            LoggerMessage::IsNotSmaller(ref reduction) => {
                write!(
                    f,
                    "Supervisor: interesting test case {}, generated by {}, is not new smallest; tell worker to try merging",
                    reduction.path().display(),
                    reduction.provenance()
                )
            }
            LoggerMessage::StartGeneratingNextReduction(id) => {
                write!(f, "Reducer {}: generating next reduction...", id)
            }
            LoggerMessage::FinishGeneratingNextReduction(id, ref reduction) => {
                write!(
                    f,
                    "Reducer {}: finished generating next reduction {} of size {}",
                    id,
                    reduction.path().display(),
                    reduction.size()
                )
            }
            LoggerMessage::NoMoreReductions(id) => write!(f, "Reducer {}: no more reductions", id),
            LoggerMessage::FinalReducedSize(final_size, orig_size) => {
                assert!(final_size <= orig_size);
                let percent = if orig_size == 0 {
                    100.0
                } else {
                    ((orig_size - final_size) as f64) / (orig_size as f64) * 100.0
                };
                write!(
                    f,
                    "Supervisor: final reduced size is {} bytes ({:.2}% reduced)",
                    final_size,
                    percent
                )
            }
            LoggerMessage::TryMerge(id, upstream_commit, worker_commit) => {
                write!(
                    f,
                    "Worker {}: trying to merge upstream's {} into our {}",
                    id,
                    upstream_commit,
                    worker_commit
                )
            }
            LoggerMessage::FinishedMerging(id, merged_size, upstream_size) => {
                if merged_size >= upstream_size {
                    write!(
                        f,
                        "Worker {}: finished merging; not worth it; merged size {} >= upstream size {}",
                        id,
                        merged_size,
                        upstream_size
                    )
                } else {
                    write!(
                        f,
                        "Worker {}: finished merging; was worth it; merged size {} < upstream size {}",
                        id,
                        merged_size,
                        upstream_size
                    )
                }
            }
        }
    }
}

/// A client to the logger actor.
#[derive(Clone, Debug)]
pub struct Logger {
    sender: mpsc::Sender<LoggerMessage>,
}

/// Logger client implementation.
impl Logger {
    /// Spawn a `Logger` actor, writing logs to the given `Write`able.
    pub fn spawn<W>(to: W) -> error::Result<(Logger, thread::JoinHandle<()>)>
    where
        W: 'static + Send + Write,
    {
        let (sender, receiver) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("preduce-logger".into())
            .spawn(move || Logger::run(to, receiver))?;
        Ok((Logger { sender: sender }, handle))
    }

    /// Log the start of spawning a worker.
    pub fn spawning_worker(&self, id: WorkerId) {
        let _ = self.sender.send(LoggerMessage::SpawningWorker(id));
    }

    /// Log the end of spawning a worker.
    pub fn spawned_worker(&self, id: WorkerId) {
        let _ = self.sender.send(LoggerMessage::SpawnedWorker(id));
    }

    /// Log that we are backing up the initial test case.
    pub fn backing_up_test_case<P, Q>(&self, from: P, to: Q)
    where
        P: AsRef<path::Path>,
        Q: AsRef<path::Path>,
    {
        let from = from.as_ref().display().to_string();
        let to = to.as_ref().display().to_string();
        self.sender
            .send(LoggerMessage::BackingUpTestCase(from, to))
            .unwrap();
    }

    /// Log that the worker with the given id is shutting down.
    pub fn shutdown_worker(&self, id: WorkerId) {
        let _ = self.sender.send(LoggerMessage::ShutdownWorker(id));
    }

    /// Log that the reducer with the given id is shutting down.
    pub fn shutdown_reducer(&self, id: ReducerId) {
        let _ = self.sender.send(LoggerMessage::ShutdownReducer(id));
    }

    /// Log that the worker with the given id is shutting down.
    pub fn worker_errored(&self, id: WorkerId, err: error::Error) {
        let _ = self.sender.send(LoggerMessage::WorkerErrored(id, err));
    }

    /// Log that the worker with the given id is shutting down.
    pub fn worker_panicked(&self, id: WorkerId, panic: Box<Any + Send + 'static>) {
        let _ = self.sender.send(LoggerMessage::WorkerPanicked(id, panic));
    }

    /// Log that the worker with the given id has started running an
    /// is-interesting predicate on its test case.
    pub fn start_judging_interesting(&self, id: WorkerId, reduction: test_case::PotentialReduction) {
        let _ = self.sender.send(LoggerMessage::StartJudgingInteresting(id, reduction));
    }

    /// Log that the worker with the given id has discovered a new interesting
    /// test case.
    pub fn judged_interesting(&self, id: WorkerId, interesting: test_case::Interesting) {
        let _ = self.sender.send(LoggerMessage::JudgedInteresting(id, interesting));
    }

    /// Log that the worker with the given id has discovered that its test case
    /// is not interesting.
    pub fn judged_not_interesting(&self, id: WorkerId, reduction: test_case::PotentialReduction) {
        let _ = self.sender
            .send(LoggerMessage::JudgedNotInteresting(id, reduction));
    }

    /// Log that the supervisor has a new globally smallest interesting test
    /// case.
    pub fn new_smallest(&self, interesting: test_case::Interesting, orig_size: u64) {
        let new_size = interesting.size();
        assert!(new_size < orig_size);
        assert!(orig_size != 0);
        let _ = self.sender
            .send(LoggerMessage::NewSmallest(interesting, orig_size));
    }

    /// Log that the supervisor received a new interesting test case, but that
    /// it is not smaller than the current globally smallest interesting test
    /// case.
    pub fn is_not_smaller(&self, reduction: test_case::Interesting) {
        let _ = self.sender.send(LoggerMessage::IsNotSmaller(reduction));
    }

    /// Log that this reducer actor has started generating its next potential
    /// reduction.
    pub fn start_generating_next_reduction(&self, id: ReducerId) {
        let _ = self.sender
            .send(LoggerMessage::StartGeneratingNextReduction(id));
    }

    /// Log that this reducer actor has completed generating its next potential
    /// reduction.
    pub fn finish_generating_next_reduction(&self, id: ReducerId, reduction: test_case::PotentialReduction) {
        let _ = self.sender
            .send(LoggerMessage::FinishGeneratingNextReduction(id, reduction));
    }

    /// Log that this reducer actor has exhuasted potential reductions for the
    /// current globally smallest interesting test case.
    pub fn no_more_reductions(&self, id: ReducerId) {
        let _ = self.sender.send(LoggerMessage::NoMoreReductions(id));
    }

    /// Log the final reduced test case's size once the reduction process has
    /// completed.
    pub fn final_reduced_size(&self, final_size: u64, orig_size: u64) {
        assert!(final_size <= orig_size);
        let _ = self.sender
            .send(LoggerMessage::FinalReducedSize(final_size, orig_size));
    }

    /// Log that the worker with the given id is attempting a merge.
    pub fn try_merging(&self, id: WorkerId, upstream_commit: git2::Oid, worker_commit: git2::Oid) {
        let _ = self.sender
            .send(LoggerMessage::TryMerge(id, upstream_commit, worker_commit));
    }

    /// Log that the worker with the given id is attempting a merge.
    pub fn finished_merging(&self, id: WorkerId, merged_size: u64, upstream_size: u64) {
        let _ = self.sender.send(LoggerMessage::FinishedMerging(
            id,
            merged_size,
            upstream_size,
        ));
    }

    /// Log that the reducer with the given id is spawning.
    pub fn spawning_reducer(&self, id: ReducerId) {
        let _ = self.sender.send(LoggerMessage::SpawningReducer(id));
    }

    /// Log that the reducer with the given id has completed spawning.
    pub fn spawned_reducer(&self, id: ReducerId) {
        let _ = self.sender.send(LoggerMessage::SpawnedReducer(id));
    }

    /// Log that the reducer with the given id errored out.
    pub fn reducer_errored(&self, id: ReducerId, err: error::Error) {
        let _ = self.sender.send(LoggerMessage::ReducerErrored(id, err));
    }

    /// Log that the reducer with the given id is shutting down.
    pub fn reducer_panicked(&self, id: ReducerId, panic: Box<Any + Send + 'static>) {
        let _ = self.sender.send(LoggerMessage::ReducerPanicked(id, panic));
    }
}

const BUCKETS: u64 = 20;

fn sum(h: &Histogram) -> u64 {
    h.buckets().map(|b| b.count()).sum()
}

/// Logger actor implementation.
impl Logger {
    fn run<W>(mut to: W, incoming: mpsc::Receiver<LoggerMessage>)
    where
        W: Write,
    {
        let mut smallest_size = 0;

        // Reduction provenance -> (new smallest interesting,
        //                          interesting-but-not-smallest,
        //                          not interesting)
        let mut stats: BTreeMap<String, (Histogram, Histogram, Histogram)> = BTreeMap::new();

        // Histograms of various kinds of reductions' sizes.
        let mut all_reductions = Histogram::with_buckets(BUCKETS);
        let mut smallest_reductions = Histogram::with_buckets(BUCKETS);
        let mut not_smallest_reductions = Histogram::with_buckets(BUCKETS);
        let mut any_interesting_reductions = Histogram::with_buckets(BUCKETS);
        let mut not_interesting_reductions = Histogram::with_buckets(BUCKETS);

        for log_msg in incoming {
            writeln!(&mut to, "{}", log_msg).expect("Should write to log file");

            match log_msg {
                msg @ LoggerMessage::ReducerErrored(..) |
                msg @ LoggerMessage::WorkerErrored(..) |
                msg @ LoggerMessage::ReducerPanicked(..) |
                msg @ LoggerMessage::WorkerPanicked(..) => {
                    println!("{}", msg);
                }

                LoggerMessage::NewSmallest(interesting, orig_size) => {
                    let new_size = interesting.size();

                    smallest_size = new_size;

                    println!(
                        "({:.2}%, {} bytes)",
                        if orig_size == 0 {
                            100.0
                        } else {
                            ((orig_size - new_size) as f64) / (orig_size as f64) * 100.0
                        },
                        new_size
                    );

                    let provenance = interesting.provenance().to_string();
                    stats.entry(provenance)
                        .or_insert_with(|| (Histogram::with_buckets(BUCKETS),
                                            Histogram::with_buckets(BUCKETS),
                                            Histogram::with_buckets(BUCKETS)))
                        .0
                        .add(interesting.delta());

                    all_reductions.add(interesting.delta());
                    smallest_reductions.add(interesting.delta());
                    any_interesting_reductions.add(interesting.delta());
                }

                LoggerMessage::IsNotSmaller(interesting) => {
                    let provenance = interesting.provenance().to_string();
                    stats.entry(provenance)
                        .or_insert_with(|| (Histogram::with_buckets(BUCKETS),
                                            Histogram::with_buckets(BUCKETS),
                                            Histogram::with_buckets(BUCKETS)))
                        .1
                        .add(interesting.delta());

                    all_reductions.add(interesting.delta());
                    not_smallest_reductions.add(interesting.delta());
                    any_interesting_reductions.add(interesting.delta());
                }

                LoggerMessage::JudgedNotInteresting(_, reduction) => {
                    let provenance = reduction.provenance().to_string();
                    stats.entry(provenance)
                        .or_insert_with(|| (Histogram::with_buckets(BUCKETS),
                                            Histogram::with_buckets(BUCKETS),
                                            Histogram::with_buckets(BUCKETS)))
                        .2
                        .add(reduction.delta());

                    all_reductions.add(reduction.delta());
                    not_interesting_reductions.add(reduction.delta());
                }

                LoggerMessage::FinishedMerging(_, merged_size, upstream_size)
                    if merged_size >= upstream_size => {
                        stats.entry("merge".into())
                            .or_insert_with(|| (Histogram::with_buckets(BUCKETS),
                                                Histogram::with_buckets(BUCKETS),
                                                Histogram::with_buckets(BUCKETS)))
                            .2
                            .add(0);
                }
                _ => {}
            }
        }

        println!("Final size is {}", smallest_size);
        println!();

        let mut stats: Vec<_> = stats.into_iter().collect();
        stats.sort_by(|&(_, ref s), &(_, ref t)| {
            use std::cmp::Ordering;
            match (sum(&s.0).cmp(&sum(&t.0)), sum(&s.1).cmp(&sum(&t.1)), sum(&s.2).cmp(&sum(&t.2))) {
                // Sort by most useful to least, so invert the ordering of the
                // `not_interesting` part of the tuple.
                (Ordering::Equal, Ordering::Equal, Ordering::Equal) => Ordering::Equal,
                (Ordering::Equal, Ordering::Equal, Ordering::Less) => Ordering::Greater,
                (Ordering::Equal, Ordering::Equal, Ordering::Greater) => Ordering::Less,
                (Ordering::Equal, o, _) | (o, _, _) => o,
            }
        });
        stats.reverse();

        println!("{:=<85}", "");
        println!(
            "{:<50.50} {:>10.10}  {:>10.10}  {:>10.10}",
            "Reducer",
            "smallest",
            "intrstng",
            "not intrstng"
        );
        println!("{:-<85}", "");

        let mut total_smallest = 0;
        let mut total_not_smallest = 0;
        let mut total_not_interesting = 0;
        for &(ref reducer, (ref smallest, ref not_smallest, ref not_interesting)) in &stats {
            let smallest = sum(smallest);
            let not_smallest = sum(not_smallest);
            let not_interesting = sum(not_interesting);

            total_smallest += smallest;
            total_not_smallest += not_smallest;
            total_not_interesting += not_interesting;

            // Take the last 50 characters of the reducer name, not the first
            // 50.
            let reducer: String = reducer
                .chars()
                .rev()
                .take_while(|&c| c != '/')
                .take(50)
                .collect();
            let reducer: String = reducer.chars().rev().collect();
            println!(
                "{:<50.50} {:>10}  {:>10}  {:>10}",
                reducer,
                smallest,
                not_smallest,
                not_interesting
            );
        }

        println!("{:-<85}", "");
        println!(
            "{:<50.50} {:>10}  {:>10}  {:>10}",
            "total",
            total_smallest,
            total_not_smallest,
            total_not_interesting
        );

        println!("{:=<85}", "");
        println!("All generated reductions' delta sizes, regardless their interesting-ness:");
        println!("{}", all_reductions);

        println!("{:=<85}", "");
        println!("Interesting reductions' delta sizes, regardless whether they were smallest:");
        println!("{}", any_interesting_reductions);

        println!("{:=<85}", "");
        println!("Smallest interesting reductions' delta sizes:");
        println!("{}", smallest_reductions);

        println!("{:=<85}", "");
        println!("Interesting-but-not-smallest reductions' delta sizes:");
        println!("{}", not_smallest_reductions);

        println!("{:=<85}", "");
        println!("Not interesting reductions' delta sizes:");
        println!("{}", not_interesting_reductions);

        for (reducer, (smallest, not_smallest, not_interesting)) in stats {
            println!("{:=<85}", "");
            println!("{}: smallest interesting reductions' delta sizes:", reducer);
            println!("{}", smallest);

            println!("{:=<85}", "");
            println!("{}: interesting-but-not-smallest reductions' delta sizes:", reducer);
            println!("{}", not_smallest);

            println!("{:=<85}", "");
            println!("{}: not interesting reductions' delta sizes:", reducer);
            println!("{}", not_interesting);
        }

        println!("{:=<85}", "");
    }
}
