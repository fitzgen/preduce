//! Concrete implementations of `preduce::traits::Reducer`.

extern crate rand;

use error;
use is_executable::IsExecutable;
use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::path;
use std::process;
use std::sync::Arc;
use tempdir;
use test_case::{self, TestCaseMethods};
use traits::Reducer;

impl Reducer for Box<Reducer> {
    fn name(&self) -> Cow<str> {
        (**self).name()
    }

    fn clone_boxed(&self) -> Box<Reducer>
        where
        Self: 'static
    {
        (**self).clone_boxed()
    }

    fn new_state(&mut self, seed: &test_case::Interesting) -> error::Result<Box<Any + Send>> {
        (**self).new_state(seed)
    }

    fn clone_state(&self, state: &Box<Any + Send>) -> Box<Any + Send> {
        (**self).clone_state(state)
    }

    fn next_state(
        &mut self,
        seed: &test_case::Interesting,
        prev_state: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>> {
        (**self).next_state(seed, prev_state)
    }

    fn next_state_on_interesting(
        &mut self,
        new_seed: &test_case::Interesting,
        old_seed: &test_case::Interesting,
        prev_state: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>> {
        (**self).next_state_on_interesting(new_seed, old_seed, prev_state)
    }

    fn fast_forward_states(
        &mut self,
        seed: &test_case::Interesting,
        n: usize,
        prev_state: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>> {
        (**self).fast_forward_states(seed, n, prev_state)
    }

    fn reduce(
        &mut self,
        seed: &test_case::Interesting,
        state: &Box<Any + Send>,
    ) -> error::Result<Option<test_case::PotentialReduction>> {
        (**self).reduce(seed, state)
    }

}

/// A test case reducer that is implemented as an external script.
///
/// ### IPC Protocol
///
/// The seed test case is given as the first and only argument to the script.
///
/// When `preduce` would like the next potential reduction of the seed test case
/// to be generated, it will write a file path followed by a '\n' byte to
/// `stdin`. Upon reading this path and newline, the script must generate the
/// next reduction at that path. Once generation of the reduction is complete,
/// the script must write a '\n' byte to `stdout`. Alternatively, if the
/// subprocess has exhausted all of its potential reductions, then it may simply
/// exit without printing anything.
///
/// All file paths are encoded in UTF-8.
///
/// If `preduce` does not need any more reductions from the reducer script, it
/// will write a '\n' byte to `stdin` with no preceeding path. Upon receipt, the
/// reducer script should exit cleanly, cleaning up after itself as needed.
///
/// ### Example Reducer Script
///
/// This example reducer script tries removing prefixes of the seed test case:
///
/// ```bash
/// #!/usr/bin/env bash
///
/// # The initial seed test case is the first and only argument.
/// seed="$1"
///
/// # Count how many lines are in the test case.
/// n=$(wc -l "$seed" | cut -d ' ' -f 1)
///
/// # Generate a potential reduction of the seed's last line, then its last 2
/// # lines, then its last 3 lines, etc...
///
/// for (( i=1 ; i < n; i++ )); do
///     # Read the file path and '\n' from stdin.
///     read -r reduction_path
///
///     # Check to see if `preduce` is telling us to shut down.
///     if [[ "$reduction_path" == "" ]]; then
///         exit
///     fi
///
///     # Generate the potential reduction in a new file.
///     tail -n "$i" "$seed" > "$reduction_path"
///
///     # Tell `preduce` that we are done generating the potential reduction.
///     echo
/// done
/// ```
///
/// ### Example Rust Usage
///
/// ```
/// extern crate preduce;
/// use preduce::traits::Reducer;
///
/// # fn main() { fn _foo() -> preduce::error::Result<()> {
/// // Create a `Reducer` that is implemented by a child process running an
/// // external script.
/// let mut script = preduce::reducers::Script::new("path/to/reducer/script")?;
///
/// # let some_seed_test_case = || unimplemented!();
/// // Get some known-interesting seed test case to create reductions from.
/// let seed = some_seed_test_case();
///
/// // Get the initial state for the given seed.
/// let mut state = script.new_state(&seed)?;
///
/// while let Some(reduction) = script.reduce(&seed, &state)? {
///     println!("Here is a potential reduction: {:?}", reduction);
///
///     // Advance to the next state. Alternatively, if this reduction was
///     // interesting, use `next_state_on_interesting`.
///     state = match script.next_state(&seed, &state)? {
///         None => break,
///         Some(s) => s,
///     };
/// }
/// # Ok(()) } }
/// ```
#[derive(Debug)]
pub struct Script {
    program: path::PathBuf,
    out_dir: Option<Arc<tempdir::TempDir>>,
    counter: usize,
    seed: Option<test_case::Interesting>,
    child: Option<process::Child>,
    child_stdout: Option<process::ChildStdout>,
    strict: bool,
}

#[cfg(debug)]
fn slurp<P: AsRef<path::Path>>(p: P) -> error::Result<Vec<u8>> {
    let mut contents = Vec::new();
    let mut file = fs::File::open(p)?;
    file.read_to_end(&mut contents)?;
    contents
}

#[cfg(not(debug))]
#[inline(always)]
fn slurp<P: AsRef<path::Path>>(_p: P) -> error::Result<()> {
    Ok(())
}

impl Script {
    /// Construct a reducer script with the given `program`.
    pub fn new<S>(program: S) -> error::Result<Script>
    where
        S: AsRef<path::Path>,
    {
        if !program.as_ref().is_file() {
            return Err(error::Error::DoesNotExist(program.as_ref().into()));
        }

        if !program.as_ref().is_executable() {
            return Err(error::Error::IsNotExecutable(program.as_ref().into()));
        }

        let program = program.as_ref().canonicalize()?;

        Ok(Script {
            program: program,
            out_dir: None,
            counter: 0,
            seed: None,
            child: None,
            child_stdout: None,
            strict: false,
        })
    }

    /// Enable or disable extra strict checks on the reducer script.
    ///
    /// For example, enforce that generated reductions are smaller than the
    /// seed.
    pub fn set_strict(&mut self, be_strict: bool) {
        self.strict = be_strict;
    }

    fn set_seed(&mut self, seed: test_case::Interesting) {
        self.seed = Some(seed);

        // If we have an extant child process, shut it down now. We'll start a new
        // child process with the new seed the next time
        // `next_potential_reduction` is invoked.
        self.shutdown_child();
    }

    fn spawn_child(&mut self) -> error::Result<()> {
        assert!(self.seed.is_some());
        assert!(self.out_dir.is_none());
        assert!(self.child.is_none());
        assert!(self.child_stdout.is_none());

        self.out_dir = Some(Arc::new(tempdir::TempDir::new("preduce-reducer-script")?));

        let mut cmd = process::Command::new(&self.program);
        cmd.current_dir(self.out_dir.as_ref().unwrap().path())
            .arg(self.seed.as_ref().unwrap().path())
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        self.child_stdout = Some(stdout);
        self.child = Some(child);

        Ok(())
    }

    /// Attempt to nicely tell the child to stop by sending it an empty line to
    /// use as the next "seed", whereupon it should exit cleanly, thus cleaning
    /// up any resources it was using (e.g. temporary files).
    fn shutdown_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            if {
                write!(child.stdin.as_mut().unwrap(), "\n")
                    .and_then(|_| child.wait())
                    .is_err()
            } {
                self.kill_child();
            }
            self.child_stdout = None;
            self.out_dir = None;
        }
    }

    fn kill_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child_stdout = None;
        self.out_dir = None;
    }

