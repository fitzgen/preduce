//! Custom errors and results.

use git2;
use std::any::Any;
use std::error;
use std::fmt;
use std::io;
use std::path;

/// The kinds of errors that can happen when running `preduce`.
#[derive(Debug)]
pub enum Error {
    /// A git error.
    Git(git2::Error),

    /// An IO error.
    Io(io::Error),

    /// A panicked thread's failure value.
    Thread(Box<Any + Send + 'static>),

    /// An error related to a misbehaving reducer script.
    MisbehavingReducerScript(String),

    /// An error that occurred when attempting to backup the original test case.
    TestCaseBackupFailure(io::Error),

    /// The initial test case did not pass the is-interesting predicate.
    InitialTestCaseNotInteresting,

    /// There is no file at the given path, when we expected one.
    DoesNotExist(path::PathBuf),

    /// Running `git gc` on a repository failed.
    GitGcFailed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> ::std::result::Result<(), fmt::Error> {
        match *self {
            Error::Git(ref e) => write!(f, "git: {}", e),
            Error::Io(ref e) => fmt::Display::fmt(e, f),
            Error::Thread(ref e) => write!(f, "Thread panicked: {:?}", e),
            Error::MisbehavingReducerScript(ref details) => {
                write!(f, "Misbehaving reducer script: {}", details)
            }
            Error::TestCaseBackupFailure(ref e) => {
                write!(f, "Could not backup initial test case: {}", e)
            }
            Error::InitialTestCaseNotInteresting => {
                write!(
                    f,
                    "The initial test case did not pass the is-interesting predicate"
                )
            }
            Error::DoesNotExist(ref file_path) => {
                write!(f, "The file does not exist: {}", file_path.display())
            }
            Error::GitGcFailed => write!(f, "Running `git gc` on a repository failed"),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Git(ref e) => error::Error::description(e),
            Error::Io(ref e) => error::Error::description(e),
            Error::Thread(_) => "A panicked thread",
            Error::MisbehavingReducerScript(_) => "Misbehaving reducer script",
            Error::TestCaseBackupFailure(_) => "Could not backup initial test case",
            Error::InitialTestCaseNotInteresting => {
                "The initial test case did not pass the is-interesting predicate"
            }
            Error::DoesNotExist(_) => "There is no file at the given path, but we expected one",
            Error::GitGcFailed => "Running `git gc` on a repository failed",
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<git2::Error> for Error {
    fn from(e: git2::Error) -> Self {
        Error::Git(e)
    }
}

impl From<Box<Any + Send + 'static>> for Error {
    fn from(e: Box<Any + Send + 'static>) -> Self {
        Error::Thread(e)
    }
}

/// A `Result` whose `Err` variant is `preduce::error::Error`.
pub type Result<T> = ::std::result::Result<T, Error>;
