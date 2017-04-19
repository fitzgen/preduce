//! The worker actor pulls potentially interesting test cases from the
//! supervisor and tests them for interestingness.

use super::{Logger, Supervisor};
use error;
use git;
use std::fmt;
use std::panic;
use std::path;
use std::sync::mpsc;
use std::thread;
use tempdir;
use test_case::{self, TestCaseMethods};
use traits;

/// An identifier for a worker actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkerId(usize);

impl WorkerId {
    pub fn new(id: usize) -> WorkerId {
        WorkerId(id)
    }
}

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Messages that can be sent to worker actors.
#[derive(Debug)]
enum WorkerMessage {
    NextReductionResponse(test_case::PotentialReduction),
    Shutdown,
}

/// A client handle to a worker actor.
#[derive(Clone, Debug)]
pub struct Worker {
    id: WorkerId,
    sender: mpsc::Sender<WorkerMessage>,
}

/// Worker client API.
impl Worker {
    /// Spawn a new worker actor.
    pub fn spawn(id: WorkerId,
                 predicate: Box<traits::IsInteresting>,
                 supervisor: Supervisor,
                 logger: Logger,
                 upstream: &path::Path)
                 -> Self {
        logger.spawning_worker(id);

        let upstream = upstream.into();
        let (sender, receiver) = mpsc::channel();

        let me = Worker {
            id: id,
            sender: sender,
        };
        let me2 = me.clone();

        thread::spawn(move || { run(id, me2, predicate, receiver, supervisor, logger, upstream); });

        me
    }

    /// Get the id of this worker.
    pub fn id(&self) -> WorkerId {
        self.id
    }

    /// Tell this worker to shutdown.
    pub fn shutdown(self) {
        self.sender.send(WorkerMessage::Shutdown).unwrap();
    }

    /// Send the worker the response to its request for another potential
    /// reduction.
    pub fn next_reduction_response(&self, reduction: test_case::PotentialReduction) {
        self.sender
            .send(WorkerMessage::NextReductionResponse(reduction))
            .unwrap();
    }
}

// Worker actor implementation.

fn run(id: WorkerId,
       me: Worker,
       predicate: Box<traits::IsInteresting>,
       incoming: mpsc::Receiver<WorkerMessage>,
       supervisor: Supervisor,
       logger: Logger,
       upstream: path::PathBuf) {
    match {
              let supervisor2 = supervisor.clone();
              let logger2 = logger.clone();
              panic::catch_unwind(panic::AssertUnwindSafe(move || {
            try_run(id, me, predicate, incoming, supervisor2, logger2, upstream)
        }))
          } {
        Err(p) => {
            supervisor.worker_panicked(id, p);
        }
        Ok(Err(e)) => {
            supervisor.worker_errored(id, e);
        }
        Ok(Ok(())) => {}
    }
}

fn try_run(id: WorkerId,
           me: Worker,
           predicate: Box<traits::IsInteresting>,
           incoming: mpsc::Receiver<WorkerMessage>,
           supervisor: Supervisor,
           logger: Logger,
           upstream: path::PathBuf)
           -> error::Result<()> {
    logger.spawned_worker(id);

    let prefix = format!("preduce-worker-{}", id);
    let tempdir = tempdir::TempDir::new(&prefix)?;
    let repo = git::TempRepo::clone(upstream, &tempdir)?;

    loop {
        supervisor.request_next_reduction(me.clone());
        match incoming.recv().unwrap() {
            WorkerMessage::Shutdown => {
                logger.shutdown_worker(id);
                return Ok(());
            }
            WorkerMessage::NextReductionResponse(reduction) => {
                logger.start_judging_interesting(id);
                let maybe_interesting = reduction.into_interesting(&*predicate, &repo)?;
                if let Some(interesting) = maybe_interesting {
                    logger.judged_interesting(id, interesting.size());
                    supervisor.report_interesting(interesting);
                } else {
                    logger.judged_not_interesting(id);
                }
            }
        }
    }
}