    fn next_temp_file(&mut self) -> error::Result<test_case::TempFile> {
        let mut file_name = String::from("reduction");
        file_name.push_str(&self.counter.to_string());
        self.counter += 1;

        let mut file_path = path::PathBuf::from(file_name);
        if let Some(ext) = self.seed.as_ref().unwrap().path().extension() {
            file_path.set_extension(ext);
        }

        test_case::TempFile::new(self.out_dir.as_ref().unwrap().clone(), file_path)
    }

    fn next_potential_reduction_impl(
        &mut self,
    ) -> error::Result<Option<test_case::PotentialReduction>> {
        assert!(self.out_dir.is_some() && self.child.is_some() && self.child_stdout.is_some());

        let before_seed_contents = slurp(self.seed.as_ref().unwrap().path())?;

        let temp_file = self.next_temp_file().or_else(|e| {
            self.kill_child();
            Err(e)
        })?;

        // Write the desired path of the next reduction to the child's stdin. If
        // this fails, then the child already exited, presumably because it
        // determined it could not generate any reductions from the test file.
        if {
            let child = self.child.as_mut().unwrap();
            let child_stdin = child.stdin.as_mut().unwrap();
            write!(child_stdin, "{}\n", temp_file.path().display()).is_err()
        } {
            self.kill_child();
            return Ok(None);
        }

        // Read the newline response from the child's stdout, indicating that
        // the child has finished generating the reduction.
        let mut newline = [0];
        if {
            let child_stdout = self.child_stdout.as_mut().unwrap();
            child_stdout.read_exact(&mut newline).is_err()
        } {
            self.kill_child();
            return Ok(None);
        }

        if newline[0] != b'\n' {
            self.kill_child();
            let details = format!(
                "'{}' is not conforming to the reducer script protocol: \
                 expected a newline response",
                self.program.to_string_lossy()
            );
            return Err(error::Error::MisbehavingReducerScript(details));
        }

        if !temp_file.path().is_file() {
            self.kill_child();
            let details = format!(
                "'{}' did not generate a test case file at {}",
                self.program.to_string_lossy(),
                temp_file.path().display()
            );
            return Err(error::Error::MisbehavingReducerScript(details));
        }

        let after_seed_contents = slurp(self.seed.as_ref().unwrap().path())?;
        if before_seed_contents != after_seed_contents {
            let details = format!(
                "seed file was modified while '{}' generated its next reduction",
                self.program.to_string_lossy()
            );
            return Err(error::Error::MisbehavingReducerScript(details));
        }

        let reduction = test_case::PotentialReduction::new(
            self.seed.clone().unwrap(),
            self.program.to_string_lossy(),
            temp_file,
        )?;

        if self.strict {
            let seed_size = self.seed.as_ref().unwrap().size();
            if reduction.size() >= seed_size {
                self.kill_child();
                let details = format!(
                    "'{}' is generating reductions that are greater than or equal the seed's size: \
                     {} >= {}",
                    self.program.to_string_lossy(),
                    reduction.size(),
                    seed_size
                );
                return Err(error::Error::MisbehavingReducerScript(details));
            }
        }

        Ok(Some(reduction))
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        self.kill_child();
    }
}

