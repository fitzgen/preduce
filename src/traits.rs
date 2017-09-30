//! Interfaces and common behaviors.

use error;
use score;
use std::any::Any;
use std::borrow::Cow;
use std::fmt;
use std::path;
use test_case;

/// A reducer generates potentially-interesting reductions from a
/// known-interesting initial seed test case.
///
/// Reducers should produce potentially-interesting reductions that are smaller
/// in size than the given known-interesting seed.
///
/// Reducers should produce reductions that are, generally speaking, likely to
/// be found interesting.
///
/// Example reduction strategies that might be implemented as different
/// reducers:
///
/// * Removing individual lines from the seed
/// * Removing blocks indented more than N spaces/tabs
/// * Replacing C function definitions with forward declarations
/// * Replacing a Lisp S-Expression with `'()`
/// * Replacing a JavaScript expression with `null`
/// * Removing one outer layer of an HTML tree, eg reducing
///   `<b><u>hello</u></b>` to `<u>hello</u>`
/// * Etc...
///
/// This is analogous to a "pass" in C-Reduce.
///
/// ### `Box<Any + Send>` State
///
/// `preduce` tests many reductions for interesting-ness concurrently, even
/// reductions generated from the same reducer. Therefore, when a reduction is
/// found to be interesting, and we want to search go down that rabbit hole, we
/// need to know *which* rabbit hole to go down. This *which* is what the
/// `Box<Any + Send>` state represents. It can also be used to cache the results
/// of computations across multiple threads of search space traversal.
///
/// Normally, we would use an associated `Self::State` type instead of `Box<Any
/// + Send>`. However, the `Reducer` trait needs to be object safe, which
/// precludes associated types. `preduce` will only call a reducer with state
/// that it created, so it is always OK to downcast and `unwrap` these boxes.
///
/// If a reducer doesn't need this state, it can always use `Box<()>` as its
/// state.
///
/// If a reducer can't efficiently continue a traversal when we find that a
/// reduction was interesting and wish to pick up where we previously left off,
/// then it can opt to redirect calls to `next_state_on_interesting` to
/// `new_state`. This has the downside of iterating over the same first N
/// reductions that are likely still not interesting.
///
/// How might this state threading be used? Consider implementing a reducer that
/// removes a single line at a time from the seed test case, in order from top
/// to bottom. We could use `(usize, usize)` as our state, where the first
/// `usize` is an index of the next line to try removing, and the second `usize`
/// is the number of lines in the seed file.
///
/// * When creating the initial state, we will count the total number of lines
/// in the seed test case and initialize our index to 0. This is an example
/// where the state can serve as a cache: we won't ever have to count the number
/// of lines in the file again.
///
/// * Every time `next_state` is called, we advance the index. When the index is
/// greater than or equal to the number of lines in the seed test case, we've
/// exhausted all reductions. There are no more lines to try removing.
///
/// * When `next_state_on_interesting` is called, we know that the new seed test
/// case is the same as the old seed that our state was created with, sans the
/// line at `index - 1` in the old seed. Therefore, we can efficiently continue
/// reducing without re-counting the lines in the new seed. All we need to do is
/// decrement the total lines by one, and decrementing our index by one.
///
/// ### Example Reducer
///
/// Here is the single-line-removing reducer in Rust code:
///
/// ```
/// use preduce::error::Result;
/// use preduce::test_case;
/// use preduce::traits::Reducer;
/// use std::any::Any;
/// use std::borrow::Cow;
///
/// // A reducer that removes single lines from seed test cases.
/// #[derive(Debug)]
/// pub struct SingleLines;
///
/// impl SingleLines {
///     // A helper to downcast the `Box<Any + Send>` state into this reducer's
///     // "real" state type.
///     fn downcast(state: &Box<Any + Send>) -> (usize, usize) {
///         *state.downcast_ref::<(usize, usize)>().unwrap()
///     }
/// }
///
/// impl preduce::traits::Reducer for SingleLines {
///     fn name(&self) -> Cow<str> {
///         Cow::from("SingleLines")
///     }
///
///     fn clone_boxed(&self) -> Box<Reducer> {
///         Box::new(SingleLines)
///     }
///
///     // When constructing the initial state for a seed, we count the number
///     // of lines in the file and initialize our index at zero.
///     fn new_state(
///         &mut self,
///         seed: &test_case::Interesting
///     ) -> Result<Box<Any + Send>> {
/// #       let count_number_of_lines_in_file = |_| -> Result<_> {  unimplemented!() };
///         let num_lines = count_number_of_lines_in_file(seed)?;
///         Ok(Box::new((0, num_lines)))
///     }
///
///     fn clone_state(&self, state: &Box<Any + Send>) -> Box<Any + Send> {
///         Box::new(Self::downcast(state))
///     }
///
///     // Advancing the state is done by incrementing the index of the next
///     // line to try removing.
///     fn next_state(
///         &mut self,
///         seed: &test_case::Interesting,
///         prev_state: &Box<Any + Send>
///     ) -> Result<Option<Box<Any + Send>>> {
///         let (index, num_lines) = Self::downcast(prev_state);
///         Ok(Some(Box::new((index + 1, num_lines))))
///     }
///
///     // When we know that the previous state resulted in an interesting test
///     // case, we don't need to recount the number of lines in the file or start
///     // removing lines at index zero again. We know there is only one fewer line
///     // in the new seed, and that we already tried removing the `index - 1`
///     // lines from the old seed.
///     fn next_state_on_interesting(
///         &mut self,
///         _new_seed: &test_case::Interesting,
///         _old_seed: &test_case::Interesting,
///         prev_state: &Box<Any + Send>
///     ) -> Result<Option<Box<Any + Send>>> {
///         let (index, num_lines) = Self::downcast(prev_state);
///         Ok(Some(Box::new((index - 1, num_lines - 1))))
///     }
///
///     // Remove the line at the given state's current index from the seed
///     // file. If the index is greater than or equal to the number of lines in
///     // the seed, then we're out of lines to try removing.
///     fn reduce(
///         &mut self,
///         seed: &test_case::Interesting,
///         state: &Box<Any + Send>
///     ) -> Result<Option<test_case::PotentialReduction>> {
///         let (index, num_lines) = Self::downcast(state);
///
///         if index >= num_lines {
///             return Ok(None);
///         }
///
/// #       let remove_nth_line = |_, _| -> Result<_> { unimplemented!() };
///         let reduction = remove_nth_line(index, seed)?;
///         Ok(Some(reduction))
///     }
///
///     // If `preduce` would like to skip further ahead, we can do it more
///     // efficiently than calling `next_state` in a loop, so override the
///     // provided default implementation of `fast_forward_states`.
///     fn fast_forward_states(
///         &mut self,
///         _seed: &test_case::Interesting,
///         n: usize,
///         prev_state: &Box<Any + Send>
///     ) -> Result<Option<Box<Any + Send>>> {
///         let (index, num_lines) = Self::downcast(prev_state);
///         Ok(Some(Box::new((index + n, num_lines))))
///     }
/// }
/// ```
pub trait Reducer: fmt::Debug + Send {
    /// Get this reducer's unique name.
    fn name(&self) -> Cow<str>;

