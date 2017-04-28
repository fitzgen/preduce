//! The supervisor actor manages workers, and brokers their access to new
//! reductions.

use super::{Logger, Worker, WorkerId};
use super::super::Options;
use error;
use git::{self, RepoExt};
use signposts;
use std::any::Any;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path;
use std::sync::mpsc;
use std::thread;
use test_case::{self, TestCaseMethods};
use traits;

/// The messages that can be sent to the supervisor actor.
#[derive(Debug)]
enum SupervisorMessage {
    RequestNextReduction(Worker),
    WorkerPanicked(WorkerId, Box<Any + Send + 'static>),
    WorkerErrored(WorkerId, error::Error),
    ReportInteresting(Worker, path::PathBuf, test_case::Interesting),
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
    pub fn spawn<I, R>
        (opts: Options<I, R>)
         -> error::Result<(Supervisor, thread::JoinHandle<error::Result<()>>)>
        where I: 'static + traits::IsInteresting,
              R: 'static + traits::Reducer
    {
        let (sender, receiver) = mpsc::channel();
        let sender2 = sender.clone();

        let handle = thread::Builder::new()
            .name(format!("preduce-supervisor"))
            .spawn(move || {
                       let supervisor = Supervisor {
                           sender: sender2,
                       };
                       SupervisorActor::run(opts, supervisor, receiver)
                   })?;

        let sup = Supervisor {
            sender: sender,
        };

        Ok((sup, handle))
    }

    /// Request the next potentially-interesting test case reduction. The
    /// response will be sent back to the `who` worker.
    pub fn request_next_reduction(&self, who: Worker) {
        self.sender
            .send(SupervisorMessage::RequestNextReduction(who))
            .unwrap();
    }

    /// Notify the supervisor that the worker with the given id panicked.
    pub fn worker_panicked(&self,
                           id: WorkerId,
                           panic: Box<Any + Send + 'static>) {
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
    pub fn report_interesting(&self,
                              who: Worker,
                              downstream: path::PathBuf,
                              interesting: test_case::Interesting) {
        self.sender
            .send(SupervisorMessage::ReportInteresting(who, downstream, interesting))
            .unwrap();
    }
}

// Supervisor actor implementation.

struct SupervisorActor<I, R>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    opts: Options<I, R>,
    me: Supervisor,
    logger: Logger,
    logger_handle: thread::JoinHandle<()>,
    repo: git::TempRepo,
    worker_id_counter: usize,
    workers: HashMap<WorkerId, Worker>,
}

impl<I, R> SupervisorActor<I, R>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    fn run(opts: Options<I, R>,
           me: Supervisor,
           incoming: mpsc::Receiver<SupervisorMessage>)
           -> error::Result<()> {
        let repo = git::TempRepo::new("preduce-supervisor")?;

        let num_workers = opts.num_workers();
        let (logger, logger_handle) = Logger::spawn(io::stdout())?;
        let mut supervisor = SupervisorActor {
            opts: opts,
            me: me,
            logger: logger,
            logger_handle: logger_handle,
            repo: repo,
            worker_id_counter: 0,
            workers: HashMap::with_capacity(num_workers),
        };

        supervisor.backup_original_test_case()?;

        supervisor.spawn_workers()?;
        let initial_interesting = supervisor.verify_initially_interesting()?;
        supervisor.run_loop(incoming, initial_interesting)
    }

    /// Run the supervisor's main loop, serving reductions to workers, and
    /// keeping track of the globally smallest interesting test case.
    fn run_loop(mut self,
                incoming: mpsc::Receiver<SupervisorMessage>,
                initial_interesting: test_case::Interesting)
                -> error::Result<()> {
        let _signpost = signposts::SupervisorRunLoop::new();

        let mut smallest_interesting = initial_interesting;
        let orig_size = smallest_interesting.size();

        for msg in incoming {
            match msg {
                SupervisorMessage::WorkerErrored(id, err) => {
                    self.logger.worker_errored(id, err);
                    self.restart_worker(id)?;
                }

                SupervisorMessage::WorkerPanicked(id, panic) => {
                    self.logger.worker_panicked(id, panic);
                    self.restart_worker(id)?;
                }

                SupervisorMessage::RequestNextReduction(who) => {
                    self.send_next_reduction_to(who)?;
                }

                SupervisorMessage::ReportInteresting(who,
                                                     downstream,
                                                     interesting) => {
                    self.handle_new_interesting_test_case(
                            who,
                            downstream,
                            orig_size,
                            &mut smallest_interesting,
                            interesting
                        )?;
                }
            }

            if self.workers.is_empty() {
                assert!(self.opts
                            .reducer()
                            .next_potential_reduction()?
                            .is_none());
                break;
            }
        }

        self.shutdown(smallest_interesting, orig_size)
    }

