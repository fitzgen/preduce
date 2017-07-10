//! Implementations of the `IsInteresting` trait.

use error;
use std::fs;
use std::panic::UnwindSafe;
use std::path;
use std::process;
use traits::IsInteresting;

impl IsInteresting for Box<IsInteresting> {
    fn is_interesting(&self, potential_reduction: &path::Path) -> error::Result<bool> {
        (**self).is_interesting(potential_reduction)
    }

    fn clone(&self) -> Box<IsInteresting>
    where
        Self: 'static,
    {
        (**self).clone()
    }
}

/// An `IsInteresting` implementation that rejects empty test cases, and accepts
/// all others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonEmpty;

impl IsInteresting for NonEmpty {
    fn is_interesting(&self, potential_reduction: &path::Path) -> error::Result<bool> {
        let len = fs::File::open(potential_reduction)?.metadata()?.len();
        Ok(len != 0)
    }

    fn clone(&self) -> Box<IsInteresting>
    where
        Self: 'static,
    {
        Box::new(NonEmpty) as _
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
/// let test = preduce::interesting::Script::new("/path/to/my_test.sh").unwrap();
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
    program: path::PathBuf,
}

impl Script {
    /// Construct a new `Script` is-interesting test that runs the given program.
    pub fn new<S>(program: S) -> error::Result<Script>
    where
        S: AsRef<path::Path>,
    {
        if !program.as_ref().is_file() {
            return Err(error::Error::DoesNotExist(program.as_ref().into()));
        }

        let program = program.as_ref().canonicalize()?;
        Ok(Script { program: program })
    }
}

impl IsInteresting for Script {
    fn is_interesting(&self, potential_reduction: &path::Path) -> error::Result<bool> {
        assert!(potential_reduction.is_file());
        assert!(self.program.is_file());

        let mut cmd = process::Command::new(&self.program);
        cmd.stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .stdin(process::Stdio::null());

        match (
            potential_reduction.parent(),
            potential_reduction.file_name(),
        ) {
            (Some(dir), Some(file)) => {
                cmd.current_dir(dir).arg(file);
            }
            _ => {
                cmd.arg(potential_reduction);
            }
        }

        Ok(cmd.spawn()?.wait()?.success())
    }

    fn clone(&self) -> Box<IsInteresting> {
        Box::new(Clone::clone(self)) as _
    }
}

/// Given two is-interesting tests, combine them into a single is-interesting
/// test that returns `true` if both sub-is-interesting tests return `true`, and
/// `false` otherwise.
///
/// Beyond generally combining is-interesting tests, `And` provides
/// short-circuiting, which is helpful when one is-interesting test is
/// significantly faster than the other.
///
/// ### Example
///
/// ```
/// extern crate preduce;
/// use preduce::traits::IsInteresting;
/// # fn main() { fn _foo() {
///
/// let test = preduce::interesting::And::new(
///     // A relatively cheap check.
///     preduce::interesting::NonEmpty,
///     // An expensive check.
///     preduce::interesting::Script::new("/path/to/expensive/script").unwrap()
/// );
///
/// # fn get_some_random_test_case() -> &'static ::std::path::Path { unimplemented!() }
/// let test_case = get_some_random_test_case();
/// if test.is_interesting(test_case).unwrap() {
///     println!("Both is-interesting tests passed!");
/// } else {
///     println!("One or both is-interesting tests failed.");
/// }
/// # } }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct And<T, U> {
    first: T,
    second: U,
}

impl<T, U> And<T, U> {
    /// Combine `T` and `U` into a single `T && U` is-interesting test.
    pub fn new(first: T, second: U) -> And<T, U> {
        And {
            first: first,
            second: second,
        }
    }
}

impl<T, U> IsInteresting for And<T, U>
where
    T: IsInteresting,
    U: IsInteresting,
{
    fn is_interesting(&self, potential_reduction: &path::Path) -> error::Result<bool> {
        Ok(
            self.first.is_interesting(potential_reduction)? &&
                self.second.is_interesting(potential_reduction)?,
        )
    }

    fn clone(&self) -> Box<IsInteresting>
    where
        Self: 'static,
    {
        Box::new(And::new(self.first.clone(), self.second.clone())) as _
    }
}

/// Given two is-interesting tests, combine them into a single is-interesting
/// test that returns `true` if either sub-test returns `true`.
///
/// ### Example
///
/// ```
/// extern crate preduce;
/// use preduce::traits::IsInteresting;
/// # fn main() { fn _foo() {
///
/// let test = preduce::interesting::Or::new(
///     preduce::interesting::Script::new("/path/to/first/script").unwrap(),
///     preduce::interesting::Script::new("/path/to/second/script").unwrap()
/// );
///
/// # fn get_some_random_test_case() -> &'static ::std::path::Path { unimplemented!() }
/// let test_case = get_some_random_test_case();
/// if test.is_interesting(test_case).unwrap() {
///     // We know only one passed because we either short-circuited after the
///     // first successful test and did not run the second one, or the first
///     // failed and the second succeeded.
///     println!("One of the is-interesting tests passed!");
/// } else {
///     println!("Both is-interesting tests failed.");
/// }
/// # } }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Or<T, U> {
    first: T,
    second: U,
}

