//! Concrete implementations of `preduce::traits::Reducer`.

use error;
use is_executable::IsExecutable;
use preduce_ipc_types::{FastForwardRequest, NewRequest, NextOnInterestingRequest, NextRequest,
                        ReduceRequest, Request};
use preduce_ipc_types::{FastForwardResponse, NewResponse, NextOnInterestingResponse, NextResponse,
                        ReduceResponse, Response};
use serde_json;
use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::io::{self, BufRead, Write};
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
        Self: 'static,
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
        prev_state: &Box<Any + Send>,
    ) -> error::Result<Option<Box<Any + Send>>> {
        (**self).next_state(seed, prev_state)
    }

    fn next_state_on_interesting(
        &mut self,
        new_seed: &test_case::Interesting,
        old_seed: &test_case::Interesting,
        prev_state: &Box<Any + Send>,
    ) -> error::Result<Option<Box<Any + Send>>> {
        (**self).next_state_on_interesting(new_seed, old_seed, prev_state)
    }

    fn fast_forward_states(
        &mut self,
        seed: &test_case::Interesting,
        n: usize,
        prev_state: &Box<Any + Send>,
    ) -> error::Result<Option<Box<Any + Send>>> {
        (**self).fast_forward_states(seed, n, prev_state)
    }

    fn reduce(
        &mut self,
        seed: &test_case::Interesting,
        state: &Box<Any + Send>,
    ) -> error::Result<Option<test_case::Candidate>> {
        (**self).reduce(seed, state)
    }
}