impl Reducer for Script {
    fn name(&self) -> Cow<str> {
        self.program.to_string_lossy()
    }

    fn clone_boxed(&self) -> Box<Reducer>
        where
        Self: 'static,
    {
        Box::new(Script {
            program: self.program.clone(),
            out_dir: None,
            counter: 0,
            seed: None,
            child: None,
            child_stdout: None,
            strict: self.strict,
        })
    }

    fn new_state(&mut self, seed: &test_case::Interesting) -> error::Result<Box<Any + Send>> {
        self.set_seed(seed.clone());
        Ok(Box::new(()))
    }

    fn clone_state(&self, _: &Box<Any + Send>) -> Box<Any + Send> {
        Box::new(())
    }

    fn next_state(
        &mut self,
        _: &test_case::Interesting,
        _: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>> {
        Ok(Some(Box::new(())))
    }

    fn next_state_on_interesting(
        &mut self,
        new_seed: &test_case::Interesting,
        _old_seed: &test_case::Interesting,
        _prev_state: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>> {
        Ok(Some(self.new_state(new_seed)?))
    }

    fn reduce(
        &mut self,
        _: &test_case::Interesting,
        _: &Box<Any + Send>
    ) -> error::Result<Option<test_case::PotentialReduction>> {
        assert!(
            self.seed.is_some(),
            "Must be initialized with calls to set_seed before asking for potential \
             reductions"
        );

        if self.child.is_none() {
            self.spawn_child()?;
        }

        match self.next_potential_reduction_impl() {
            result @ Ok(_) => result,
            result @ Err(_) => {
                self.kill_child();
                result
            }
        }
    }
}

/// A reducer which ends after the first `Ok(None)` or `Err`.
///
/// Analogous to [`std::iter::Iterator::fuse`][iterfuse]. The `Fuse` combinator
/// ensures that once a reducer has either yielded an error or signaled
/// exhaustion, that it will always return `Ok(None)` forever after, until it is
/// reconfigured with a new seed or state.
///
/// [iterfuse]: https://doc.rust-lang.org/nightly/std/iter/trait.Iterator.html#method.fuse
///
/// ### Example
///
/// ```
/// extern crate preduce;
/// use preduce::traits::Reducer;
///
/// # fn main() { fn _foo() -> preduce::error::Result<()> {
/// let script = preduce::reducers::Script::new("/path/to/some/reducer/script")?;
/// let mut fused = preduce::reducers::Fuse::new(script);
///
/// # let some_seed_test_case = || unimplemented!();
/// let seed = some_seed_test_case();
/// let mut state = fused.new_state(&seed)?;
///
/// while let Some(reduction) = fused.reduce(&seed, &state)? {
///     println!("A potential reduction is {:?}", reduction);
///
///     // Advance to the next state. Alternatively, if this reduction was
///     // interesting, use `next_state_on_interesting`.
///     state = match fused.next_state(&seed, &state)? {
///         None => break,
///         Some(s) => s,
///     };
/// }
///
/// // This will always hold true until `fused` is reconfigured with some new
/// // seed or state.
/// assert_eq!(fused.reduce(&seed, &state).unwrap(), None);
/// assert_eq!(fused.reduce(&seed, &state).unwrap(), None);
/// assert_eq!(fused.reduce(&seed, &state).unwrap(), None);
/// # Ok(()) } }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fuse<R> {
    inner: R,
}

#[derive(Debug)]
enum FuseState {
    Finished,
    NotFinished(Box<Any + Send>),
}

impl<R> Fuse<R> {
    /// Ensure that the given `reducer` ends after having emitted `Ok(None)` or
    /// `Err`.
    pub fn new(inner: R) -> Fuse<R> {
        Fuse {
            inner: inner,
        }
    }

