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
#![deny(unsafe_code)]

extern crate blake2;
extern crate ctrlc;
extern crate either;
extern crate fixedbitset;
extern crate generic_array;
extern crate histo;
extern crate is_executable;
extern crate lru_cache;
extern crate num_cpus;
extern crate preduce_ipc_types;
extern crate serde_json;
extern crate tempdir;
extern crate typenum;

mod actors;
pub mod error;
pub mod interesting;
pub mod oracle;
mod queue;
pub mod reducers;
pub mod score;
mod signposts;
pub mod test_case;
pub mod traits;

#[cfg(test)]
mod test_utils;

use std::mem;
use std::path;

/// A builder to configure a `preduce` run's options, and finally start the
/// candidate process.
///
/// ```
/// # fn _ignore() -> preduce::error::Result<()> {
/// let predicate = preduce::interesting::Script::new("is_interesting.sh")?;
/// let reducer = preduce::reducers::Script::new("generate_candidates.sh")?;
/// let test_case = "path/to/test-case";
///
/// // Construct the `Options` builder.
/// preduce::Options::new(predicate, vec![Box::new(reducer)], test_case)
///     // Then configure and tweak various options.
///     .workers(12)
///     // Finally, kick off the candidate process.
///     .run()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Options<I>
where
    I: traits::IsInteresting,
{
    test_case: path::PathBuf,
    is_interesting: I,
    reducers: Vec<Box<traits::Reducer>>,
    workers: usize,
    print_histograms: bool,
}

/// APIs for configuring options and spawning the candidate process.
impl<I> Options<I>
where
    I: 'static + traits::IsInteresting,
{
    /// Construct a new `Options` builder.
    ///
    /// You must provide the is-interesting predicate, the test case candidate
    /// generators, and the initial test case.
    ///
    /// ### Panics
    ///
    /// This function panics if the `reducers` vec is empty.
    ///
    /// ### Example
    ///
    /// ```
    /// # fn _ignore() -> preduce::error::Result<()> {
    /// let predicate = preduce::interesting::Script::new("is_interesting.sh")?;
    /// let reducer = preduce::reducers::Script::new("generate_candidates.sh")?;
    ///
    /// let opts = preduce::Options::new(predicate, vec![Box::new(reducer)], "path/to/test-case");
    /// # let _ = opts;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new<P>(
        is_interesting: I,
        reducers: Vec<Box<traits::Reducer>>,
        test_case: P,
    ) -> Options<I>
    where
        P: Into<path::PathBuf>,
    {
        assert!(!reducers.is_empty());
        Options {
            test_case: test_case.into(),
            is_interesting: is_interesting,
            reducers: reducers,
            workers: num_cpus::get(),
            print_histograms: false,
        }
    }

    /// Specify how many workers should be testing candidates of the initial
    /// test case for interesting-ness in parallel.
    ///
    /// ```
    /// # fn _ignore() -> preduce::error::Result<()> {
    /// let predicate = preduce::interesting::Script::new("is_interesting.sh")?;
    /// let reducer = preduce::reducers::Script::new("generate_candidates.sh")?;
    /// let test_case = "path/to/test-case";
    ///
    /// let opts = preduce::Options::new(predicate, vec![Box::new(reducer)], test_case)
    ///     // Only use 4 workers instead of the number-of-logical-CPUs
    ///     // default.
    ///     .workers(4);
    /// # let _ = opts;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ### Panics
    ///
    /// Panics if `num_workers` is zero.
    pub fn workers(mut self, num_workers: usize) -> Options<I> {
        assert!(num_workers != 0);
        self.workers = num_workers;
        self
    }

    /// Whether we should print various histograms when we finish the candidate
    /// process.
    ///
    /// This is generally only useful for people hacking on `preduce`, or those
    /// for who get tickled just by seeing a histogram in their terminal.
    pub fn print_histograms(mut self, should_print_histograms: bool) -> Options<I> {
        self.print_histograms = should_print_histograms;
        self
    }

    /// Finish configuration and run the test case candidate process to
    /// completion.
    ///
    /// ```
    /// # fn _ignore() -> preduce::error::Result<()> {
    /// let predicate = preduce::interesting::Script::new("is_interesting.sh")?;
    /// let reducer = preduce::reducers::Script::new("generate_candidates.sh")?;
    /// let test_case = "path/to/test-case";
    ///
    /// preduce::Options::new(predicate, vec![Box::new(reducer)], test_case).run()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn run(self) -> error::Result<()> {
        // We must be robust in the face of one of our reducer scripts dying
        // while we're still trying to communicate with it. We are set up to
        // handle `Err` results properly, but we can't tolerate receiving
        // SIGPIPE and getting killed because we attempted to write to the
        // defunct child process's stdin.
        let _ignore_sigpipe = sig::AutoIgnoreSigpipe::default();

        let (_, handle) = actors::Supervisor::spawn(self)?;
        handle.join()??;
        Ok(())
    }
}

/// APIs for accessing the `Options`' configured settings.
impl<I> Options<I>
where
    I: 'static + traits::IsInteresting,
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

    /// Get this `Options`' `Reducer`s.
    pub fn reducers(&self) -> &[Box<traits::Reducer>] {
        &self.reducers[..]
    }

    /// Take ownership of this `Options`' `Reducer`s. Panics if the reducers
    /// have already been taken.
    pub(crate) fn take_reducers(&mut self) -> Vec<Box<traits::Reducer>> {
        assert!(
            !self.reducers.is_empty(),
            "should not have already taken the reducers"
        );
        mem::replace(&mut self.reducers, vec![])
    }
}

#[cfg(unix)]
#[allow(unsafe_code)]
mod sig {
    extern crate libc;

    pub struct AutoIgnoreSigpipe {
        previous_handler: libc::sighandler_t,
    }

    impl Default for AutoIgnoreSigpipe {
        fn default() -> AutoIgnoreSigpipe {
            let previous_handler = unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };
            AutoIgnoreSigpipe { previous_handler }
        }
    }

    impl Drop for AutoIgnoreSigpipe {
        fn drop(&mut self) {
            unsafe {
                libc::signal(libc::SIGPIPE, self.previous_handler);
            }
        }
    }
}

#[cfg(not(unix))]
mod sig {
    #[derive(Default)]
    pub struct AutoIgnoreSigpipe;
}