/// A test case reducer that is implemented as an external script.
///
/// See the `preduce_ipc_types` crate's documentation for information on the IPC
/// protocol.
///
/// See the `preduce_reducer_script` crate's documentation for example reducer
/// scripts.
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
/// // Get some known-interesting seed test case to create candidates from.
/// let seed = some_seed_test_case();
///
/// // Get the initial state for the given seed.
/// let mut state = script.new_state(&seed)?;
///
/// while let Some(candidate) = script.reduce(&seed, &state)? {
///     println!("Here is a candidate: {:?}", candidate);
///
///     // Advance to the next state. Alternatively, if this candidate was
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
    child: Option<process::Child>,
    child_stdin: Option<io::BufWriter<process::ChildStdin>>,
    child_stdout: Option<io::BufReader<process::ChildStdout>>,
    strict: bool,
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
            child: None,
            child_stdin: None,
            child_stdout: None,
            strict: false,
        })
    }

    /// Enable or disable extra strict checks on the reducer script.
    ///
    /// For example, enforce that generated candidates are smaller than the
    /// seed.
    pub fn set_strict(&mut self, be_strict: bool) {
        self.strict = be_strict;
    }

    fn spawn_child(&mut self) -> error::Result<()> {
        assert!(self.out_dir.is_none());
        assert!(self.child.is_none());
        assert!(self.child_stdin.is_none());
        assert!(self.child_stdout.is_none());

        self.out_dir = Some(Arc::new(tempdir::TempDir::new("preduce-reducer-script")?));

        let mut cmd = process::Command::new(&self.program);
        cmd.current_dir(self.out_dir.as_ref().unwrap().path())
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped());

        let mut child = cmd.spawn()?;

        let stdin = child.stdin.take().unwrap();
        self.child_stdin = Some(io::BufWriter::with_capacity(1024 * 256, stdin));

        let stdout = child.stdout.take().unwrap();
        self.child_stdout = Some(io::BufReader::new(stdout));

        self.child = Some(child);

        Ok(())
    }

    /// Attempt to nicely tell the child to stop by sending it an empty line to
    /// use as the next "seed", whereupon it should exit cleanly, thus cleaning
    /// up any resources it was using (e.g. temporary files).
    fn shutdown_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            if (|| -> error::Result<()> {
                {
                    let mut child_stdin = self.child_stdin.as_mut().unwrap();
                    serde_json::to_writer(&mut child_stdin, &Request::Shutdown)?;
                    writeln!(&mut child_stdin)?;
                    child_stdin.flush()?;
                }
                self.child_stdin = None;
                child.wait()?;
                Ok(())
            })()
                .is_err()
            {
                self.kill_child();
            }
            self.child_stdout = None;
            self.out_dir = None;
        }
    }

    fn kill_child(&mut self) {
        self.child_stdin = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child_stdout = None;
        self.out_dir = None;
    }

    fn next_temp_file(&mut self) -> error::Result<test_case::TempFile> {
        let mut file_name = String::from("candidate");
        file_name.push_str(&self.counter.to_string());
        self.counter += 1;

        let file_path = path::PathBuf::from(file_name);
        test_case::TempFile::new(self.out_dir.as_ref().unwrap().clone(), file_path)
    }

    fn misbehaving_reducer_script<T>(&mut self, details: String) -> error::Result<T> {
        self.kill_child();
        Err(error::Error::MisbehavingReducerScript(details))
    }

    fn downcast(state: &Box<Any + Send>) -> &serde_json::Value {
        state.downcast_ref::<serde_json::Value>().unwrap()
    }

    fn request(&mut self, request: Request) -> error::Result<Response> {
        assert!(self.child.is_some());
        assert!(self.child_stdout.is_some());

        match (|| {
            let mut stdin = self.child_stdin.as_mut().unwrap();
            serde_json::to_writer(&mut stdin, &request)?;
            write!(&mut stdin, "\n")?;
            stdin.flush()?;

            let stdout = self.child_stdout.as_mut().unwrap();
            let mut line = String::new();
            stdout.read_line(&mut line)?;

            let response: Response = serde_json::from_str(&line)?;
            Ok(response)
        })()
        {
            r @ Ok(_) => r,
            e @ Err(_) => {
                self.kill_child();
                e
            }
        }
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        self.shutdown_child();
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
            child: None,
            child_stdin: None,
            child_stdout: None,
            strict: self.strict,
        })
    }

    fn new_state(&mut self, seed: &test_case::Interesting) -> error::Result<Box<Any + Send>> {
        if self.child.is_none() {
            self.spawn_child()?;
        }

        let response = self.request(Request::New(NewRequest {
            seed: seed.path().into(),
        }))?;
        match response {
            Response::New(NewResponse { state }) => Ok(Box::new(state)),
            otherwise => {
                let program = self.program.to_string_lossy().to_string();
                self.misbehaving_reducer_script(format!(
                    "Expected a `Response::New` in response to a `Request::New` request; \
                     got `{:?}` from '{}'",
                    otherwise,
                    program
                ))
            }
        }
    }

    fn clone_state(&self, state: &Box<Any + Send>) -> Box<Any + Send> {
        Box::new(Self::downcast(state).clone())
    }

    fn next_state(
        &mut self,
        seed: &test_case::Interesting,
        state: &Box<Any + Send>,
    ) -> error::Result<Option<Box<Any + Send>>> {
        // It's possible that we killed the child for misbehaving since we
        // generated this state, so we can't assert that the child exists.
        if self.child.is_none() {
            self.spawn_child()?;
        }

        let state = Self::downcast(state);
        let response = self.request(Request::Next(NextRequest {
            seed: seed.path().into(),
            state: state.clone(),
        }))?;

        match response {
            Response::Next(NextResponse { next_state }) => {
                Ok(next_state.map(|ns| Box::new(ns) as Box<Any + Send>))
            }
            otherwise => {
                let program = self.program.to_string_lossy().to_string();
                self.misbehaving_reducer_script(format!(
                    "Expected a `Response::Next` in response to a `Request::Next` request; \
                     got `{:?}` from '{}'",
                    otherwise,
                    program
                ))
            }
        }
    }

    fn next_state_on_interesting(
        &mut self,
        new_seed: &test_case::Interesting,
        old_seed: &test_case::Interesting,
        state: &Box<Any + Send>,
    ) -> error::Result<Option<Box<Any + Send>>> {
        if self.child.is_none() {
            self.spawn_child()?;
        }

        let state = Self::downcast(state);
        let response = self.request(Request::NextOnInteresting(NextOnInterestingRequest {
            new_seed: new_seed.path().into(),
            old_seed: old_seed.path().into(),
            state: state.clone(),
        }))?;

        match response {
            Response::NextOnInteresting(NextOnInterestingResponse { next_state }) => {
                Ok(next_state.map(|ns| Box::new(ns) as Box<Any + Send>))
            }
            otherwise => {
                let program = self.program.to_string_lossy().to_string();
                self.misbehaving_reducer_script(format!(
                    "Expected a `Response::NextOnInteresting` in response to a \
                     `Request::NextOnInteresting` request; got `{:?}` from '{}'",
                    otherwise,
                    program
                ))
            }
        }
    }

    fn fast_forward_states(
        &mut self,
        seed: &test_case::Interesting,
        n: usize,
        state: &Box<Any + Send>,
    ) -> error::Result<Option<Box<Any + Send>>> {
        if self.child.is_none() {
            self.spawn_child()?;
        }

        let state = Self::downcast(state);
        let response = self.request(Request::FastForward(FastForwardRequest {
            seed: seed.path().into(),
            n,
            state: state.clone(),
        }))?;

        match response {
            Response::FastForward(FastForwardResponse { next_state }) => {
                Ok(next_state.map(|ns| Box::new(ns) as Box<Any + Send>))
            }
            otherwise => {
                let program = self.program.to_string_lossy().to_string();
                self.misbehaving_reducer_script(format!(
                    "Expected a `Response::FastForward` in response to a `Request::FastForward` \
                     request; got `{:?}` from '{}'",
                    otherwise,
                    program
                ))
            }
        }
    }

    fn reduce(
        &mut self,
        seed: &test_case::Interesting,
        state: &Box<Any + Send>,
    ) -> error::Result<Option<test_case::Candidate>> {
        if self.child.is_none() {
            self.spawn_child()?;
        }

        let state = Self::downcast(state);
        let temp_file = self.next_temp_file()?;
        let response = self.request(Request::Reduce(ReduceRequest {
            seed: seed.path().into(),
            state: state.clone(),
            dest: temp_file.path().into(),
        }))?;

        match response {
            Response::Reduce(ReduceResponse { reduced: true }) => {
                if !temp_file.path().is_file() {
                    let program = self.program.to_string_lossy().to_string();
                    return self.misbehaving_reducer_script(format!(
                        "'{}' did not generate a test case file at {}",
                        program,
                        temp_file.path().display()
                    ));
                }
                Ok(Some(test_case::Candidate::new(
                    seed.clone(),
                    self.program.to_string_lossy(),
                    temp_file,
                )?))
            }
            Response::Reduce(ReduceResponse { reduced: false }) => Ok(None),
            otherwise => {
                let program = self.program.to_string_lossy().to_string();
                self.misbehaving_reducer_script(format!(
                    "Expected a `Response::Reduce` in response to a `Request::Reduce` request; \
                     got {:?} from '{}'",
                    otherwise,
                    program
                ))
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
/// while let Some(candidate) = fused.reduce(&seed, &state)? {
///     println!("A candidate is {:?}", candidate);
///
///     // Advance to the next state. Alternatively, if this candidate was
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
        Fuse { inner: inner }
    }

    fn downcast<'a, 'b>(&'a self, state: &'b Box<Any + Send>) -> &'b RefCell<FuseState> {
        state
            .downcast_ref::<RefCell<FuseState>>()
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
            FuseState::NotFinished(ref inner) => Box::new(RefCell::new(
                FuseState::NotFinished(self.inner.clone_state(inner)),
            )),
        }
    }

    fn next_state(
        &mut self,
        seed: &test_case::Interesting,
        prev_state: &Box<Any + Send>,
    ) -> error::Result<Option<Box<Any + Send>>> {
        let prev_state = self.downcast(prev_state);
        let mut prev_state = prev_state.borrow_mut();

        let result = match *prev_state {
            FuseState::Finished => return Ok(None),
            FuseState::NotFinished(ref inner) => self.inner.next_state(seed, inner),
        };

        match result {
            Ok(Some(inner)) => Ok(Some(Box::new(RefCell::new(FuseState::NotFinished(inner))))),
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
        prev_state: &Box<Any + Send>,
    ) -> error::Result<Option<Box<Any + Send>>> {
        let prev_state = self.downcast(prev_state);
        let mut prev_state = prev_state.borrow_mut();

        let result = match *prev_state {
            FuseState::Finished => return Ok(None),
            FuseState::NotFinished(ref inner) => {
                self.inner
                    .next_state_on_interesting(new_seed, old_seed, inner)
            }
        };

        match result {
            Ok(Some(inner)) => Ok(Some(Box::new(RefCell::new(FuseState::NotFinished(inner))))),
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
        prev_state: &Box<Any + Send>,
    ) -> error::Result<Option<Box<Any + Send>>> {
        let prev_state = self.downcast(prev_state);
        let mut prev_state = prev_state.borrow_mut();

        let result = match *prev_state {
            FuseState::Finished => return Ok(None),
            FuseState::NotFinished(ref inner) => self.inner.fast_forward_states(seed, n, inner),
        };

        match result {
            Ok(Some(inner)) => Ok(Some(Box::new(RefCell::new(FuseState::NotFinished(inner))))),
            result @ Ok(None) | result @ Err(_) => {
                *prev_state = FuseState::Finished;
                result
            }
        }
    }

    fn reduce(
        &mut self,
        seed: &test_case::Interesting,
        state: &Box<Any + Send>,
    ) -> error::Result<Option<test_case::Candidate>> {
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
    use test_case;
    use traits::Reducer;

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
                _prev_state: &Box<Any + Send>,
            ) -> error::Result<Option<Box<Any + Send>>> {
                Ok(Some(Box::new(())))
            }

            fn next_state_on_interesting(
                &mut self,
                _new_seed: &test_case::Interesting,
                _old_seed: &test_case::Interesting,
                _prev_state: &Box<Any + Send>,
            ) -> error::Result<Option<Box<Any + Send>>> {
                Ok(Some(Box::new(())))
            }

            fn reduce(
                &mut self,
                _seed: &test_case::Interesting,
                _state: &Box<Any + Send>,
            ) -> error::Result<Option<test_case::Candidate>> {
                let result = match self.0 % 3 {
                    0 => Ok(Some(test_case::Candidate::testing_only_new())),
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
