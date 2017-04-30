//! Concrete implementations of `preduce::traits::Reducer`.

extern crate rand;

use error;
use std::ffi;
use std::io::{Read, Write};
use std::path;
use std::process;
use std::sync::Arc;
use tempdir;
use test_case::{self, TestCaseMethods};
use traits::Reducer;

impl Reducer for Box<Reducer> {
    fn set_seed(&mut self, seed: test_case::Interesting) {
        (**self).set_seed(seed)
    }

    fn next_potential_reduction(&mut self) -> error::Result<Option<test_case::PotentialReduction>> {
        (**self).next_potential_reduction()
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
/// # fn main() { fn _foo() {
/// let mut script = preduce::reducers::Script::new("/path/to/reducer/script");
///
/// # let some_seed_test_case = || unimplemented!();
/// script.set_seed(some_seed_test_case());
///
/// while let Some(reduction) = script.next_potential_reduction().unwrap() {
///     println!("A potential reduction is {:?}", reduction);
/// }
/// # } }
/// ```
#[derive(Debug)]
pub struct Script {
    program: ffi::OsString,
    out_dir: Option<Arc<tempdir::TempDir>>,
    counter: usize,
    seed: Option<test_case::Interesting>,
    child: Option<process::Child>,
    child_stdout: Option<process::ChildStdout>,
    strict: bool
}

impl Script {
    /// Construct a reducer script with the given `program`.
    pub fn new<S>(program: S) -> Script
    where
        S: Into<ffi::OsString>,
    {
        Script {
            program: program.into(),
            out_dir: None,
            counter: 0,
            seed: None,
            child: None,
            child_stdout: None,
            strict: false
        }
    }

    /// Enable or disable extra strict checks on the reducer script.
    ///
    /// For example, enforce that generated reductions are smaller than the
    /// seed.
    pub fn set_strict(&mut self, be_strict: bool) {
        self.strict = be_strict;
    }

    fn spawn_child(&mut self) -> error::Result<()> {
        assert!(self.seed.is_some());
        assert!(self.out_dir.is_none() && self.child.is_none() && self.child_stdout.is_none());

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

    fn kill_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
        self.child_stdout = None;
        self.out_dir = None;
    }

    fn next_temp_file(&mut self) -> error::Result<test_case::TempFile> {
        let file_path = path::PathBuf::from(self.counter.to_string());
        self.counter += 1;
        test_case::TempFile::new(self.out_dir.as_ref().unwrap().clone(), file_path)
    }

    fn next_potential_reduction_impl(&mut self,)
        -> error::Result<Option<test_case::PotentialReduction>> {
        assert!(self.out_dir.is_some() && self.child.is_some() && self.child_stdout.is_some());

        let temp_file = self.next_temp_file()
            .or_else(
                |e| {
                    self.kill_child();
                    Err(e)
                }
            )?;

        // Write the desired path of the next reduction to the child's stdin. If
        // this fails, then the child already exited, presumably because it
        // determined it could not generate any reductions from the test file.
        if {
               let mut child = self.child.as_mut().unwrap();
               let mut child_stdin = child.stdin.as_mut().unwrap();
               write!(child_stdin, "{}\n", temp_file.path().display()).is_err()
           } {
            self.kill_child();
            return Ok(None);
        }

        // Read the newline response from the child's stdout, indicating that
        // the child has finished generating the reduction.
        let mut newline = [0];
        if {
               let mut child_stdout = self.child_stdout.as_mut().unwrap();
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

        let reduction = test_case::PotentialReduction::new(
            self.seed.clone().unwrap(),
            self.program.to_string_lossy(),
            temp_file
        )?;

        if self.strict {
            let seed_size = self.seed.as_ref().unwrap().size();
            if reduction.size() >= seed_size {
                self.kill_child();
                let details = format!(
                    "'{}' is generating reductions that are greater than or equal the seed's size: {} >= {}",
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
    fn set_seed(&mut self, seed: test_case::Interesting) {
        self.seed = Some(seed);

        // If we have an extant child process, kill it now. We'll start a new
        // child process with the new seed the next time
        // `next_potential_reduction` is invoked.
        self.kill_child();
    }

    fn next_potential_reduction(&mut self) -> error::Result<Option<test_case::PotentialReduction>> {
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

/// Shuffle the order of the generated reductions from the reducer `R`.
///
/// Reducers generally tend to produce reductions starting at the beginning of
/// the seed test case and then, as they are drained, generate reductions
/// towards the end of the seed test case. This behavior can cause more merge
/// conflicts than is otherwise necessary.
///
/// The `Shuffle` reducer combinator helps alleviate this issue: it eagerly
/// generates potential reductions from its sub-reducer and then shuffles the
/// reductions as `next_potential_reduction` is called.
///
/// ### Example
///
/// ```
/// extern crate preduce;
/// use preduce::traits::Reducer;
///
/// # fn main() { fn _foo() {
/// // Take some extant reducer.
/// let reducer = preduce::reducers::Script::new("/path/to/reducer/script");
///
/// // And then use `Shuffle` to randomly reorder its generated reductions in
/// // batches of 100 at a time.
/// let mut shuffled = preduce::reducers::Shuffle::new(100, reducer);
///
/// # let some_seed_test_case = || unimplemented!();
/// # let some_out_dir = || unimplemented!();
/// shuffled.set_seed(some_seed_test_case());
///
/// while let Some(reduction) = shuffled.next_potential_reduction().unwrap() {
///     println!("A potential reduction is {:?}", reduction);
/// }
/// # } }
/// ```
#[derive(Clone, Debug)]
pub struct Shuffle<R> {
    reducer: R,
    buffer: Vec<test_case::PotentialReduction>
}

impl<R> Shuffle<R> {
    /// Given a reducer `R`, shuffle its reductions in batches of `capacity` at
    /// a time.
    pub fn new(capacity: usize, reducer: R) -> Shuffle<R> {
        assert!(capacity > 0);
        Shuffle {
            reducer: reducer,
            buffer: Vec::with_capacity(capacity)
        }
    }
}

impl<R> Reducer for Shuffle<R>
where
    R: Reducer,
{
    fn set_seed(&mut self, seed: test_case::Interesting) {
        self.buffer.clear();
        self.reducer.set_seed(seed);
    }

    fn next_potential_reduction(&mut self) -> error::Result<Option<test_case::PotentialReduction>> {
        if self.buffer.is_empty() {
            for _ in 0..self.buffer.capacity() {
                match self.reducer.next_potential_reduction() {
                    Ok(None) => break,
                    Ok(Some(path)) => self.buffer.push(path),
                    Err(e) => return Err(e),
                }
            }

            let capacity = self.buffer.capacity();
            let shuffled = rand::sample(&mut rand::thread_rng(), self.buffer.drain(..), capacity);
            self.buffer = shuffled;
        }

        Ok(self.buffer.pop())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChainState {
    /// Currently pulling from the first reducer. Second is queued up.
    First,

    /// We exhausted the first reducer, and are now pulling from the second.
    Second,

    /// We exhausted both reducers.
    Done
}

/// Generate reductions from `T`, followed by reductions from `U`.
///
/// The `Chain` reducer combinator concatenates all of `T`'s generated
/// reductions with all of `U`s generated reductions. The resulting reductions
/// will always be emitted in order, such that `T` is exhausted before `U` is
/// first used.
///
/// ### Example
///
/// ```
/// extern crate preduce;
/// use preduce::traits::Reducer;
///
/// # fn main() { fn _foo() {
/// let first = preduce::reducers::Script::new("/path/to/first/reducer/script");
/// let second = preduce::reducers::Script::new("/path/to/second/reducer/script");
/// let mut chained = preduce::reducers::Chain::new(first, second);
///
/// # let some_seed_test_case = || unimplemented!();
/// # let some_out_dir = || unimplemented!();
/// chained.set_seed(some_seed_test_case());
///
/// while let Some(reduction) = chained.next_potential_reduction().unwrap() {
///     println!("A potential reduction is {:?}", reduction);
/// }
/// # } }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chain<T, U> {
    first: T,
    second: U,
    state: ChainState
}

impl<T, U> Chain<T, U> {
    /// Construct the concatenated `T · U` reducer.
    pub fn new(first: T, second: U) -> Chain<T, U> {
        Chain {
            first: first,
            second: second,
            state: ChainState::First
        }
    }
}

impl<T, U> Reducer for Chain<T, U>
where
    T: Reducer,
    U: Reducer,
{
    fn set_seed(&mut self, seed: test_case::Interesting) {
        self.first.set_seed(seed.clone());
        self.second.set_seed(seed);
        self.state = ChainState::First;
    }

    fn next_potential_reduction(&mut self) -> error::Result<Option<test_case::PotentialReduction>> {
        match self.state {
            ChainState::First => {
                match self.first.next_potential_reduction() {
                    Err(e) => Err(e),
                    Ok(Some(reduction)) => Ok(Some(reduction)),
                    Ok(None) => {
                        self.state = ChainState::Second;
                        self.next_potential_reduction()
                    }
                }
            }
            ChainState::Second => {
                match self.second.next_potential_reduction() {
                    Err(e) => Err(e),
                    Ok(Some(reduction)) => Ok(Some(reduction)),
                    Ok(None) => {
                        self.state = ChainState::Done;
                        Ok(None)
                    }
                }
            }
            ChainState::Done => Ok(None),
        }
    }
}

/// A reducer which ends after the first `Ok(None)` or `Err`.
///
/// Analogous to [`std::iter::Iterator::fuse`][iterfuse]. The `Fuse` combinator
/// ensures that once a reducer has either yielded an error or signaled
/// exhaustion, that it will always return `Ok(None)` forever after, until it is
/// reconfigured with `set_seed`.
///
/// [iterfuse]: https://doc.rust-lang.org/nightly/std/iter/trait.Iterator.html#method.fuse
///
/// ### Example
///
/// ```
/// extern crate preduce;
/// use preduce::traits::Reducer;
///
/// # fn main() { fn _foo() {
/// let script = preduce::reducers::Script::new("/path/to/some/reducer/script");
/// let mut fused = preduce::reducers::Fuse::new(script);
///
/// # let some_seed_test_case = || unimplemented!();
/// # let some_out_dir = || unimplemented!();
/// fused.set_seed(some_seed_test_case());
///
/// while let Ok(Some(reduction)) = fused.next_potential_reduction() {
///     println!("A potential reduction is {:?}", reduction);
/// }
///
/// // This will always hold true until `fused` is reconfigured with `set_seed`.
/// assert!(fused.next_potential_reduction().unwrap().is_none());
/// assert!(fused.next_potential_reduction().unwrap().is_none());
/// assert!(fused.next_potential_reduction().unwrap().is_none());
/// # } }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fuse<R> {
    reducer: R,
    finished: bool
}

impl<R> Fuse<R> {
    /// Ensure that the given `reducer` ends after having emitted `Ok(None)` or
    /// `Err`.
    pub fn new(reducer: R) -> Fuse<R> {
        Fuse {
            reducer: reducer,
            finished: false
        }
    }
}

impl<R> Reducer for Fuse<R>
where
    R: Reducer,
{
    fn set_seed(&mut self, seed: test_case::Interesting) {
        self.reducer.set_seed(seed);
        self.finished = false;
    }

    fn next_potential_reduction(&mut self) -> error::Result<Option<test_case::PotentialReduction>> {
        if self.finished {
            return Ok(None);
        }

        match self.reducer.next_potential_reduction() {
            result @ Ok(Some(_)) => result,
            result @ Ok(None) |
            result @ Err(_) => {
                self.finished = true;
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use test_case;
    use test_utils::*;
    use traits::Reducer;

    #[test]
    fn script() {
        env::set_var("PREDUCE_COUNTING_ITERATIONS", "6");
        let mut reducer = Script::new(get_reducer("counting.sh"));

        reducer.set_seed(test_case::Interesting::testing_only_new());

        for _ in 0..6 {
            let reduction = reducer.next_potential_reduction().unwrap().unwrap();
            assert!(reduction.path().is_file());
        }

        assert!(reducer.next_potential_reduction().unwrap().is_none());
    }

    #[test]
    fn shuffle() {
        env::set_var("PREDUCE_COUNTING_ITERATIONS", "6");
        let reducer = Script::new(get_reducer("counting.sh"));
        let mut reducer = Shuffle::new(3, reducer);

        reducer.set_seed(test_case::Interesting::testing_only_new());

        let mut found = [false; 6];

        for _ in 0..3 {
            let reduction = reducer.next_potential_reduction().unwrap().unwrap();
            let mut contents = String::new();
            let mut file = fs::File::open(reduction.path()).expect("should open reduction file");
            file.read_to_string(&mut contents)
                .expect("should read file contents");

            match contents.trim() {
                "0" => found[0] = true,
                "1" => found[1] = true,
                "2" => found[2] = true,
                otherwise => panic!("Unexpected reduction: {:?}", otherwise),
            }
        }

        for _ in 0..3 {
            let reduction = reducer.next_potential_reduction().unwrap().unwrap();
            let mut contents = String::new();
            let mut file = fs::File::open(reduction.path()).expect("should open reduction file");
            file.read_to_string(&mut contents)
                .expect("should read file contents");

            match contents.trim() {
                "3" => found[3] = true,
                "4" => found[4] = true,
                "5" => found[5] = true,
                otherwise => panic!("Unexpected reduction: {:?}", otherwise),
            }
        }

        assert!(found.iter().all(|&found| found));
    }

    #[test]
    fn chain() {
        env::set_var("PREDUCE_COUNTING_ITERATIONS", "6");
        let first = Script::new(get_reducer("counting.sh"));
        let second = Script::new(get_reducer("alphabet.sh"));
        let mut reducer = Chain::new(first, second);

        reducer.set_seed(test_case::Interesting::testing_only_new());

        let mut next_file_contents = || {
            let reduction = reducer.next_potential_reduction().unwrap().unwrap();
            let mut contents = String::new();
            let mut file = fs::File::open(reduction.path()).expect("should open reduction file");
            file.read_to_string(&mut contents)
                .expect("should read file to string");
            contents.trim().to_string()
        };

        assert_eq!(next_file_contents(), "0");
        assert_eq!(next_file_contents(), "1");
        assert_eq!(next_file_contents(), "2");
        assert_eq!(next_file_contents(), "3");
        assert_eq!(next_file_contents(), "4");
        assert_eq!(next_file_contents(), "5");

        assert_eq!(next_file_contents(), "a");
        assert_eq!(next_file_contents(), "b");
        assert_eq!(next_file_contents(), "c");
        assert_eq!(next_file_contents(), "d");
        assert_eq!(next_file_contents(), "e");
        assert_eq!(next_file_contents(), "f");
    }

    #[test]
    fn fuse() {
        struct Erratic(usize);

        impl Reducer for Erratic {
            fn set_seed(&mut self, _: test_case::Interesting) {}

            fn next_potential_reduction(&mut self,)
                -> error::Result<Option<test_case::PotentialReduction>> {
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

        let mut reducer = Erratic(0);
        assert!(reducer.next_potential_reduction().unwrap().is_some());
        assert!(reducer.next_potential_reduction().unwrap().is_none());
        assert!(reducer.next_potential_reduction().is_err());
        assert!(reducer.next_potential_reduction().unwrap().is_some());

        let mut reducer = Fuse::new(Erratic(0));
        assert!(reducer.next_potential_reduction().unwrap().is_some());
        assert!(reducer.next_potential_reduction().unwrap().is_none());
        assert!(reducer.next_potential_reduction().unwrap().is_none());
        assert!(reducer.next_potential_reduction().unwrap().is_none());
    }
}
