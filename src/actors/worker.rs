//! The worker actor pulls potentially interesting test cases from the
//! supervisor and tests them for interestingness.

use super::{Logger, Supervisor};
use std::fmt;
use std::sync::mpsc;
use std::thread;

/// An identifier for a worker actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

/// A client handle to a worker actor.
#[derive(Clone, Debug)]
pub struct Worker {
    sender: mpsc::Sender<WorkerMessage>,
}

/// Worker client API.
impl Worker {
    /// Spawn a new worker actor.
    pub fn spawn(id: WorkerId, supervisor: Supervisor, logger: Logger) -> Self {
        logger.spawning_worker(id);

        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || run(id, receiver, supervisor, logger));

        Worker { sender: sender }
    }
}

// Worker actor implementation.

fn run(id: WorkerId,
       incoming: mpsc::Receiver<WorkerMessage>,
       supervisor: Supervisor,
       logger: Logger) {
    logger.spawned_worker(id);

    for msg in incoming {}
}
