//! The supervisor actor manages workers, and brokers their access to new
//! reductions.

use super::{Logger, Worker, WorkerId};
use super::super::Options;
use error;
use git::{self, RepoExt};
use git2;
use std::any::Any;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path;
use std::sync::mpsc;
use std::thread;
use tempdir;
use test_case::{self, TestCaseMethods};
use traits;

/// The messages that can be sent to the supervisor actor.
#[derive(Debug)]
enum SupervisorMessage {
    RequestNextReduction(Worker),
    WorkerPanicked(WorkerId, Box<Any + Send + 'static>),
    WorkerErrored(WorkerId, error::Error),
    ReportInteresting(Worker, test_case::Interesting),
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
    pub fn spawn<I, R>(opts: Options<I, R>) -> (Supervisor, thread::JoinHandle<error::Result<()>>)
        where I: 'static + traits::IsInteresting,
              R: 'static + traits::Reducer
    {
        let (sender, receiver) = mpsc::channel();
        let sender2 = sender.clone();

        let handle = thread::spawn(move || {
                                       let supervisor = Supervisor { sender: sender2 };
                                       SupervisorActor::run(opts, supervisor, receiver)
                                   });

        let sup = Supervisor { sender: sender };

        (sup, handle)
    }

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
    pub fn report_interesting(&self, who: Worker, interesting: test_case::Interesting) {
        self.sender
            .send(SupervisorMessage::ReportInteresting(who, interesting))
            .unwrap();
    }
}

// Supervisor actor implementation.

struct SupervisorActor<'a, I, R>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    opts: Options<I, R>,
    me: Supervisor,
    logger: Logger,
    repo: git::TempRepo<'a>,
    worker_id_counter: usize,
    workers: HashMap<WorkerId, Worker>,
}

impl<'a, I, R> SupervisorActor<'a, I, R>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    fn run(opts: Options<I, R>,
           me: Supervisor,
           incoming: mpsc::Receiver<SupervisorMessage>)
           -> error::Result<()> {
        let repodir = tempdir::TempDir::new("preduce-supervisor")?;
        let repo = git::TempRepo::new(&repodir)?;

        let num_workers = opts.num_workers();
        let mut supervisor = SupervisorActor {
            opts: opts,
            me: me,
            logger: Logger::spawn(io::stdout()),
            repo: repo,
            worker_id_counter: 0,
            workers: HashMap::with_capacity(num_workers),
        };

        supervisor.backup_original_test_case()?;

        supervisor.spawn_workers();
        let initial_interesting = supervisor.verify_initially_interesting()?;
        supervisor.run_loop(incoming, initial_interesting)
    }

    /// TODO FITZGEN
    fn run_loop(&mut self,
                incoming: mpsc::Receiver<SupervisorMessage>,
                initial_interesting: test_case::Interesting)
                -> error::Result<()> {
        let mut smallest_interesting = initial_interesting;
        let orig_size = smallest_interesting.size();

        for msg in incoming {
            match msg {
                SupervisorMessage::WorkerErrored(id, err) => {
                    self.logger.worker_errored(id, err);
                    self.restart_worker(id);
                }

                SupervisorMessage::WorkerPanicked(id, panic) => {
                    self.logger.worker_panicked(id, panic);
                    self.restart_worker(id);
                }

                SupervisorMessage::RequestNextReduction(who) => {
                    self.send_next_reduction_to(who)?;
                }

                SupervisorMessage::ReportInteresting(who, interesting) => {
                    self.handle_new_interesting_test_case(who,
                                                          orig_size,
                                                          &mut smallest_interesting,
                                                          interesting)?;
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

        self.logger
            .final_reduced_size(smallest_interesting.size(), orig_size);
        Ok(())
    }

    /// TODO FITZGEN
    fn restart_worker(&mut self, id: WorkerId) {
        let old_worker = self.workers.remove(&id);
        assert!(old_worker.is_some());

        self.spawn_workers();
    }

    /// TODO FITZGEN
    fn send_next_reduction_to(&mut self, who: Worker) -> error::Result<()> {
        assert!(self.workers.contains_key(&who.id()));
        self.logger.start_generating_next_reduction();

        if let Some(reduction) = self.opts.reducer().next_potential_reduction()? {
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

    /// TODO FITZGEN
    fn handle_new_interesting_test_case(&mut self,
                                        who: Worker,
                                        orig_size: u64,
                                        smallest_interesting: &mut test_case::Interesting,
                                        interesting: test_case::Interesting)
                                        -> error::Result<()> {
        let old_size = smallest_interesting.size();
        let new_size = interesting.size();

        if new_size < old_size {
            {
                // TODO FITZGEN: fetch the worker's repo, reset our HEAD to the
                // worker's repo's HEAD.
                let remote = interesting.repo_path();
                let remote = remote.to_string_lossy();
                let remote = self.repo.remote_anonymous(&remote)?;
                // TODO FITZGEN
            }

            *smallest_interesting = interesting;
            self.opts
                .reducer()
                .set_seed(smallest_interesting.clone());
            fs::copy(smallest_interesting.path(), &self.opts.test_case)?;
            self.logger.new_smallest(new_size, orig_size);

            self.send_next_reduction_to(who)?;
            self.spawn_workers();
        } else {
            // Although the test case is interesting, it is not smaller. Tell
            // the worker to try and merge it with our current smallest
            // interesting test case and see if that is also interesting and
            // even smaller.
            self.logger.is_not_smaller();
            who.try_merge(old_size, self.repo.head_id()?);
        }

        Ok(())
    }

    /// TODO FITZGEN
    fn backup_original_test_case(&self) -> error::Result<()> {
        let mut backup_path = path::PathBuf::from(&self.opts.test_case);
        let mut file_name = self.opts.test_case.file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                let e = io::Error::new(io::ErrorKind::Other,
                                       "test case path must exist and representable in utf-8");
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

    /// TODO FITZGEN
    fn verify_initially_interesting(&mut self) -> error::Result<test_case::Interesting> {
        let initial = test_case::Interesting::initial(&self.opts.test_case,
                                                      self.opts.predicate(),
                                                      &self.repo)?
                .ok_or(error::Error::InitialTestCaseNotInteresting)?;
        self.opts.reducer().set_seed(initial.clone());
        Ok(initial)
    }

    /// TODO FITZGEN
    fn spawn_workers(&mut self) {
        let new_workers: Vec<_> = (self.workers.len()..self.opts.num_workers())
            .map(|_| {
                let id = WorkerId::new(self.worker_id_counter);
                self.worker_id_counter += 1;

                let worker = Worker::spawn(id,
                                           self.opts.predicate().clone(),
                                           self.me.clone(),
                                           self.logger.clone(),
                                           self.repo.path());
                (id, worker)
            })
            .collect();
        self.workers.extend(new_workers);
    }
}
