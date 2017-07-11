//! The supervisor actor manages workers, and brokers their access to new
//! reductions.

use super::{Logger, Reducer, ReducerId, Worker, WorkerId};
use super::super::Options;
use error;
use git::{self, RepoExt};
use signposts;
use std::any::Any;
use std::cmp;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{self, Read};
use std::path;
use std::sync::mpsc;
use std::thread;
use test_case::{self, TestCaseMethods};
use traits;

/// The messages that can be sent to the supervisor actor.
#[derive(Debug)]
enum SupervisorMessage {
    // From workers.
    WorkerPanicked(WorkerId, Box<Any + Send + 'static>),
    WorkerErrored(WorkerId, error::Error),
    RequestNextReduction(Worker),
    ReportInteresting(Worker, path::PathBuf, test_case::Interesting),

    // From reducers.
    ReducerPanicked(ReducerId, Box<Any + Send + 'static>),
    ReplyNextReduction(Reducer, Option<test_case::PotentialReduction>),
}

/// A client handle to the supervisor actor.
#[derive(Clone, Debug)]
pub struct Supervisor {
    sender: mpsc::Sender<SupervisorMessage>,
}

/// Supervisor client API.
impl Supervisor {
    /// Spawn the supervisor thread, which will in turn spawn workers and start
    /// the test case reduction process.
    pub fn spawn<I>(
        opts: Options<I>,
    ) -> error::Result<(Supervisor, thread::JoinHandle<error::Result<()>>)>
    where
        I: 'static + traits::IsInteresting,
    {
        let (sender, receiver) = mpsc::channel();
        let sender2 = sender.clone();

        let handle = thread::Builder::new()
            .name(format!("preduce-supervisor"))
            .spawn(move || {
                let supervisor = Supervisor { sender: sender2 };
                SupervisorActor::run(opts, supervisor, receiver)
            })?;

        let sup = Supervisor { sender: sender };

        Ok((sup, handle))
    }

    // Messages sent to the supervisor from the workers.

    /// Request the next potentially-interesting test case reduction. The
    /// response will be sent back to the `who` worker.
    pub fn request_next_reduction(&self, who: Worker) {
        self.sender
            .send(SupervisorMessage::RequestNextReduction(who))
            .unwrap();
    }

    /// Notify the supervisor that the worker with the given id panicked.
    pub fn worker_panicked(&self, id: WorkerId, panic: Box<Any + Send + 'static>) {
        self.sender
            .send(SupervisorMessage::WorkerPanicked(id, panic))
            .unwrap();
    }

    /// Notify the supervisor that the worker with the given id errored out.
    pub fn worker_errored(&self, id: WorkerId, err: error::Error) {
        self.sender
            .send(SupervisorMessage::WorkerErrored(id, err))
            .unwrap();
    }

    /// Notify the supervisor that the given test case has been found to be
    /// interesting.
    pub fn report_interesting(
        &self,
        who: Worker,
        downstream: path::PathBuf,
        interesting: test_case::Interesting,
    ) {
        self.sender
            .send(SupervisorMessage::ReportInteresting(
                who,
                downstream,
                interesting,
            ))
            .unwrap();
    }

    // Messages sent to the supervisor from the reducer actors.

    /// Notify the supervisor that the reducer with the given id panicked.
    pub fn reducer_panicked(&self, id: ReducerId, panic: Box<Any + Send + 'static>) {
        self.sender
            .send(SupervisorMessage::ReducerPanicked(id, panic))
            .unwrap();
    }

    /// Tell the supervisor that there are no more reductions of the current
    /// test case.
    pub fn no_more_reductions(&self, reducer: Reducer) {
        self.sender
            .send(SupervisorMessage::ReplyNextReduction(reducer, None))
            .unwrap();
    }

    /// Give the supervisor the requested next reduction of the current test
    /// case.
    pub fn reply_next_reduction(&self, reducer: Reducer, reduction: test_case::PotentialReduction) {
        self.sender
            .send(SupervisorMessage::ReplyNextReduction(
                reducer,
                Some(reduction),
            ))
            .unwrap();
    }
}

// Supervisor actor implementation.

struct SupervisorActor<I>
where
    I: 'static + traits::IsInteresting,
{
    opts: Options<I>,
    me: Supervisor,
    logger: Logger,
    logger_handle: thread::JoinHandle<()>,
    repo: git::TempRepo,
    worker_id_counter: usize,
    workers: HashMap<WorkerId, Worker>,
    idle_workers: Vec<Worker>,
    reducers: HashMap<ReducerId, Reducer>,
    exhausted_reducers: HashSet<ReducerId>,
    reduction_queue: VecDeque<(test_case::PotentialReduction, ReducerId)>,
}

impl<I> SupervisorActor<I>
where
    I: 'static + traits::IsInteresting,
{
    fn run(
        opts: Options<I>,
        me: Supervisor,
        incoming: mpsc::Receiver<SupervisorMessage>,
    ) -> error::Result<()> {
        let repo = git::TempRepo::new("preduce-supervisor")?;

        let num_workers = opts.num_workers();
        let num_reducers = opts.reducers().len();
        let (logger, logger_handle) = Logger::spawn(fs::File::create("preduce.log")?)?;

        let mut supervisor = SupervisorActor {
            opts: opts,
            me: me,
            logger: logger,
            logger_handle: logger_handle,
            repo: repo,
            worker_id_counter: 0,
            workers: HashMap::with_capacity(num_workers),
            idle_workers: Vec::with_capacity(num_workers),
            reducers: HashMap::with_capacity(num_reducers),
            exhausted_reducers: HashSet::with_capacity(num_reducers),
            reduction_queue: VecDeque::with_capacity(num_reducers),
        };

        supervisor.backup_original_test_case()?;
        supervisor.spawn_reducers()?;
        supervisor.spawn_workers()?;

        let initial_interesting = supervisor.verify_initially_interesting()?;
        supervisor.reseed_reducers(&initial_interesting);
        supervisor.run_loop(incoming, initial_interesting)
    }

    /// Run the supervisor's main loop, serving reductions to workers, and
    /// keeping track of the globally smallest interesting test case.
    fn run_loop(
        mut self,
        incoming: mpsc::Receiver<SupervisorMessage>,
        initial_interesting: test_case::Interesting,
    ) -> error::Result<()> {
        let _signpost = signposts::SupervisorRunLoop::new();

        let mut smallest_interesting = initial_interesting;
        let orig_size = smallest_interesting.size();

        for msg in incoming {
            match msg {
                // Messages from workers...
                SupervisorMessage::WorkerErrored(id, err) => {
                    self.logger.worker_errored(id, err);
                    self.restart_worker(id)?;
                }

                SupervisorMessage::WorkerPanicked(id, panic) => {
                    self.logger.worker_panicked(id, panic);
                    self.restart_worker(id)?;
                }

                SupervisorMessage::RequestNextReduction(who) => {
                    self.enqueue_worker_for_reduction(who);
                }

                SupervisorMessage::ReportInteresting(who, downstream, interesting) => {
                    self.handle_new_interesting_test_case(
                        who,
                        downstream,
                        orig_size,
                        &mut smallest_interesting,
                        interesting,
                    )?;
                }

                // Messages from reducer actors...

                // FIXME: Unlike workers, we don't currently have an easy way to
                // restart reducers, so treat reducer failures as fatal. There
                // isn't any inherent reason why we can't restart reducers,
                // however, we just need to write the glue code that clones
                // `Reducer` trait objects and all of that.
                SupervisorMessage::ReducerPanicked(id, panic) => {
                    self.logger.reducer_panicked(id, panic);
                    return Err(error::Error::ReducerActorPanicked);
                }

                SupervisorMessage::ReplyNextReduction(reducer, None) => {
                    assert!(self.reducers.contains_key(&reducer.id()));
                    self.exhausted_reducers.insert(reducer.id());
                }

                SupervisorMessage::ReplyNextReduction(reducer, Some(reduction)) => {
                    assert!(self.reducers.contains_key(&reducer.id()));

                    if reduction.size() < smallest_interesting.size() {
                        self.reduction_queue.push_back((reduction, reducer.id()));
                        self.drain_queues();
                    } else {
                        reducer.request_next_reduction();
                    }
                }
            }

            // If all of our reducers are exhausted, and we are out of potential
            // reductions to test, then shutdown any idle workers, since we
            // don't have any work for them.
            if self.exhausted_reducers.len() == self.reducers.len() &&
                self.reduction_queue.is_empty()
            {
                for worker in self.idle_workers.drain(..) {
                    self.workers.remove(&worker.id());
                    worker.shutdown();
                }
            }

            if self.workers.is_empty() {
                break;
            }
        }

        self.shutdown(smallest_interesting, orig_size)
    }

    /// Consume this supervisor actor and perform shutdown.
    fn shutdown(
        self,
        smallest_interesting: test_case::Interesting,
        orig_size: u64,
    ) -> error::Result<()> {
        assert!(self.workers.is_empty());
        assert!(self.reduction_queue.is_empty());
        assert_eq!(self.exhausted_reducers.len(), self.reducers.len());

        let _signpost = signposts::SupervisorShutdown::new();

        self.logger
            .final_reduced_size(smallest_interesting.size(), orig_size);

        // Tell all the reducer actors to shutdown, and then wait for them
        // finish their cleanup by joining the logger thread, which exits once
        // log messages can no longer be sent to it.
        for (_, r) in self.reducers {
            r.shutdown();
        }
        drop(self.logger);
        self.logger_handle.join()?;

        // Print how we got here.
        println!("git log --graph");
        ::std::process::Command::new("git")
            .args(&["log", "--graph"])
            .current_dir(self.repo.path())
            .status()
            .unwrap();
        println!(
            "====================================================================================="
        );

        // If the final, smallest interesting test case is small enough and its
        // contents are UTF-8, then print it to stdout.
        const TOO_BIG_TO_PRINT: u64 = 4096;
        let final_size = smallest_interesting.size();
        if final_size < TOO_BIG_TO_PRINT {
            let mut contents = String::with_capacity(final_size as usize);
            let mut file = fs::File::open(smallest_interesting.path())?;
            if let Ok(_) = file.read_to_string(&mut contents) {
                println!("{}", contents);
            }
        }

        Ok(())
    }

    /// Given that the worker with the given id panicked or errored out, clean
    /// up after it and spawn a replacement for it.
    fn restart_worker(&mut self, id: WorkerId) -> error::Result<()> {
        let old_worker = self.workers.remove(&id);
        assert!(old_worker.is_some());

        self.spawn_workers()
    }

    /// Generate the next reduction and send it to the given worker, or shutdown
    /// the worker if our reducer is exhausted.
    fn enqueue_worker_for_reduction(&mut self, who: Worker) {
        assert!(self.workers.contains_key(&who.id()));

        self.idle_workers.push(who);
        self.drain_queues();
    }

    /// Given that either we've generated new potential reductions to test, or a
    /// worker just became ready to test queued reductions, dispatch as many
    /// reductions to workers as possible.
    fn drain_queues(&mut self) {
        assert!(
            self.idle_workers.len() > 0 || self.reduction_queue.len() > 0,
            "Should only call drain_queues when we have potential to do new work"
        );

        let num_to_drain = cmp::min(self.idle_workers.len(), self.reduction_queue.len());
        let workers = self.idle_workers.drain(..num_to_drain);
        let reductions = self.reduction_queue.drain(..num_to_drain);

        for (worker, (reduction, reducer_id)) in workers.zip(reductions) {
            assert!(self.workers.contains_key(&worker.id()));
            assert!(self.reducers.contains_key(&reducer_id));

            // Send the worker the next reduction from the queue to test for
            // interestingness.
            worker.next_reduction(reduction);

            // And pipeline the worker's is-interesting test with generating the
            // next reduction.
            if !self.exhausted_reducers.contains(&reducer_id) {
                self.reducers[&reducer_id].request_next_reduction();
            }
        }
    }

    /// Given that the `who` worker just found a new interesting test case,
    /// either update our globally smallest interesting test case, or tell the
    /// worker to try and merge its test case with our smaller test case, and
    /// retest for interesting-ness.
    fn handle_new_interesting_test_case(
        &mut self,
        who: Worker,
        downstream: path::PathBuf,
        orig_size: u64,
        smallest_interesting: &mut test_case::Interesting,
        interesting: test_case::Interesting,
    ) -> error::Result<()> {
        let _signpost = signposts::SupervisorHandleInteresting::new();

        let new_size = interesting.size();
        let old_size = smallest_interesting.size();

        if new_size < old_size {
            // We have a new globally smallest insteresting test case! First,
            // update the original test case file with the new interesting
            // reduction. The reduction process can take a LONG time, and if the
            // computation is interrupted for whatever reason, we DO NOT want to
            // lose this incremental progress!
            *smallest_interesting = interesting;
            fs::copy(smallest_interesting.path(), &self.opts.test_case)?;
            let provenance = smallest_interesting.provenance().into();
            self.logger.new_smallest(new_size, orig_size, provenance);

            // Second, reset our repo's HEAD to the new interesting test case's
            // commit.
            self.repo
                .fetch_and_reset_hard(downstream, smallest_interesting.commit_id())?;

            // Third, re-seed our reducer actors with the new test case, and
            // respawn any workers that might have shutdown because we exhausted
            // all possible reductions on the previous smallest interesting test
            // case.
            self.reseed_reducers(smallest_interesting);
            self.spawn_workers()?;

            // Fourth, clear out any queued potential reductions that are larger
            // than our new smallest interesting test case. We don't want to
            // waste time on them. For any reduction we don't end up
            // considering, tell its progenitor to generate its next reduction
            // from the new seed.
            {
                let reducers = &self.reducers;
                self.reduction_queue.retain(|&(ref reduction, reducer_id)| {
                    if reduction.size() < new_size {
                        return true;
                    }

                    reducers[&reducer_id].request_next_reduction();
                    false
                });
            }

            // Finaly send a new reduction to the worker that reported the new
            // smallest test case.
            self.enqueue_worker_for_reduction(who);
        } else {
            // Although the test case is interesting, it is not smaller. Tell
            // the worker to try and merge it with our current smallest
            // interesting test case and see if that is also interesting and
            // even smaller. Unless it is already a merge, in which case, we
            // abandon this thread of traversal.
            self.logger.is_not_smaller(interesting.provenance().into());
            if !self.opts.should_try_merging() || interesting.provenance() == "merge" {
                self.enqueue_worker_for_reduction(who);
            } else {
                who.try_merge(old_size, self.repo.head_id()?);
            }
        }

        Ok(())
    }

    /// Backup the original test case, just in case something goes wrong, or it
    /// is needed again to reduce a different issue from the one we're currently
    /// reducing, or...
    fn backup_original_test_case(&self) -> error::Result<()> {
        let mut backup_path = path::PathBuf::from(&self.opts.test_case);
        let mut file_name = self.opts
            .test_case
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                let e = io::Error::new(
                    io::ErrorKind::Other,
                    "test case path must exist and representable in utf-8",
                );
                error::Error::TestCaseBackupFailure(e)
            })?;
        file_name.push_str(".orig");
        backup_path.set_file_name(file_name);

        self.logger
            .backing_up_test_case(&self.opts.test_case, &backup_path);

        fs::copy(&self.opts.test_case, backup_path)
            .map_err(error::Error::TestCaseBackupFailure)?;

        Ok(())
    }

