//! Interfaces and common behaviors.

use error;
use score;
use std::borrow::Cow;
use std::fmt;
use std::path;
use test_case;

/// A reducer generates potential reductions of an initial seed test case.
///
/// Example reduction strategies that might be implemented as different
/// reducers:
///
/// * Removing individual lines from the seed
/// * Removing blocks indented more than N spaces/tabs
/// * Replacing C function definitions with forward declarations
/// * Etc...
///
/// This is analogous to a "pass" in creduce.
pub trait Reducer: fmt::Debug + Send {
    /// Get this reducer's unique name.
    fn name(&self) -> Cow<str>;

    /// Configure this reducer to use generate potential reductions from the
    /// given seed test case.
    fn set_seed(&mut self, seed: test_case::Interesting);

    /// Generate the next potential reduction of the seeded test case at the
    /// given destination path.
    ///
    /// This method should return `Some(path_to_potential_reduction)` if it
    /// generated a potential reduction of the test case, or `None` if it has
    /// exhausted all of its potential reductions.
    fn next_potential_reduction(&mut self) -> error::Result<Option<test_case::PotentialReduction>>;

    /// Clone this `Reducer` as an owned trait object. The resulting reducer
    /// should be unseeded.
    fn clone_unseeded(&self) -> Box<Reducer>
    where
        Self: 'static;
}

/// Is a potential reduction interesting?
///
/// If a potential reduction is not interesting, then it will be abandoned,
/// along with further potential reductions of it.
///
/// If a potential reduction is interesting, then it is a candidate for the
/// current most-reduced test case, or a even a new further potential reduction
/// by merging it with the current most-reduced test case.
///
/// An is-interesting test should be deterministic and idempotent.
pub trait IsInteresting: Send {
    /// Return `true` if the reduced test case is interesting, `false`
    /// otherwise.
    fn is_interesting(&self, potential_reduction: &path::Path) -> error::Result<bool>;

    /// Clone this `IsInteresting` predicate as an owned trait object.
    fn clone(&self) -> Box<IsInteresting>
    where
        Self: 'static;
}

/// An oracle observes the results of interesting-ness judgements of reductions
/// and then predicts the interesting-ness of future reductions by scoring
/// them. The resulting scores are ultimately used by the supervisor actor to
/// prioritize and schedule work.
pub trait Oracle: Send {
    /// Tell the oracle that we found a new smallest interesting test case.
    fn observe_smallest_interesting(&mut self, interesting: &test_case::Interesting);

    /// Tell the oracle that we found a new interesting test case, but that it
    /// is not the smallest.
    fn observe_not_smallest_interesting(&mut self, interesting: &test_case::Interesting);

    /// Tell the oracle that we found the given reduction unininteresting.
    fn observe_not_interesting(&mut self, reduction: &test_case::PotentialReduction);

    /// Tell the oracle that the reducer with the given name has been exhausted.
    fn observe_exhausted(&mut self, reducer_name: &str);

    /// Ask the oracle's to score the given potential reduction, so we know how
    /// to prioritize testing it.
    fn predict(&mut self, reduction: &test_case::PotentialReduction) -> score::Score;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reducer_is_object_safe() {
        #[allow(dead_code)]
        fn take_reducer_by_trait_object(_: &Reducer) {}
    }

    #[test]
    fn is_interesting_is_object_safe() {
        #[allow(dead_code)]
        fn take_is_interesting_by_trait_object(_: &IsInteresting) {}
    }
}
