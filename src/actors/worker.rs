//! TODO FITZGEN

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

/// TODO FITZGEN
#[derive(Debug)]
pub enum WorkerMessage {
}

/// TODO FITZGEN
pub struct Worker {
    sender: mpsc::Sender<WorkerMessage>,
}

/// Worker client API.
impl Worker {
    /// TODO FITZGEN
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
