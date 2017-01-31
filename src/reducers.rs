//! Concrete implementations of `preduce::traits::Reducer`.

extern crate rand;

use error;
use std::ffi;
use std::io::{self, BufRead, Write};
use std::path;
use std::process;
use traits::Reducer;

/// A test case reducer that is implemented as an external script.
///
/// ### IPC Protocol
///
/// The seed test case is given as the first and only argument to the script.
///
/// When `preduce` would like the next potential reduction of the seed test case
/// to be generated, it will write a '\n' byte to `stdin`. Upon reading this
/// newline, the script should generate the next reduction at a unique path
/// within its current directory, and print this path followed by a '\n' to
/// `stdout`. Alternatively, if the subprocess has exhausted all of its
/// potential reductions, then it may simply exit without printing anything.
///
/// All generated reduction's file paths must be encoded in valid UTF-8.
///
/// Scripts must not write files that are outside their current directory.
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
/// n=$(wc -l "$seed" | cut -c1)
///
/// # Generate a potential reduction of the seed's last line, then its last 2
/// # lines, then its last 3 lines, etc...
/// for (( i=1 ; i < n; i++ )); do
///     # Read the '\n' from stdin and ignore it.
///     read -r ignored
///
///     # Generate the potential reduction in a new file.
///     tail -n "$i" > "tail-$i"
///
///     # Tell `preduce` about the potential reduction.
///     echo "tail-$i"
/// }
/// ```
#[derive(Debug)]
pub struct Script {
    program: ffi::OsString,
    seed: Option<path::PathBuf>,
    out_dir: Option<path::PathBuf>,
    child: Option<process::Child>,
    child_stdout: Option<io::BufReader<process::ChildStdout>>,
}

impl Script {
    /// Construct a reducer script with the given `program`.
    pub fn new<S>(program: S) -> Script
        where S: Into<ffi::OsString>
    {
        Script {
            program: program.into(),
            seed: None,
            out_dir: None,
            child: None,
            child_stdout: None,
        }
    }

    fn spawn_child(&mut self) -> error::Result<()> {
        assert!(self.seed.is_some() && self.out_dir.is_some());
        assert!(self.child.is_none() && self.child_stdout.is_none());

        let mut cmd = process::Command::new(&self.program);
        cmd.current_dir(self.out_dir.as_ref().unwrap())
            .arg(self.seed.as_ref().unwrap())
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::null());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        self.child_stdout = Some(io::BufReader::new(stdout));
        self.child = Some(child);

        Ok(())
    }

    fn kill_child(&mut self) {
        self.child_stdout = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        self.kill_child();
    }
}

impl Reducer for Script {
    fn set_seed(&mut self, seed: &path::Path) {
        assert!(seed.is_file());
        self.seed = Some(seed.into());

        // If we have an extant child process, kill it now. We'll start a new
        // child process with the new seed the next time
        // `next_potential_reduction` is invoked.
        self.kill_child();
    }

    fn set_out_dir(&mut self, out_dir: &path::Path) {
        assert!(out_dir.is_dir());
        self.out_dir = Some(out_dir.into());

        // Same as with `set_seed`.
        self.kill_child();
    }