    /// Consume this supervisor actor and perform shutdown.
    fn shutdown(self,
                smallest_interesting: test_case::Interesting,
                orig_size: u64)
                -> error::Result<()> {
        let _signpost = signposts::SupervisorShutdown::new();

        self.logger
            .final_reduced_size(smallest_interesting.size(), orig_size);
        drop(self.logger);
        self.logger_handle.join()?;

        println!("git log --graph");
        ::std::process::Command::new("git")
            .args(&["log", "--graph"])
            .current_dir(self.repo.path())
            .status()
            .unwrap();

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
    fn send_next_reduction_to(&mut self, who: Worker) -> error::Result<()> {
        assert!(self.workers.contains_key(&who.id()));

        let _signpost = signposts::SupervisorNextReduction::new();
        self.logger.start_generating_next_reduction();

        if let Some(reduction) = self.opts
               .reducer()
               .next_potential_reduction()? {
            self.logger.finish_generating_next_reduction();
            who.next_reduction(reduction);
        } else {
            self.logger.no_more_reductions();

            let old_worker = self.workers.remove(&who.id());
            assert!(old_worker.is_some());

            who.shutdown();
        }

        Ok(())
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
) -> error::Result<()>{
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
            self.logger.new_smallest(new_size, orig_size);

            // Second, reset our repo's HEAD to the new interesting test case's
            // commit.
            self.repo
                .fetch_and_reset_hard(downstream,
                                      smallest_interesting.commit_id())?;

            // Third, re-seed our reducer with the new test case, send new work
            // to the reporting worker, and respawn any workers that might have
            // shutdown because we exhausted all possible reductions on the
            // previous smallest interesting test case.
            self.opts
                .reducer()
                .set_seed(smallest_interesting.clone());
            self.send_next_reduction_to(who)?;
            self.spawn_workers()?;
        } else {
            // Although the test case is interesting, it is not smaller. Tell
            // the worker to try and merge it with our current smallest
            // interesting test case and see if that is also interesting and
            // even smaller. Unless it is already a merge, in which case, we
            // abandon this thread of traversal.
            self.logger.is_not_smaller();
            if !self.opts.should_try_merging() || interesting.provenance() == Some("merge") {
                self.send_next_reduction_to(who)?;
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
            .ok_or_else(
                || {
                    let e = io::Error::new(
                        io::ErrorKind::Other,
                        "test case path must exist and representable in utf-8"
                    );
                    error::Error::TestCaseBackupFailure(e)
                }
            )?;
        file_name.push_str(".orig");
        backup_path.set_file_name(file_name);

        self.logger
            .backing_up_test_case(&self.opts.test_case, &backup_path);

        fs::copy(&self.opts.test_case, backup_path)
            .map_err(error::Error::TestCaseBackupFailure)?;

        Ok(())
    }

    /// Verify that the initial, unreduced test case is itself interesting.
    fn verify_initially_interesting
        (&mut self)
         -> error::Result<test_case::Interesting> {
        let initial = test_case::Interesting::initial(&self.opts.test_case,
                                                      self.opts.predicate(),
                                                      &self.repo)?;
        let initial = initial
            .ok_or(error::Error::InitialTestCaseNotInteresting)?;
        self.opts.reducer().set_seed(initial.clone());
        Ok(initial)
    }

    /// Spawn (or re-spawn) workers until we have the number of active,
    /// concurrent workers originally requested in the `Options`.
    fn spawn_workers(&mut self) -> error::Result<()> {
        assert!(self.workers.len() <= self.opts.num_workers());

        let new_workers: error::Result<Vec<_>> = (self.workers.len()..
                                                  self.opts.num_workers())
                .map(|_| {
                    let id = WorkerId::new(self.worker_id_counter);
                    self.worker_id_counter += 1;

                    let worker = Worker::spawn(id,
                                               self.opts.predicate().clone(),
                                               self.me.clone(),
                                               self.logger.clone(),
                                               self.repo.path())?;
                    Ok((id, worker))
                })
                .collect();
        let new_workers = new_workers?;
        self.workers.extend(new_workers);
        Ok(())
    }
}
