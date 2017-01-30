//! Implementations of the `IsInteresting` trait.

use error;
use std::ffi;
use std::fs;
use std::path;
use std::process;
use traits::IsInteresting;

/// An `IsInteresting` implementation that rejects empty test cases, and accepts
/// all others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonEmpty;

impl IsInteresting for NonEmpty {
    fn is_interesting(&self, potential_reduction: &path::Path) -> error::Result<bool> {
        let len = fs::File::open(potential_reduction)?
            .metadata()?
            .len();
        Ok(len != 0)
    }
}

/// Spawn a subprocess that runs a user-provided script to determine whether a
/// test case is interesting.
///
/// Subprocesses that exit with `0` are interesting, all other exit codes are
/// interpreted as not interesting.
///
/// The user-provided script is given a single argument: a relative path to the
/// test case file it should test.
///
/// ### Example
///
/// Suppose we have some custom test script, `my_test.sh`:
///
/// ```bash
/// #!/usr/bin/env bash
///
/// # Note that `grep` exits 0 if it found any matches, 1 otherwise. This
/// # is-interesting predicate script should allow us to reduce a test case to
/// # something close to containing only the word "magic"!
///
/// grep magic "$1"
/// ```
///
/// Then in our Rust code, we would construct and use the
/// `preduce::interesting::Script` like this:
///
/// ```
/// extern crate preduce;
/// use preduce::traits::IsInteresting;
/// # fn main() {
/// # fn _foo() {
///
/// // Construct the is-interesting test that uses `my_test.sh`.
/// let test = preduce::interesting::Script::new("/path/to/my_test.sh");
///
/// // Now run the test on some random data.
/// # fn get_some_random_test_case() -> &'static ::std::path::Path { unimplemented!() }
/// let test_case = get_some_random_test_case();
/// if test.is_interesting(test_case).unwrap() {
///     println!("It is interesting! Must be magic!");
/// } else {
///     println!("Not magical -- get rid of it!");
/// }
/// # }
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    program: ffi::OsString,
}

impl Script {
    /// Construct a new `Script` is-interesting test that runs the given program.
    pub fn new<S>(program: S) -> Script where S: Into<ffi::OsString> {
        Script {
            program: program.into(),
        }
    }
}

impl IsInteresting for Script {
    fn is_interesting(&self, potential_reduction: &path::Path) -> error::Result<bool> {
        let mut cmd = process::Command::new(&self.program);

        cmd.stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .stdin(process::Stdio::null());

        match (potential_reduction.parent(), potential_reduction.file_name()) {
            (Some(dir), Some(file)) => {
                cmd.current_dir(dir).arg(file);
            }
            _ => {
                cmd.arg(potential_reduction);
            }
        }

        Ok(cmd.spawn()?.wait()?.success())
    }
}

#[cfg(test)]
mod tests {
    extern crate tempfile;

    use std::env;
    use std::io::Write;
    use std::path;
    use super::*;

    #[test]
    fn non_empty_file_is_interesting() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(&mut *tmp, "z").unwrap();
        let is_interesting = NonEmpty.is_interesting(tmp.path()).unwrap();
        assert!(is_interesting);
    }

    #[test]
    fn empty_file_is_not_interesting() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let is_interesting = NonEmpty.is_interesting(tmp.path()).unwrap();
        assert!(!is_interesting);
    }

    #[test]
    fn exit_zero_is_interesting() {
        let mut script = path::PathBuf::new();
        if let Ok(dir) = env::var("CARGO_MANIFEST_DIR") {
            script.push(dir);
        }
        script.push("tests/exit_0.sh");

        let test = Script::new(script);
        let test_case = tempfile::NamedTempFile::new().unwrap();
        assert!(test.is_interesting(test_case.path()).unwrap());
    }

    #[test]
    fn exit_non_zero_is_not_interesting() {
        let mut script = path::PathBuf::new();
        if let Ok(dir) = env::var("CARGO_MANIFEST_DIR") {
            script.push(dir);
        }
        script.push("tests/exit_1.sh");

        let test = Script::new(script);
        let test_case = tempfile::NamedTempFile::new().unwrap();
        assert!(!test.is_interesting(test_case.path()).unwrap());
    }
}
