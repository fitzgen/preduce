//! The sigint actor listens for SIGINT and notifies the supervisor to
//! gracefully shut down upon its receipt.

use super::{Logger, Supervisor};
use ctrlc;
use error;
use std::sync::{Once, ONCE_INIT};
use std::sync::atomic::{AtomicBool, Ordering, ATOMIC_BOOL_INIT};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// The different kinds of log messages that can be sent to the logger actor.
#[derive(Debug)]
pub enum SigintMessage {
    Shutdown,
}

/// A client to the sigint actor.
#[derive(Clone, Debug)]
pub struct Sigint {
    sender: mpsc::Sender<SigintMessage>,
}

/// Sigint client implementation.
impl Sigint {
    /// Spawn a `Sigint` actor, writing logs to the given `Write`able.
    pub fn spawn(
        supervisor: Supervisor,
        logger: Logger,
    ) -> error::Result<(Sigint, thread::JoinHandle<()>)> {
        let (sender, receiver) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("preduce-sigint".into())
            .spawn(move || Sigint::run(receiver, supervisor, logger))?;
        Ok((Sigint { sender: sender }, handle))
    }

    /// Tell the SIGINT actor to shutdown.
    pub fn shutdown(&self) {
        let _ = self.sender.send(SigintMessage::Shutdown);
    }
}

/// Sigint actor implementation.
impl Sigint {
    fn run(incoming: mpsc::Receiver<SigintMessage>, supervisor: Supervisor, logger: Logger) {
        static GOT_SIGINT: AtomicBool = ATOMIC_BOOL_INIT;
        static SET_SIGINT_HANDLER: Once = ONCE_INIT;

        SET_SIGINT_HANDLER.call_once(|| {
            // Just ignore any potential error setting the handler. It just
            // means that if we do get a SIGINT, then we'll be shutdown
            // un-gracefully at that time.
            let _ = ctrlc::set_handler(move || {
                GOT_SIGINT.store(true, Ordering::SeqCst);
            });
        });

        loop {
            thread::sleep(Duration::from_millis(50));

            match incoming.try_recv() {
                Ok(SigintMessage::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => return,
                Err(mpsc::TryRecvError::Empty) => if GOT_SIGINT.swap(false, Ordering::SeqCst) {
                    logger.got_sigint();
                    supervisor.got_sigint();
                },
            }
        }
    }
}
