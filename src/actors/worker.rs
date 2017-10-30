//! The worker actor pulls potentially interesting test cases from the
//! supervisor and tests them for interestingness.

use super::{Logger, Supervisor};
use either::{Either, Left, Right};
use error;
use signposts;
use std::fmt;
use std::panic;
use std::sync::mpsc;
use std::thread;
use test_case;
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
    NextCandidate(test_case::Candidate),
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
    pub fn spawn(
        id: WorkerId,
        predicate: Box<traits::IsInteresting>,
        supervisor: Supervisor,
        logger: Logger,
    ) -> error::Result<Worker> {
        logger.spawning_worker(id);

        let (sender, receiver) = mpsc::channel();

        let me = Worker {
            id: id,
            sender: sender,
        };
        let me2 = me.clone();

        thread::Builder::new()
            .name(format!("preduce-worker-{}", id))
            .spawn(move || {
                WorkerActor::run(id, me2, predicate, receiver, supervisor, logger);
            })?;

        Ok(me)
    }

    /// Get the id of this worker.
    pub fn id(&self) -> WorkerId {
        self.id
    }

    // For communication with this worker from the supervisor, don't unwrap the
    // mpsc sends. Instead of panicking the supervisor, let the catch_unwind'ing
    // of the worker inform the supervisor of a worker's early, unexpected
    // demise.

    /// Tell this worker to shutdown.
    pub fn shutdown(self) {
        let _ = self.sender.send(WorkerMessage::Shutdown);
    }

    /// Send the worker the response to its request for another potential
    /// candidate.
    pub fn next_candidate(&self, candidate: test_case::Candidate) {
        let _ = self.sender.send(WorkerMessage::NextCandidate(candidate));
    }
}

// Worker actor implementation.

struct WorkerActor {
    id: WorkerId,
    me: Worker,
    predicate: Box<traits::IsInteresting>,
    incoming: mpsc::Receiver<WorkerMessage>,
    supervisor: Supervisor,
    logger: Logger,
}

impl fmt::Debug for WorkerActor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "WorkerActor")
    }
}

#[derive(Debug)]
struct Test {
    worker: WorkerActor,
    candidate: test_case::Candidate,
}

#[derive(Debug)]
struct Interesting {
    worker: WorkerActor,
    interesting: test_case::Interesting,
}

impl WorkerActor {
    fn run(
        id: WorkerId,
        me: Worker,
        predicate: Box<traits::IsInteresting>,
        incoming: mpsc::Receiver<WorkerMessage>,
        supervisor: Supervisor,
        logger: Logger,
    ) {
        match {
            let supervisor2 = supervisor.clone();
            let logger2 = logger.clone();
            panic::catch_unwind(panic::AssertUnwindSafe(move || {
                WorkerActor::try_run(id, me, predicate, incoming, supervisor2, logger2)
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

    fn try_run(
        id: WorkerId,
        me: Worker,
        predicate: Box<traits::IsInteresting>,
        incoming: mpsc::Receiver<WorkerMessage>,
        supervisor: Supervisor,
        logger: Logger,
    ) -> error::Result<()> {
        logger.spawned_worker(id);

        let worker = WorkerActor {
            id: id,
            me: me,
            predicate: predicate,
            incoming: incoming,
            supervisor: supervisor,
            logger: logger,
        };

        let mut test = match worker.get_next_candidate(None) {
            Some(test) => test,
            None => return Ok(()),
        };

        loop {
            match test.judge()? {
                Left(interesting) => {
                    // The test case was judged interesting -- tell the
                    // supervisor!
                    match interesting.report_to_supervisor() {
                        None => {
                            // That interesting test case became the new
                            // globally smallest interesting test case, but when
                            // we tried to reduce it further, we found it was
                            // already so minimal that the reducer couldn't
                            // produce any candidates. Time to
                            // shutdown.
                            return Ok(());
                        }
                        Some(new_test) => {
                            // That interesting test case became the new
                            // globally smallest interesting test case, and this
                            // is a new candidate based on it for us to test.
                            test = new_test;
                        }
                    }
                }
                Right((worker, not_interesting)) => {
                    // The test case was judged not interesting; grab a new
                    // candidate to test.
                    test = match worker.get_next_candidate(Some(not_interesting)) {
                        Some(test) => test,
                        None => return Ok(()),
                    };
                }
            }
        }
    }

    fn shutdown<T>(self) -> Option<T> {
        self.logger.shutdown_worker(self.id);
        None
    }

    fn get_next_candidate(
        self,
        not_interesting: Option<test_case::Candidate>,
    ) -> Option<Test> {
        let _signpost = signposts::WorkerGetNextCandidate::new();

        self.supervisor
            .request_next_candidate(self.me.clone(), not_interesting);
        match self.incoming.recv().unwrap() {
            WorkerMessage::Shutdown => self.shutdown(),
            WorkerMessage::NextCandidate(candidate) => Some(Test {
                worker: self,
                candidate: candidate,
            }),
        }
    }
}

impl Test {
    fn judge(
        self,
    ) -> error::Result<Either<Interesting, (WorkerActor, test_case::Candidate)>> {
        let _signpost = signposts::WorkerJudgeInteresting::new();

        self.worker
            .logger
            .start_judging_interesting(self.worker.id, self.candidate.clone());
        match self.candidate.into_interesting(&self.worker.predicate)? {
            Left(interesting) => {
                self.worker
                    .logger
                    .judged_interesting(self.worker.id, interesting.clone());
                Ok(Left(Interesting {
                    worker: self.worker,
                    interesting: interesting,
                }))
            }
            Right(not_interesting) => {
                self.worker
                    .logger
                    .judged_not_interesting(self.worker.id, not_interesting.clone());
                Ok(Right((self.worker, not_interesting)))
            }
        }
    }
}

impl Interesting {
    fn report_to_supervisor(self) -> Option<Test> {
        let _signpost = signposts::WorkerReportInteresting::new();

        self.worker
            .supervisor
            .report_interesting(self.worker.me.clone(), self.interesting.clone());

        match self.worker.incoming.recv().unwrap() {
            WorkerMessage::Shutdown => self.worker.shutdown(),
            WorkerMessage::NextCandidate(candidate) => Some(Test {
                worker: self.worker,
                candidate: candidate,
            }),
        }
    }
}
