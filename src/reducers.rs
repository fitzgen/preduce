//! Concrete implementations of `preduce::traits::Reducer`.

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
        child_stdout.read_line(&mut path)?;

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

#[cfg(test)]
mod tests {
    extern crate tempdir;
    extern crate tempfile;

    use super::*;
    use test_utils::*;
    use traits::Reducer;

    #[test]
    fn script_generates_potential_reductions() {
        let mut reducer = Script::new(get_script("counting.sh"));

        let seed = tempfile::NamedTempFile::new().unwrap();
        reducer.set_seed(seed.path());

        let tmpdir = tempdir::TempDir::new("counting").unwrap();
        reducer.set_out_dir(tmpdir.path());

        for _ in 0..5 {
            let path = reducer.next_potential_reduction();
            let path = path.unwrap().unwrap();
            assert!(path.is_file());
        }

        assert!(reducer.next_potential_reduction().unwrap().is_none());
    }
}
