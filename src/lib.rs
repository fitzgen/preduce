//! A generic and parallel test case reducer.
//!
//! For information on using the `preduce` binary, run
//!
//! ```commands
//! $ preduce --help
//! ```
//!
//! For programmatic usage of `preduce` as a library, see the `preduce::Options`
//! entry point to `preduce`'s public API.

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

extern crate git2;
extern crate tempdir;
extern crate num_cpus;

mod actors;
pub mod error;
mod git;
pub mod interesting;
pub mod reducers;
pub mod test_case;
pub mod traits;

#[cfg(test)]
mod test_utils;

use std::path;

/// A builder to configure a `preduce` run's options, and finally start the
/// reduction process.
///
/// ```
/// # fn _ignore() -> preduce::error::Result<()> {
/// let predicate = preduce::interesting::Script::new("is_interesting.sh");
/// let reducer = preduce::reducers::Script::new("generate_reductions.sh");
/// let test_case = "path/to/test-case";
///
/// // Construct the `Options` builder.
/// preduce::Options::new(predicate, reducer, test_case)
///     // Then configure and tweak various options.
///     .workers(12)
///     // Finally, kick off the reduction process.
///     .run()?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct Options<I, R>
    where I: traits::IsInteresting,
          R: traits::Reducer
{
    test_case: path::PathBuf,
    is_interesting: I,
    reducer: R,
    workers: usize,
}

/// APIs for configuring options and spawning the reduction process.
impl<I, R> Options<I, R>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    /// Construct a new `Options` builder.
    ///
    /// You must provide the is-interesting predicate, the test case
    /// reduction generator, and the initial test case.
    ///
    /// ```
    /// let predicate = preduce::interesting::Script::new("is_interesting.sh");
    /// let reducer = preduce::reducers::Script::new("generate_reductions.sh");
    ///
    /// let opts = preduce::Options::new(predicate, reducer, "path/to/test-case");
    /// # let _ = opts;
    /// ```
    pub fn new<P>(is_interesting: I, reducer: R, test_case: P) -> Options<I, reducers::Fuse<R>>
        where P: Into<path::PathBuf>
    {
        Options {
            test_case: test_case.into(),
            is_interesting: is_interesting,
            reducer: reducers::Fuse::new(reducer),
            workers: num_cpus::get(),
        }
    }

    /// Specify how many workers should be testing reductions of the initial
    /// test case for interesting-ness in parallel.
    ///
    /// ```
    /// let predicate = preduce::interesting::Script::new("is_interesting.sh");
    /// let reducer = preduce::reducers::Script::new("generate_reductions.sh");
    /// let test_case = "path/to/test-case";
    ///
    /// let opts = preduce::Options::new(predicate, reducer, test_case)
    ///     // Only use 4 workers instead of the number-of-logical-CPUs
    ///     // default.
    ///     .workers(4);
    /// # let _ = opts;
    /// ```
    ///
    /// ### Panics
    ///
    /// Panics if `num_workers` is zero.
    pub fn workers(mut self, num_workers: usize) -> Options<I, R> {
        assert!(num_workers != 0);
        self.workers = num_workers;
        self
    }

    /// Finish configuration and run the test case reduction process to
    /// completion.
    ///
    /// ```
    /// # fn _ignore() -> preduce::error::Result<()> {
    /// let predicate = preduce::interesting::Script::new("is_interesting.sh");
    /// let reducer = preduce::reducers::Script::new("generate_reductions.sh");
    /// let test_case = "path/to/test-case";
    ///
    /// preduce::Options::new(predicate, reducer, test_case).run()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn run(self) -> error::Result<()> {
        let (_, handle) = actors::Supervisor::spawn(self);
        handle.join()??;
        Ok(())
    }
}

/// APIs for accessing the `Options`' configured settings.
impl<I, R> Options<I, R>
    where I: 'static + traits::IsInteresting,
          R: 'static + traits::Reducer
{
    /// Get the number of workers this `Options` is configured to use.
    pub fn num_workers(&self) -> usize {
        assert!(self.workers > 0);
        self.workers
    }

    /// Get this `Options`' `IsInteresting` predicate.
    pub fn predicate(&self) -> &I {
        &self.is_interesting
    }

    /// Get this `Options`' `Reducer`.
    pub fn reducer(&mut self) -> &mut R {
        &mut self.reducer
    }
}