    /// Verify that the initial, unreduced test case is itself interesting.
    fn verify_initially_interesting(&mut self) -> error::Result<test_case::Interesting> {
        let initial = test_case::Interesting::initial(
            &self.opts.test_case,
            self.opts.predicate(),
            &self.repo,
        )?;
        let initial = initial.ok_or(error::Error::InitialTestCaseNotInteresting)?;
        Ok(initial)
    }

    /// Spawn (or re-spawn) workers until we have the number of active,
    /// concurrent workers originally requested in the `Options`.
    fn spawn_workers(&mut self) -> error::Result<()> {
        assert!(self.workers.len() <= self.opts.num_workers());

        let new_workers: error::Result<Vec<_>> = (self.workers.len()..self.opts.num_workers())
            .map(|_| {
                let id = WorkerId::new(self.worker_id_counter);
                self.worker_id_counter += 1;

                let worker = Worker::spawn(
                    id,
                    self.opts.predicate().clone(),
                    self.me.clone(),
                    self.logger.clone(),
                    self.repo.path(),
                )?;
                Ok((id, worker))
            })
            .collect();
        let new_workers = new_workers?;
        self.workers.extend(new_workers);
        Ok(())
    }

    /// Spawn a reducer actor for each reducer given to us in the options.
    fn spawn_reducers(&mut self) -> error::Result<()> {
        let reducers = self.opts.take_reducers();
        for (i, reducer) in reducers.into_iter().enumerate() {
            let id = ReducerId::new(i);
            let reducer_actor = Reducer::spawn(id, reducer, self.me.clone(), self.logger.clone())?;
            self.reducers.insert(id, reducer_actor);
            self.exhausted_reducers.insert(id);
        }
        Ok(())
    }

    /// Reseed each of the reducer actors with the new smallest interesting test
    /// case.
    fn reseed_reducers(&mut self, smallest_interesting: &test_case::Interesting) {
        for (id, reducer_actor) in &self.reducers {
            reducer_actor.set_new_seed(smallest_interesting.clone());

            // If the reducer was exhausted, put it back to work again by
            // requesting the next reduction. If it isn't exhausted, then we
            // will request its next reduction after we pull its most recently
            // generated (or currently being generated) reduction from the
            // reduction queue.
            if self.exhausted_reducers.contains(id) {
                reducer_actor.request_next_reduction();
                self.exhausted_reducers.remove(id);
            }
        }
    }
}
