//! Custom errors and results.

use serde_json;
use std::any::Any;
use std::error;
use std::fmt;
use std::io;
use std::path;

/// The kinds of errors that can happen when running `preduce`.
#[derive(Debug)]
pub enum Error {
    /// An IO error.
    Io(io::Error),

    /// A JSON encoding/decoding error.
    Json(serde_json::Error),

    /// A panicked thread's failure value.
    Thread(Box<Any + Send + 'static>),

    /// An error related to a misbehaving reducer script.
    MisbehavingReducerScript(String),

    /// An error that occurred when attempting to backup the original test case.
    TestCaseBackupFailure(io::Error),

    /// The initial test case did not pass the is-interesting predicate.
    InitialTestCaseNotInteresting,

    /// An "is interesting?" predicate script was not executable.
    IsNotExecutable(path::PathBuf),

    /// There is no file at the given path, when we expected one.
    DoesNotExist(path::PathBuf),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> ::std::result::Result<(), fmt::Error> {
        match *self {
            Error::Io(ref e) => fmt::Display::fmt(e, f),
            Error::Json(ref e) => fmt::Display::fmt(e, f),
            Error::Thread(ref e) => write!(f, "Thread panicked: {:?}", e),
            Error::MisbehavingReducerScript(ref details) => {
                write!(f, "Misbehaving reducer script: {}", details)
            }
            Error::TestCaseBackupFailure(ref e) => {
                write!(f, "Could not backup initial test case: {}", e)
            }
            Error::InitialTestCaseNotInteresting => write!(
                f,
                "The initial test case did not pass the is-interesting predicate"
            ),
            Error::IsNotExecutable(ref file_path) => {
                write!(f, "The script is not executable: {}", file_path.display())
            }
            Error::DoesNotExist(ref file_path) => {
                write!(f, "The file does not exist: {}", file_path.display())
            }
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Io(ref e) => error::Error::description(e),
            Error::Json(ref e) => error::Error::description(e),
            Error::Thread(_) => "A panicked thread",
            Error::MisbehavingReducerScript(_) => "Misbehaving reducer script",
            Error::TestCaseBackupFailure(_) => "Could not backup initial test case",
            Error::InitialTestCaseNotInteresting => {
                "The initial test case did not pass the is-interesting predicate"
            }
            Error::IsNotExecutable(_) => {
                "The script is not executable"
            }
            Error::DoesNotExist(_) => "There is no file at the given path, but we expected one",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Io(ref e) => Some(e),
            Error::Json(ref e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

impl From<Box<Any + Send + 'static>> for Error {
    fn from(e: Box<Any + Send + 'static>) -> Self {
        Error::Thread(e)
    }
}

/// A `Result` whose `Err` variant is `preduce::error::Error`.
pub type Result<T> = ::std::result::Result<T, Error>;
