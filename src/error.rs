//! Custom errors and results.

use git2;
use std::error;
use std::fmt;
use std::io;

/// The kinds of errors that can happen when running `preduce`.
#[derive(Debug)]
pub enum Error {
    /// A git error.
    Git(git2::Error),

    /// An IO error.
    Io(io::Error),

    /// An error related to a misbehaving reducer script.
    MisbehavingReducerScript(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> ::std::result::Result<(), fmt::Error> {
        match *self {
            Error::Git(ref e) => fmt::Display::fmt(e, f),
            Error::Io(ref e) => fmt::Display::fmt(e, f),
            Error::MisbehavingReducerScript(ref details) => {
                write!(f, "Misbehaving reducer script: {}", details)
            }
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Git(ref e) => error::Error::description(e),
            Error::Io(ref e) => error::Error::description(e),
            Error::MisbehavingReducerScript(_) => "Misbehaving reducer script",
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

/// A `Result` whose `Err` variant is `preduce::error::Error`.
pub type Result<T> = ::std::result::Result<T, Error>;