    /// Clone this reducer into a boxed trait object.
    fn clone_boxed(&self) -> Box<Reducer>
    where
        Self: 'static;

    /// Create the initial state for this known-interesting seed test case.
    fn new_state(&mut self, seed: &test_case::Interesting) -> error::Result<Box<Any + Send>>;

    /// Clone some state previously returned by this reducer.
    fn clone_state(&self, &Box<Any + Send>) -> Box<Any + Send>;

    /// Advance to the next state.
    ///
    /// We don't know if the reduction produced with the given `prev_state` was
    /// interesting or not. `preduce` tests many reductions in parallel, even
    /// reductions generated from the same reducer, so we might still be testing
    /// the reduction produced with the last state. Or, we might even have a
    /// heuristic that decided we should skip over the last state.
    ///
    /// For example, a reducer that is removing lines one at a time from a file
    /// would increment its current index state.
    ///
    /// If the reducer has exhausted all reductions, and there is no next state,
    /// then it should return `None`.
    fn next_state(
        &mut self,
        seed: &test_case::Interesting,
        prev_state: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>>;

    /// Advance to the next state, given that the previous state produced an
    /// known-interesting test case.
    ///
    /// The previous state was used to generate `new_seed` from `old_seed`,
    /// which has been found interesting. Given that information, advance the
    /// `prev_state` to some new state. If the reducer has exhausted all
    /// reductions, and there is no next state, then it should return `None`.
    ///
    /// For example, a reducer that is removing lines one at a time from a file
    /// would not need to increment its index state in
    /// `next_state_on_interesting`, because that line was removed from the test
    /// case, and the next line to test removing has shifted down to that index.
    fn next_state_on_interesting(
        &mut self,
        new_seed: &test_case::Interesting,
        old_seed: &test_case::Interesting,
        prev_state: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>>;

    /// Skip over the next `n` states, returning an eagerly advanced state.
    ///
    /// By default, this will call `next_state` in a loop `n` times. If a
    /// reducer can implement this more efficiently, , then it should override
    /// the default implementation. For example, a reducer removing lines one at
    /// a time from a file could simply add `n` to its current index.
    ///
    /// Alternatively, if skipping ahead doesn't even make sense for a reducer,
    /// for example it is doing some kind of binary search, then it can opt out
    /// of skipping by returning a clone of the `prev_state`.
    ///
    /// If the reducer will be exhausted in `n` steps, then `None` should be
    /// returned.
    fn fast_forward_states(
        &mut self,
        seed: &test_case::Interesting,
        n: usize,
        prev_state: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>> {
        let mut state = match self.next_state(seed, prev_state)? {
            None => return Ok(None),
            Some(state) => state,
        };

        for _ in 1..n {
            state = match self.next_state(seed, &state)? {
                None => return Ok(None),
                Some(state) => state,
            };
        }

        Ok(Some(state))
    }

    /// Generate a potentially-interesting reduction of the given
    /// known-interesting seed test case with the given state.
    ///
    /// If the reducer has exhausted all of its reductions, then it should
    /// return `None`.
    fn reduce(
        &mut self,
        seed: &test_case::Interesting,
        state: &Box<Any + Send>
    ) -> error::Result<Option<test_case::PotentialReduction>>;
}

/// Is a potential reduction interesting?
///
/// If a potential reduction is not interesting, then it will be abandoned,
/// along with further potential reductions of it.
///
/// If a potential reduction is interesting, then it is a candidate for the
/// current most-reduced test case.
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
