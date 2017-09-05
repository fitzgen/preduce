//! A reducer actor wraps a `preduce::traits::Reducer` trait object, allowing us
//! to generate potential reductions from multiple reducers in parallel, and
//! pipelined with each worker that is testing interestingness.

use super::{Logger, Supervisor};
use error;
use signposts;
use std::fmt;
use std::panic;
use std::sync::mpsc;
use std::thread;
use test_case;
use traits;

/// An identifier for a reducer actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReducerId(usize);

impl ReducerId {
    pub fn new(id: usize) -> ReducerId {
        ReducerId(id)
    }
}

impl fmt::Display for ReducerId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Messages that can be sent to reducer actors.
#[derive(Debug)]
enum ReducerMessage {
    Shutdown,
    RequestNextReduction,
    SetNewSeed(test_case::Interesting),
}

/// A client handle to a reducer actor.
#[derive(Clone, Debug)]
pub struct Reducer {
    id: ReducerId,
    sender: mpsc::Sender<ReducerMessage>,
}

/// Reducer client API.
impl Reducer {
    /// Spawn a new reducer actor.
    pub fn spawn(
        id: ReducerId,
        reducer: Box<traits::Reducer>,
        supervisor: Supervisor,
        logger: Logger,
    ) -> error::Result<Reducer> {
        logger.spawning_reducer(id);

        let (sender, receiver) = mpsc::channel();

        let me = Reducer { id, sender: sender };
        let me2 = me.clone();

        thread::Builder::new()
            .name(format!("preduce-reducer-{:?}", reducer))
            .spawn(move || {
                ReducerActor::run(id, me2, reducer, receiver, supervisor, logger);
            })?;

        Ok(me)
    }

    /// Get this reducer's ID.
    pub fn id(&self) -> ReducerId {
        self.id
    }

    // For communication with this reducer from the supervisor, don't unwrap the
    // mpsc sends. Instead of panicking the supervisor, let the catch_unwind'ing
    // of the reducer inform the supervisor of a reducer's early, unexpected
    // demise.

    /// Tell this reducer to shutdown.
    pub fn shutdown(self) {
        let _ = self.sender.send(ReducerMessage::Shutdown);
    }

    /// Send the reducer the response to its request for another potential
    /// reduction.
    pub fn request_next_reduction(&self) {
        let _ = self.sender.send(ReducerMessage::RequestNextReduction);
    }

    /// Reseed this reducer actor with the given test case.
    pub fn set_new_seed(&self, new_seed: test_case::Interesting) {
        let _ = self.sender.send(ReducerMessage::SetNewSeed(new_seed));
    }
}

// Reducer actor implementation.

struct ReducerActor {
    me: Reducer,
    reducer: Box<traits::Reducer>,
    incoming: mpsc::Receiver<ReducerMessage>,
    supervisor: Supervisor,
    logger: Logger,
}

impl fmt::Debug for ReducerActor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ReducerActor")
    }
}

impl ReducerActor {
    fn run(
        id: ReducerId,
        me: Reducer,
        reducer: Box<traits::Reducer>,
        incoming: mpsc::Receiver<ReducerMessage>,
        supervisor: Supervisor,
        logger: Logger,
    ) {
        let supervisor2 = supervisor.clone();
        match {
            let actor = ReducerActor {
                me,
                reducer,
                incoming,
                supervisor,
                logger,
            };
            panic::catch_unwind(panic::AssertUnwindSafe(move || actor.run_loop()))
        } {
            Err(p) => {
                supervisor2.reducer_panicked(id, p);
            }
            Ok(()) => {}
        }
    }

    fn run_loop(mut self) {
        self.logger.spawned_reducer(self.me.id);

        let mut seed = None;

        for msg in self.incoming {
            match msg {
                ReducerMessage::Shutdown => {
                    self.logger.shutdown_reducer(self.me.id);
                    return;
                }
                ReducerMessage::SetNewSeed(new_seed) => {
                    seed = Some(new_seed.clone());
                    self.reducer.set_seed(new_seed);
                }
                ReducerMessage::RequestNextReduction => {
                    let _signpost = signposts::ReducerNextReduction::new();

                    self.logger.start_generating_next_reduction(self.me.id);
                    match self.reducer.next_potential_reduction() {
                        Err(e) => {
                            // Log the error and tell the supervisor we are out
                            // of reductions until the next seed test case.
                            self.logger.reducer_errored(self.me.id, e);
                            self.supervisor
                                .no_more_reductions(self.me.clone(), seed.clone().unwrap());
                        }
                        Ok(None) => {
                            self.logger.no_more_reductions(self.me.id);
                            self.supervisor
                                .no_more_reductions(self.me.clone(), seed.clone().unwrap());
                        }
                        Ok(Some(reduction)) => {
                            self.logger
                                .finish_generating_next_reduction(self.me.id, reduction.clone());
                            self.supervisor
                                .reply_next_reduction(self.me.clone(), reduction);
                        }
                    }
                }
            }
        }
    }
}
