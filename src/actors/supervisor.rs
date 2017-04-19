//! The supervisor actor manages workers, and brokers their access to new
//! reductions.

use super::{Logger, Worker, WorkerId};
use super::super::Options;
use error;
use git;
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
    ReportInteresting(test_case::Interesting),
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
                                       run(opts, supervisor, receiver)
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
    pub fn report_interesting(&self, interesting: test_case::Interesting) {
        self.sender
            .send(SupervisorMessage::ReportInteresting(interesting))
            .unwrap();
    }
}

// Supervisor actor implementation.

fn run<I, R>(mut opts: Options<I, R>,
             me: Supervisor,
             receiver: mpsc::Receiver<SupervisorMessage>)
             -> error::Result<()>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    let logger = Logger::spawn(io::stdout());

    backup_test_case(&opts.test_case, &logger)?;

    let repodir = tempdir::TempDir::new("preduce-supervisor")?;
    let repo = git::TempRepo::new(&repodir)?;
    let mut smallest_interesting = verify_initially_interesting(&mut opts, &repo)?;
    let orig_size = smallest_interesting.size();

    let mut workers = spawn_workers(&opts, me, logger.clone(), repo.path());

    for msg in receiver {
        match msg {
            SupervisorMessage::WorkerErrored(id, err) => {
                logger.worker_errored(id, err);
                let worker = workers.remove(&id);
                assert!(worker.is_some());
                // TODO FITZGEN: spawn a new worker
            }
            SupervisorMessage::WorkerPanicked(id, panic) => {
                logger.worker_panicked(id, panic);
                let worker = workers.remove(&id);
                assert!(worker.is_some());
                // TODO FITZGEN: spawn a new worker
            }
            SupervisorMessage::RequestNextReduction(who) => {
                logger.start_generating_next_reduction();
                if let Some(reduction) = opts.reducer().next_potential_reduction()? {
                    logger.finish_generating_next_reduction();
                    who.next_reduction_response(reduction);
                } else {
                    logger.no_more_reductions();
                    let worker = workers.remove(&who.id());
                    who.shutdown();
                    assert!(worker.is_some());
                }
            }
            SupervisorMessage::ReportInteresting(interesting) => {
                let old_size = smallest_interesting.size();
                let new_size = interesting.size();
                if new_size < old_size {
                    // TODO FITZGEN: fetch the worker's repo, reset our HEAD to
                    // the worker's repo's HEAD.

                    smallest_interesting = interesting;
                    opts.reducer().set_seed(smallest_interesting.clone());

                    // TODO FITZGEN: if workers.len() < opts.num_workers(),
                    // spawn more workers.

                    fs::copy(smallest_interesting.path(), &opts.test_case)?;
                    logger.new_smallest(new_size, orig_size);
                } else {
                    logger.is_not_smaller();
                    // TODO FITZGEN: send it back to the worker to attempt to
                    // merge this interesting test case with the smallest
                    // interesting test case.
                }
            }
        }

        if workers.is_empty() {
            assert!(opts.reducer().next_potential_reduction()?.is_none());
            break;
        }
    }

    logger.final_reduced_size(smallest_interesting.size(), orig_size);
    Ok(())
}

fn verify_initially_interesting<I, R>(opts: &mut Options<I, R>,
                                      repo: &git2::Repository)
                                      -> error::Result<test_case::Interesting>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    let initial = test_case::Interesting::initial(&opts.test_case, opts.predicate(), &repo)?
        .ok_or(error::Error::InitialTestCaseNotInteresting)?;
    opts.reducer().set_seed(initial.clone());
    Ok(initial)
}

fn backup_test_case(test_case: &path::Path, logger: &Logger) -> error::Result<()> {
    let mut backup_path = path::PathBuf::from(test_case);
    let mut file_name = test_case.file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
                        let e = io::Error::new(io::ErrorKind::Other,
                                   "test case path must exist and representable in utf-8");
                        error::Error::TestCaseBackupFailure(e)
                    })?;
    file_name.push_str(".orig");
    backup_path.set_file_name(file_name);

    logger.backing_up_test_case(test_case, &backup_path);

    fs::copy(test_case, backup_path)
        .map_err(error::Error::TestCaseBackupFailure)?;

    Ok(())
}

fn spawn_workers<I, R>(opts: &Options<I, R>,
                       me: Supervisor,
                       logger: Logger,
                       upstream: &path::Path)
                       -> HashMap<WorkerId, Worker>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    (0..opts.num_workers())
        .map(|i| {
            let id = WorkerId::new(i);
            let worker = Worker::spawn(id,
                                       opts.predicate().clone(),
                                       me.clone(),
                                       logger.clone(),
                                       upstream);
            (id, worker)
        })
        .collect()
}
