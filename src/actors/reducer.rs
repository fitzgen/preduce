//! A reducer actor wraps a `preduce::traits::Reducer` trait object, allowing us
//! to generate potential reductions from multiple reducers in parallel, and
//! pipelined with each worker that is testing interestingness.

use super::{Logger, Supervisor};
use error;
use signposts;
use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::panic;
use std::sync::mpsc;
use std::thread;
use test_case;
use traits;

/// An identifier for a request to an actor.
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
    RequestNextReduction(Option<test_case::Interesting>),
    NotInteresting(test_case::PotentialReduction),
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

    /// Tell the reducer that this test case was not interesting, and therefore
    /// it can forget about the state that generated it.
    pub fn not_interesting(&self, test: test_case::PotentialReduction) {
        let _ = self.sender.send(ReducerMessage::NotInteresting(test));
    }

    /// Send the reducer the response to its request for another potential
    /// reduction.
    pub fn request_next_reduction(
        &self,
        interesting: Option<test_case::Interesting>
    ) {
        let _ = self.sender.send(ReducerMessage::RequestNextReduction(interesting));
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
            Ok(Err(e)) => {
                supervisor2.reducer_errored(id, e);
            }
            Ok(Ok(())) => {}
        }
    }

    fn run_loop(mut self) -> error::Result<()> {
        self.logger.spawned_reducer(self.me.id);

        let mut current_seed = None;
        let mut current_state = None;

        // A map from a reduction we generated that is actively being tested, to
        // the seed it was generated from and the state used to generate
        // it. Note that the seed it was generated from is not necessarily the
        // current seed.
        let mut active_states: HashMap<
            test_case::PotentialReduction,
            (test_case::Interesting, Box<Any + Send>)
        > = Default::default();

        for msg in &self.incoming {
            match msg {
                ReducerMessage::Shutdown => {
                    self.logger.shutdown_reducer(self.me.id);
                    return Ok(());
                }
                ReducerMessage::SetNewSeed(new_seed) => {
                    current_state = Some(self.reducer.new_state(&new_seed)?);
                    current_seed = Some(new_seed);
                }
                ReducerMessage::NotInteresting(reduction) => {
                    active_states.remove(&reduction)
                        .expect("Reducer actors should only be informed of their own \
                                 reductions' interesting-ness");
                }
                ReducerMessage::RequestNextReduction(interesting) => {
                    let _signpost = signposts::ReducerNextReduction::new();

                    self.logger.start_generating_next_reduction(self.me.id);

                    let seed = current_seed.clone()
                        .expect("must not RequestNextReduction before SetNewSeed");

                    let state = match current_state.take() {
                        Some(s) => s,
                        None => {
                            self.exhuasted(seed.clone());
                            continue;
                        }
                    };

                    let next_state = match interesting {
                        None => self.reducer.next_state(&seed, &state)?,
                        Some(new_seed) => {
                            let current_state = {
                                let old_potential_reduction = new_seed.as_potential_reduction()
                                    .expect("RequestNextReduction's interesting test case must \
                                             hail from a potential reduction");
                                let (old_seed, old_state) = active_states.remove(old_potential_reduction)
                                    .expect("RequestNextReduction with an unknown RequestId");
                                self.reducer.next_state_on_interesting(
                                    &new_seed,
                                    &old_seed,
                                    &old_state
                                )?
                            };
                            current_seed = Some(new_seed);
                            current_state
                        }
                    };

                    let state = match next_state {
                        Some(s) => s,
                        None => {
                            current_state = None;
                            self.exhuasted(seed.clone());
                            continue;
                        }
                    };

                    let result = self.reducer.reduce(&seed, &state);
                    match result {
                        Err(e) => {
                            // Log the error and tell the supervisor we are out
                            // of reductions until the next seed test case.
                            current_state = None;
                            self.logger.reducer_errored(self.me.id, e);
                            self.supervisor
                                .no_more_reductions(self.me.clone(), seed.clone());
                        }
                        Ok(None) => {
                            current_state = None;
                            self.exhuasted(seed.clone());
                        }
                        Ok(Some(reduction)) => {
                            let cloned_state = self.reducer.clone_state(&state);
                            active_states.insert(reduction.clone(), (seed.clone(), cloned_state));
                            current_state = Some(state);

                            self.logger
                                .finish_generating_next_reduction(self.me.id, reduction.clone());
                            self.supervisor
                                .reply_next_reduction(self.me.clone(), reduction);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn exhuasted(&self, seed: test_case::Interesting) {
        self.logger.no_more_reductions(self.me.id);
        self.supervisor.no_more_reductions(self.me.clone(), seed);
    }
}
