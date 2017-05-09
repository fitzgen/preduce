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
    /// A chained diagnostic message and underlying error.
    Chained(&'static str, Box<Error>),

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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> ::std::result::Result<(), fmt::Error> {
        match *self {
            Error::Chained(msg, ref e) => {
                write!(f, "{}: {}", msg, e)
            }
            Error::Git(ref e) => fmt::Display::fmt(e, f),
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
        }
    }
}

impl error::Error for Error {
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Chained(_, ref e) => Some(e),
            Error::Git(ref e) => Some(e),
            Error::Io(ref e) => Some(e),

            Error::Thread(_) |
            Error::MisbehavingReducerScript(_) |
            Error::TestCaseBackupFailure(_) |
            Error::InitialTestCaseNotInteresting |
            Error::DoesNotExist(_) => None
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Chained(msg, _) => msg,
            Error::Git(ref e) => error::Error::description(e),
            Error::Io(ref e) => error::Error::description(e),
            Error::Thread(_) => "A panicked thread",
            Error::MisbehavingReducerScript(_) => "Misbehaving reducer script",
            Error::TestCaseBackupFailure(_) => "Could not backup initial test case",
            Error::InitialTestCaseNotInteresting => "The initial test case did not pass the is-interesting predicate",
            Error::DoesNotExist(_) => "There is no file at the given path, but we expected one",
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

/// Allow chaining custom diagnostic messages onto errors.
pub trait ChainErr<T> {
    /// Chain the given message onto this error.
    fn chain_err(self, msg: &'static str) -> Result<T>;
}

impl<T, E> ChainErr<T> for ::std::result::Result<T, E>
    where E: Into<Error>
{
    #[inline]
    fn chain_err(self, msg: &'static str) -> Result<T> {
        self.map_err(|e| {
            let e = e.into();
            Error::Chained(msg, Box::new(e))
        })
    }
}

macro_rules! chained {
    (
        $msg:expr ,
        fn $name:ident ( $( $args:tt )* ) -> $ret:ty {
            $( $body:tt )*
        }
    ) => {
        fn $name ( $( $args )* ) -> $ret {
            use error::{self, ChainErr};

            // Required to coerce the closure into `FnOnce` instead of `Fn`.
            fn once<T, F: FnOnce() -> T>(f: F) -> T {
                f()
            }

            let result: error::Result<_> = once(move || {
                $( $body )*
            });

            result.chain_err($msg)
        }
    }
}

/// A `Result` whose `Err` variant is `preduce::error::Error`.
pub type Result<T> = ::std::result::Result<T, Error>;