    fn downcast<'a, 'b>(&'a self, state: &'b Box<Any + Send>) -> &'b RefCell<FuseState> {
        state.downcast_ref::<RefCell<FuseState>>()
            .expect("Fuse::downcast given unexpected state")
    }
}

impl<R> Reducer for Fuse<R>
where
    R: Reducer,
{
    fn name(&self) -> Cow<str> {
        self.inner.name()
    }

    fn clone_boxed(&self) -> Box<Reducer>
    where
        Self: 'static,
    {
        Box::new(Fuse {
            inner: self.inner.clone_boxed(),
        })
    }

    fn new_state(&mut self, seed: &test_case::Interesting) -> error::Result<Box<Any + Send>> {
        let inner = self.inner.new_state(seed)?;
        Ok(Box::new(RefCell::new(FuseState::NotFinished(inner))))
    }

    fn clone_state(&self, state: &Box<Any + Send>) -> Box<Any + Send> {
        let state = self.downcast(state);
        let state = state.borrow();
        match *state {
            FuseState::Finished => Box::new(RefCell::new(FuseState::Finished)),
            FuseState::NotFinished(ref inner) => {
                Box::new(RefCell::new(FuseState::NotFinished(self.inner.clone_state(inner))))
            }
        }
    }

    fn next_state(
        &mut self,
        seed: &test_case::Interesting,
        prev_state: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>> {
        let prev_state = self.downcast(prev_state);
        let mut prev_state = prev_state.borrow_mut();

        let result = match *prev_state {
            FuseState::Finished => return Ok(None),
            FuseState::NotFinished(ref inner) => self.inner.next_state(seed, inner),
        };

        match result {
            Ok(Some(inner)) => {
                Ok(Some(Box::new(RefCell::new(FuseState::NotFinished(inner)))))
            }
            result @ Ok(None) | result @ Err(_) => {
                *prev_state = FuseState::Finished;
                result
            }
        }
    }

    fn next_state_on_interesting(
        &mut self,
        new_seed: &test_case::Interesting,
        old_seed: &test_case::Interesting,
        prev_state: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>> {
        let prev_state = self.downcast(prev_state);
        let mut prev_state = prev_state.borrow_mut();

        let result = match *prev_state {
            FuseState::Finished => return Ok(None),
            FuseState::NotFinished(ref inner) => {
                self.inner.next_state_on_interesting(new_seed, old_seed, inner)
            }
        };

        match result {
            Ok(Some(inner)) => {
                Ok(Some(Box::new(RefCell::new(FuseState::NotFinished(inner)))))
            }
            result @ Ok(None) | result @ Err(_) => {
                *prev_state = FuseState::Finished;
                result
            }
        }
    }

    fn fast_forward_states(
        &mut self,
        seed: &test_case::Interesting,
        n: usize,
        prev_state: &Box<Any + Send>
    ) -> error::Result<Option<Box<Any + Send>>> {
        let prev_state = self.downcast(prev_state);
        let mut prev_state = prev_state.borrow_mut();

        let result = match *prev_state {
            FuseState::Finished => return Ok(None),
            FuseState::NotFinished(ref inner) => {
                self.inner.fast_forward_states(seed, n, inner)
            }
        };

        match result {
            Ok(Some(inner)) => {
                Ok(Some(Box::new(RefCell::new(FuseState::NotFinished(inner)))))
            }
            result @ Ok(None) | result @ Err(_) => {
                *prev_state = FuseState::Finished;
                result
            }
        }
    }

    fn reduce(
        &mut self,
        seed: &test_case::Interesting,
        state: &Box<Any + Send>
    ) -> error::Result<Option<test_case::PotentialReduction>> {
        let state = self.downcast(state);
        let mut state = state.borrow_mut();

        let result = match *state {
            FuseState::Finished => return Ok(None),
            FuseState::NotFinished(ref inner) => self.inner.reduce(seed, inner),
        };

        match result {
            result @ Ok(Some(_)) => result,
            result @ Ok(None) | result @ Err(_) => {
                *state = FuseState::Finished;
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use std::env;
    use test_case;
    use test_utils::*;
    use traits::Reducer;

    fn with_counting_iterations<F, T>(n: u8, mut f: F) -> T
    where
        F: FnMut() -> T
    {
        use std::sync::Mutex;

        lazy_static! {
            static ref ENV_COUNT_MUTEX: Mutex<()> = Mutex::new(());
        }

        let _lock = ENV_COUNT_MUTEX.lock().unwrap();

        let old = env::var("PREDUCE_COUNTING_ITERATIONS").unwrap_or("".into());
        env::set_var("PREDUCE_COUNTING_ITERATIONS", n.to_string());

        let ret = f();

        env::set_var("PREDUCE_COUNTING_ITERATIONS", old);

        ret
    }

    #[test]
    fn script() {
        with_counting_iterations(10, || {
            let mut reducer = Script::new(get_reducer("counting.sh")).unwrap();
            let seed = test_case::Interesting::testing_only_new();
            let mut state = reducer.new_state(&seed).unwrap();

            let reduction = reducer.reduce(&seed, &state)
                .unwrap()
                .unwrap();
            assert!(reduction.path().is_file());

            for _ in 1..10 {
                state = reducer.next_state(&seed, &state).unwrap().unwrap();
                let reduction = reducer.reduce(&seed, &state)
                    .unwrap()
                    .unwrap();
                assert!(reduction.path().is_file());
            }

            state = reducer.next_state(&seed, &state).unwrap().unwrap();
            assert!(reducer.reduce(&seed, &state)
                    .unwrap()
                    .is_none());
        });
    }

    #[test]
    fn fuse() {
        #[derive(Clone, Debug)]
        struct Erratic(usize);

        impl Reducer for Erratic {
            fn name(&self) -> Cow<str> {
                Cow::from("Erratic")
            }

            fn clone_boxed(&self) -> Box<Reducer>
            where
                Self: 'static,
            {
                Box::new(self.clone())
            }

            fn new_state(&mut self, _: &test_case::Interesting) -> error::Result<Box<Any + Send>> {
                Ok(Box::new(()))
            }

            fn clone_state(&self, _: &Box<Any + Send>) -> Box<Any + Send> {
                Box::new(())
            }

            fn next_state(
                &mut self,
                _seed: &test_case::Interesting,
                _prev_state: &Box<Any + Send>
            ) -> error::Result<Option<Box<Any + Send>>> {
                Ok(Some(Box::new(())))
            }

            fn next_state_on_interesting(
                &mut self,
                _new_seed: &test_case::Interesting,
                _old_seed: &test_case::Interesting,
                _prev_state: &Box<Any + Send>
            ) -> error::Result<Option<Box<Any + Send>>> {
                Ok(Some(Box::new(())))
            }

            fn reduce(
                &mut self,
                _seed: &test_case::Interesting,
                _state: &Box<Any + Send>
            ) -> error::Result<Option<test_case::PotentialReduction>> {
                let result = match self.0 % 3 {
                    0 => Ok(Some(test_case::PotentialReduction::testing_only_new())),
                    1 => Ok(None),
                    2 => Err(error::Error::MisbehavingReducerScript("TEST".into())),
                    _ => unreachable!(),
                };
                self.0 += 1;
                result
            }
        }

        let seed = test_case::Interesting::testing_only_new();

        let mut reducer = Erratic(0);
        let state = reducer.new_state(&seed).unwrap();
        assert!(reducer.reduce(&seed, &state).unwrap().is_some());
        assert!(reducer.reduce(&seed, &state).unwrap().is_none());
        assert!(reducer.reduce(&seed, &state).is_err());
        assert!(reducer.reduce(&seed, &state).unwrap().is_some());

        let mut reducer = Fuse::new(Erratic(0));
        let state = reducer.new_state(&seed).unwrap();
        assert!(reducer.reduce(&seed, &state).unwrap().is_some());
        assert!(reducer.reduce(&seed, &state).unwrap().is_none());
        assert!(reducer.reduce(&seed, &state).unwrap().is_none());
        assert!(reducer.reduce(&seed, &state).unwrap().is_none());
    }

    #[test]
    fn not_executable() {
        match Script::new("./tests/fixtures/lorem-ipsum.txt") {
            Err(error::Error::IsNotExecutable(_)) => {}
            otherwise => {
                panic!("Expected Error::IsNotExecutable, found {:?}", otherwise);
            }
        }
    }
}
