//! Interfaces and common behaviors.

use error;
use std::path;

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
pub trait Reducer {
    /// Configure this reducer to use generate potential reductions from the
    /// given seed test case.
    fn set_seed(&mut self, seed: &path::Path);

    /// Configure this reducer to use generate potential reductions into the
    /// given out directory.
    fn set_out_dir(&mut self, out_dir: &path::Path);

    /// Generate the next potential reduction of the seeded test case at the
    /// given destination path.
    ///
    /// This method should return `Some(path_to_potential_reduction)` if it
    /// generated a potential reduction of the test case, or `None` if it has
    /// exhausted all of its potential reductions.
    fn next_potential_reduction(&mut self) -> error::Result<Option<path::PathBuf>>;
}

/// Is a potential reduction interesting?
///
/// If a potential reduction is not interesting, then it will be abandoned,
/// along with further potential reductions of it.
///
/// If a potential reduction is interesting, then it is a candidate for the
/// current most-reduced test case, or a even a new further potential reduction
/// by merging it with the current most-reduced test case.
pub trait IsInteresting {
    /// Return `true` if the reduced test case is interesting, `false`
    /// otherwise.
    fn is_interesting(&self, potential_reduction: &path::Path) -> error::Result<bool>;
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

