//! The logger actor receives log messages and writes them to a log file.

use super::WorkerId;
use error;
use git2;
use std::any::Any;
use std::fmt;
use std::io::Write;
use std::path;
use std::sync::mpsc;
use std::thread;

/// The different kinds of log messages that can be sent to the logger actor.
#[derive(Debug)]
enum LoggerMessage {
    SpawningWorker(WorkerId),
    SpawnedWorker(WorkerId),
    ShutdownWorker(WorkerId),
    WorkerPanicked(WorkerId, Box<Any + Send + 'static>),
    WorkerErrored(WorkerId, error::Error),
    BackingUpTestCase(String, String),
    StartJudgingInteresting(WorkerId),
    JudgedInteresting(WorkerId, u64),
    JudgedNotInteresting(WorkerId),
    NewSmallest(u64, u64),
    IsNotSmaller,
    StartGeneratingNextReduction,
    FinishGeneratingNextReduction,
    NoMoreReductions,
    FinalReducedSize(u64, u64),
    TryMerge(WorkerId, git2::Oid, git2::Oid),
    FinishedMerging(WorkerId, u64, u64),
}

impl fmt::Display for LoggerMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            LoggerMessage::SpawningWorker(id) => write!(f, "Supervisor: Spawning worker {}", id),
            LoggerMessage::SpawnedWorker(id) => write!(f, "Worker {}: spawned", id),
            LoggerMessage::ShutdownWorker(id) => write!(f, "Worker {}: shutting down", id),
            LoggerMessage::WorkerErrored(id, ref err) => write!(f, "Worker {}: error: {}", id, err),
            LoggerMessage::WorkerPanicked(id, _) => write!(f, "Worker {}: panicked!", id),
            LoggerMessage::BackingUpTestCase(ref from, ref to) => {
                write!(
                    f,
                    "Supervisor: backing up initial test case from {} to {}",
                    from,
                    to
                )
            }
            LoggerMessage::StartJudgingInteresting(id) => {
                write!(
                    f,
                    "Worker {}: judging a test case's interesting-ness...",
                    id
                )
            }
            LoggerMessage::JudgedInteresting(id, size) => {
                write!(
                    f,
                    "Worker {}: found an interesting test case of size {} bytes",
                    id,
                    size
                )
            }
            LoggerMessage::JudgedNotInteresting(id) => {
                write!(f, "Worker {}: found test case not interesting", id)
            }
            LoggerMessage::NewSmallest(new_size, orig_size) => {
                assert!(new_size < orig_size);
                assert!(orig_size != 0);
                let percent = ((orig_size - new_size) as f64) / (orig_size as f64) * 100.0;
                write!(
                    f,
                    "Supervisor: new smallest interesting test case: {} bytes ({:.2}% reduced)",
                    new_size,
                    percent
                )
            }
            LoggerMessage::IsNotSmaller => {
                write!(
                    f,
                    "Supervisor: interesting test case is not new smallest; tell worker to try merging"
                )
            }
            LoggerMessage::StartGeneratingNextReduction => {
                write!(f, "Supervisor: generating next reduction...")
            }
            LoggerMessage::FinishGeneratingNextReduction => {
                write!(f, "Supervisor: finished generating next reduction")
            }
            LoggerMessage::NoMoreReductions => write!(f, "Supervisor: no more reductions"),
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
                write!(f, "Worker {}: trying to merge upstream's {} into our {}", id, upstream_commit, worker_commit)
            }
            LoggerMessage::FinishedMerging(id, merged_size, upstream_size) => {
                write!(f, "Worker {}: finished merging; merged size is {}, upstream size is {}", id, merged_size, upstream_size)
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
        let handle = thread::Builder::new().name("preduce-logger".into()).spawn(move || Logger::run(to, receiver))?;
        Ok((Logger { sender: sender }, handle))
    }

    /// Log the start of spawning a worker.
    pub fn spawning_worker(&self, id: WorkerId) {
        self.sender
            .send(LoggerMessage::SpawningWorker(id))
            .unwrap();
    }

    /// Log the end of spawning a worker.
    pub fn spawned_worker(&self, id: WorkerId) {
        self.sender
            .send(LoggerMessage::SpawnedWorker(id))
            .unwrap();
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
        self.sender
            .send(LoggerMessage::ShutdownWorker(id))
            .unwrap();
    }

    /// Log that the worker with the given id is shutting down.
    pub fn worker_errored(&self, id: WorkerId, err: error::Error) {
        self.sender
            .send(LoggerMessage::WorkerErrored(id, err))
            .unwrap();
    }

    /// Log that the worker with the given id is shutting down.
    pub fn worker_panicked(&self, id: WorkerId, panic: Box<Any + Send + 'static>) {
        self.sender
            .send(LoggerMessage::WorkerPanicked(id, panic))
            .unwrap();
    }

    /// Log that the worker with the given id has started running an
    /// is-interesting predicate on its test case.
    pub fn start_judging_interesting(&self, id: WorkerId) {
        self.sender
            .send(LoggerMessage::StartJudgingInteresting(id))
            .unwrap();
    }

    /// Log that the worker with the given id has discovered a new interesting
    /// test case.
    pub fn judged_interesting(&self, id: WorkerId, size: u64) {
        self.sender
            .send(LoggerMessage::JudgedInteresting(id, size))
            .unwrap();
    }

    /// Log that the worker with the given id has discovered that its test case
    /// is not interesting.
    pub fn judged_not_interesting(&self, id: WorkerId) {
        self.sender
            .send(LoggerMessage::JudgedNotInteresting(id))
            .unwrap();
    }

    /// Log that the supervisor has a new globally smallest interesting test
    /// case.
    pub fn new_smallest(&self, new_size: u64, orig_size: u64) {
        assert!(new_size < orig_size);
        assert!(orig_size != 0);
        self.sender
            .send(LoggerMessage::NewSmallest(new_size, orig_size))
            .unwrap();
    }

    /// Log that the supervisor received a new interesting test case, but that
    /// it is not smaller than the current globally smallest interesting test
    /// case.
    pub fn is_not_smaller(&self) {
        self.sender.send(LoggerMessage::IsNotSmaller).unwrap();
    }

    /// Log that the supervisor has started generating the next potential
    /// reduction.
    pub fn start_generating_next_reduction(&self) {
        self.sender
            .send(LoggerMessage::StartGeneratingNextReduction)
            .unwrap();
    }

    /// Log that the supervisor has completed generating the next potential
    /// reduction.
    pub fn finish_generating_next_reduction(&self) {
        self.sender
            .send(LoggerMessage::FinishGeneratingNextReduction)
            .unwrap();
    }

    /// Log that the supervisor has exhuasted potential reductions for the
    /// current globally smallest interesting test case.
    pub fn no_more_reductions(&self) {
        self.sender
            .send(LoggerMessage::NoMoreReductions)
            .unwrap();
    }

    /// Log the final reduced test case's size once the reduction process has
    /// completed.
    pub fn final_reduced_size(&self, final_size: u64, orig_size: u64) {
        assert!(final_size <= orig_size);
        self.sender
            .send(LoggerMessage::FinalReducedSize(final_size, orig_size))
            .unwrap();
    }

    /// Log that the worker with the given id is attempting a merge.
    pub fn try_merging(&self, id: WorkerId, upstream_commit: git2::Oid, worker_commit: git2::Oid) {
        self.sender.send(LoggerMessage::TryMerge(id, upstream_commit, worker_commit)).unwrap();
    }

    /// Log that the worker with the given id is attempting a merge.
    pub fn finished_merging(&self, id: WorkerId, merged_size: u64, upstream_size: u64) {
        self.sender.send(LoggerMessage::FinishedMerging(id, merged_size, upstream_size)).unwrap();
    }
}

/// Logger actor implementation.
impl Logger {
    fn run<W>(mut to: W, incoming: mpsc::Receiver<LoggerMessage>)
    where
        W: Write,
    {
        for log_msg in incoming {
            writeln!(&mut to, "{}", log_msg).expect("Should write to log file");
        }
    }
}
