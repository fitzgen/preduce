//! The supervisor actor manages workers, and brokers their access to new
//! reductions.

use super::{Logger, Worker, WorkerId};
use super::super::Options;
use error;
use std::fs;
use std::io;
use std::path;
use std::sync::mpsc;
use std::thread;
use traits;

/// The messages that can be sent to the supervisor actor.
#[derive(Debug)]
enum SupervisorMessage {
    RequestNextReduction(Worker),
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
}

// Supervisor actor implementation.

fn run<I, R>(opts: Options<I, R>,
             me: Supervisor,
             receiver: mpsc::Receiver<SupervisorMessage>)
             -> error::Result<()>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    let logger = Logger::spawn(io::stdout());
    backup_test_case(&opts.test_case, &logger)?;
    let workers = spawn_workers(&opts, me, logger.clone());

    for msg in receiver {
        match msg {
            SupervisorMessage::RequestNextReduction(who) => {
                unimplemented!();
            }
        }
    }

    Ok(())
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

    fs::copy(test_case, backup_path).map_err(error::Error::TestCaseBackupFailure)?;

    Ok(())
}

fn spawn_workers<I, R>(opts: &Options<I, R>, me: Supervisor, logger: Logger) -> Vec<Worker>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    (0..opts.num_workers())
        .map(|i| Worker::spawn(WorkerId::new(i), me.clone(), logger.clone()))
        .collect()
}