impl<T, U> Or<T, U> {
    /// Combine `T` and `U` into a single `T || U` is-interesting test.
    pub fn new(first: T, second: U) -> Or<T, U> {
        Or {
            first: first,
            second: second,
        }
    }
}

impl<T, U> IsInteresting for Or<T, U>
where
    T: IsInteresting,
    U: IsInteresting,
{
    fn is_interesting(&self, potential_reduction: &path::Path) -> error::Result<bool> {
        Ok(
            self.first.is_interesting(potential_reduction)? ||
                self.second.is_interesting(potential_reduction)?,
        )
    }

    fn clone(&self) -> Box<IsInteresting>
    where
        Self: 'static,
    {
        Box::new(And::new(self.first.clone(), self.second.clone())) as _
    }
}

impl<T> IsInteresting for T
where
    T: Clone + Send + UnwindSafe + for<'a> Fn(&'a path::Path) -> error::Result<bool>,
{
    fn is_interesting(&self, reduction: &path::Path) -> error::Result<bool> {
        (*self)(reduction)
    }

    fn clone(&self) -> Box<IsInteresting>
    where
        Self: 'static,
    {
        Box::new(self.clone()) as _
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path;
    use test_case;
    use test_utils::*;

    fn temp_file() -> test_case::TempFile {
        let test_case = test_case::TempFile::anonymous().unwrap();
        fs::File::create(test_case.path()).unwrap();
        test_case
    }

    #[test]
    fn non_empty_file_is_interesting() {
        let tmp = test_case::TempFile::anonymous().unwrap();
        {
            let mut tmp = fs::File::create(tmp.path()).unwrap();
            write!(&mut tmp, "z").unwrap();
        }
        let is_interesting = NonEmpty.is_interesting(tmp.path()).unwrap();
        assert!(is_interesting);
    }

    #[test]
    fn empty_file_is_not_interesting() {
        let tmp = temp_file();
        let is_interesting = NonEmpty.is_interesting(tmp.path()).unwrap();
        assert!(!is_interesting);
    }

    #[test]
    fn exit_zero_is_interesting() {
        let test = Script::new(get_exit_0()).unwrap();
        let test_case = temp_file();
        assert!(test.is_interesting(test_case.path()).unwrap());
    }

    #[test]
    fn exit_non_zero_is_not_interesting() {
        let test = Script::new(get_exit_1()).unwrap();
        let test_case = temp_file();
        assert!(!test.is_interesting(test_case.path()).unwrap());
    }

    #[test]
    fn and_both_true() {
        let test = And::new(
            Script::new(get_exit_0()).unwrap(),
            Script::new(get_exit_0()).unwrap(),
        );
        let test_case = temp_file();
        assert!(
            test.is_interesting(test_case.path())
                .expect("is interesting should return Ok")
        );
    }

    #[test]
    fn and_one_false() {
        let test = And::new(
            Script::new(get_exit_0()).unwrap(),
            Script::new(get_exit_1()).unwrap(),
        );
        let test_case = temp_file();
        assert!(!test.is_interesting(test_case.path()).unwrap());
    }

    #[test]
    fn or_first_true() {
        let test = Or::new(
            Script::new(get_exit_0()).unwrap(),
            Script::new(get_exit_1()).unwrap(),
        );
        let test_case = temp_file();
        assert!(test.is_interesting(test_case.path()).unwrap());
    }

    #[test]
    fn or_second_true() {
        let test = Or::new(
            Script::new(get_exit_1()).unwrap(),
            Script::new(get_exit_0()).unwrap(),
        );
        let test_case = temp_file();
        assert!(test.is_interesting(test_case.path()).unwrap());
    }

    #[test]
    fn or_both_false() {
        let test = Or::new(
            Script::new(get_exit_1()).unwrap(),
            Script::new(get_exit_1()).unwrap(),
        );
        let test_case = temp_file();
        assert!(!test.is_interesting(test_case.path()).unwrap());
    }

    #[test]
    fn func_returns_true() {
        let test = |_: &path::Path| Ok(true);
        let test = &test;
        let test_case = temp_file();
        assert!(test.is_interesting(test_case.path()).unwrap());
    }

    #[test]
    fn func_returns_false() {
        let test = |_: &path::Path| Ok(false);
        let test = &test;
        let test_case = temp_file();
        assert!(!test.is_interesting(test_case.path()).unwrap());
    }
}
