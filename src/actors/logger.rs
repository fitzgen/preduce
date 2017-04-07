//! TODO FITZGEN

use super::WorkerId;
use std::fmt;
use std::io::Write;
use std::path;
use std::sync::mpsc;
use std::thread;

/// TODO FITZGEN
#[derive(Debug)]
pub enum LoggerMessage {
    SpawningWorker(WorkerId),
    SpawnedWorker(WorkerId),
    BackingUpTestCase(String, String),
}

impl fmt::Display for LoggerMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            LoggerMessage::SpawningWorker(id) => write!(f, "Spawning worker {}", id),
            LoggerMessage::SpawnedWorker(id) => write!(f, "Spawned worker {}", id),
            LoggerMessage::BackingUpTestCase(ref from, ref to) => {
                write!(f, "Backing up initial test case {} to {}", from, to)
            }
        }
    }
}

/// TODO FITZGEN
#[derive(Clone, Debug)]
pub struct Logger {
    sender: mpsc::Sender<LoggerMessage>,
}

/// Logger client implementation.
impl Logger {
    /// Spawn a `Logger` actor, writing logs to the given `Write`able.
    pub fn spawn<W>(to: W) -> Logger
        where W: 'static + Send + Write
    {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || Logger::run(to, receiver));
        Logger { sender: sender }
    }

    /// Log the start of spawning a worker.
    pub fn spawning_worker(&self, id: WorkerId) {
        self.sender.send(LoggerMessage::SpawningWorker(id)).unwrap();
    }

    /// Log the end of spawning a worker.
    pub fn spawned_worker(&self, id: WorkerId) {
        self.sender.send(LoggerMessage::SpawnedWorker(id)).unwrap();
    }

    /// Log that we are backing up the initial test case.
    pub fn backing_up_test_case<P, Q>(&self, from: P, to: Q)
        where P: AsRef<path::Path>,
              Q: AsRef<path::Path>
    {
        let from = from.as_ref().display().to_string();
        let to = to.as_ref().display().to_string();
        self.sender.send(LoggerMessage::BackingUpTestCase(from, to)).unwrap();
    }
}

/// Logger actor implementation.
impl Logger {
    fn run<W>(mut to: W, incoming: mpsc::Receiver<LoggerMessage>)
        where W: Write
    {
        for log_msg in incoming {
            writeln!(&mut to, "{}", log_msg).expect("Should write to log file");
        }
    }
}