    fn next_potential_reduction(&mut self) -> error::Result<Option<path::PathBuf>> {
        assert!(self.seed.is_some() && self.out_dir.is_some(),
                "Must be initialized with calls to set_{seed,out_dir} before \
                 asking for potential reductions");

        if self.child.is_none() {
            self.spawn_child()?;
        }

        assert!(self.child.is_some() && self.child_stdout.is_some());

        let mut child = self.child.as_mut().unwrap();
        write!(child.stdin.as_mut().unwrap(), "\n")?;

        let mut child_stdout = self.child_stdout.as_mut().unwrap();
        let mut path = String::new();
        if let Err(_) = child_stdout.read_line(&mut path) {
            return Ok(None);
        }

        if path.len() == 0 {
            return Ok(None);
        }

        if path.pop() != Some('\n') {
            let details = format!("'{}' is not conforming to the reducer script protocol: \
                                   expected a trailing newline",
                                  self.program.to_string_lossy());
            return Err(error::Error::MisbehavingReducerScript(details));
        }

        let path: path::PathBuf = path.into();
        let path = if path.is_relative() {
            let mut abs = self.out_dir.clone().unwrap();
            abs.push(path);
            abs.canonicalize()?
        } else {
            path.canonicalize()?
        };

        if !path.starts_with(self.out_dir.as_ref().unwrap()) {
            let details = format!("'{}' is generating test cases outside of its out directory: {}",
                                  self.program.to_string_lossy(),
                                  path.to_string_lossy());
            return Err(error::Error::MisbehavingReducerScript(details));
        }

        if !path.is_file() {
            let details = format!("'{}' is generating test cases that don't exist: {}",
                                  self.program.to_string_lossy(),
                                  path.to_string_lossy());
            return Err(error::Error::MisbehavingReducerScript(details));
        }

        Ok(Some(path))
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
/// let mut reducer = preduce::reducers::Shuffle::new(100, reducer);
///
/// while let Some(reduction) = reducer.next_potential_reduction().unwrap() {
///     println!("A potential reduction is {:?}", reduction);
/// }
/// # } }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shuffle<R> {
    reducer: R,
    buffer: Vec<path::PathBuf>,
}

impl<R> Shuffle<R> {
    /// Given a reducer `R`, shuffle its reductions in batches of `capacity` at
    /// a time.
    pub fn new(capacity: usize, reducer: R) -> Shuffle<R> {
        assert!(capacity > 0);
        Shuffle {
            reducer: reducer,
            buffer: Vec::with_capacity(capacity),
        }
    }
}

impl<R> Reducer for Shuffle<R>
    where R: Reducer
{
    fn set_seed(&mut self, seed: &path::Path) {
        self.buffer.clear();
        self.reducer.set_seed(seed);
    }

    fn set_out_dir(&mut self, out_dir: &path::Path) {
        self.buffer.clear();
        self.reducer.set_out_dir(out_dir);
    }

    fn next_potential_reduction(&mut self) -> error::Result<Option<path::PathBuf>> {
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
    Done,
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
/// chained.set_out_dir(some_out_dir());
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
    state: ChainState,
}

impl<T, U> Chain<T, U> {
    /// Construct the concatenated `T · U` reducer.
    pub fn new(first: T, second: U) -> Chain<T, U> {
        Chain {
            first: first,
            second: second,
            state: ChainState::First,
        }
    }
}

impl<T, U> Reducer for Chain<T, U>
    where T: Reducer,
          U: Reducer
{
    fn set_seed(&mut self, seed: &path::Path) {
        self.first.set_seed(seed);
        self.second.set_seed(seed);
        self.state = ChainState::First;
    }

    fn set_out_dir(&mut self, out_dir: &path::Path) {
        self.first.set_out_dir(out_dir);
        self.second.set_out_dir(out_dir);
        self.state = ChainState::First;
    }

    fn next_potential_reduction(&mut self) -> error::Result<Option<path::PathBuf>> {
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

#[cfg(test)]
mod tests {
    extern crate tempdir;
    extern crate tempfile;

    use std::env;
    use super::*;
    use test_utils::*;
    use traits::Reducer;

    #[test]
    fn script() {
        env::set_var("PREDUCE_COUNTING_ITERATIONS", "6");
        let mut reducer = Script::new(get_script("counting.sh"));

        let seed = tempfile::NamedTempFile::new().unwrap();
        reducer.set_seed(seed.path());

        let tmpdir = tempdir::TempDir::new("script").unwrap();
        reducer.set_out_dir(tmpdir.path());

        for _ in 0..6 {
            let path = reducer.next_potential_reduction();
            let path = path.unwrap().unwrap();
            assert!(path.is_file());
        }

        assert!(reducer.next_potential_reduction().unwrap().is_none());
    }

    #[test]
    fn shuffle() {
        env::set_var("PREDUCE_COUNTING_ITERATIONS", "6");
        let reducer = Script::new(get_script("counting.sh"));
        let mut reducer = Shuffle::new(3, reducer);

        let seed = tempfile::NamedTempFile::new().unwrap();
        reducer.set_seed(seed.path());

        let tmpdir = tempdir::TempDir::new("shuffle").unwrap();
        reducer.set_out_dir(tmpdir.path());

        let mut found = [false; 6];

        for _ in 0..3 {
            let reduction = reducer.next_potential_reduction().unwrap().unwrap();
            let file_name = reduction.file_name().map(|s| s.to_string_lossy().into_owned());
            match file_name.as_ref().map(|s| &s[..]) {
                Some("counting-0") => found[0] = true,
                Some("counting-1") => found[1] = true,
                Some("counting-2") => found[2] = true,
                otherwise => panic!("Unexpected reduction: {:?}", otherwise),
            }
        }

        for _ in 0..3 {
            let reduction = reducer.next_potential_reduction().unwrap().unwrap();
            let file_name = reduction.file_name().map(|s| s.to_string_lossy().into_owned());
            match file_name.as_ref().map(|s| &s[..]) {
                Some("counting-3") => found[3] = true,
                Some("counting-4") => found[4] = true,
                Some("counting-5") => found[5] = true,
                otherwise => panic!("Unexpected reduction: {:?}", otherwise),
            }
        }

        assert!(found.iter().all(|&found| found));
    }

    #[test]
    fn chain() {
        env::set_var("PREDUCE_COUNTING_ITERATIONS", "6");
        let first = Script::new(get_script("counting.sh"));
        let second = Script::new(get_script("alphabet.sh"));
        let mut reducer = Chain::new(first, second);

        let seed = tempfile::NamedTempFile::new().unwrap();
        reducer.set_seed(seed.path());

        let tmpdir = tempdir::TempDir::new("shuffle").unwrap();
        reducer.set_out_dir(tmpdir.path());

        let mut next_file_name = || {
            let reduction = reducer.next_potential_reduction().unwrap().unwrap();
            reduction.file_name().unwrap().to_string_lossy().into_owned()
        };

        assert_eq!(next_file_name(), "counting-0");
        assert_eq!(next_file_name(), "counting-1");
        assert_eq!(next_file_name(), "counting-2");
        assert_eq!(next_file_name(), "counting-3");
        assert_eq!(next_file_name(), "counting-4");
        assert_eq!(next_file_name(), "counting-5");

        assert_eq!(next_file_name(), "alphabet-a");
        assert_eq!(next_file_name(), "alphabet-b");
        assert_eq!(next_file_name(), "alphabet-c");
        assert_eq!(next_file_name(), "alphabet-d");
        assert_eq!(next_file_name(), "alphabet-e");
        assert_eq!(next_file_name(), "alphabet-f");
    }
}
