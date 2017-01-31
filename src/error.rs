//! Custom errors and results.

use std::error;
use std::fmt;
use std::io;

/// The kinds of errors that can happen when running `preduce`.
#[derive(Debug)]
pub enum Error {
    /// An IO error.
    Io(io::Error),

    /// TODO FITZGEN
    MisbehavingReducerScript(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> ::std::result::Result<(), fmt::Error> {
        match *self {
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

/// A `Result` whose `Err` variant is `preduce::error::Error`.
pub type Result<T> = ::std::result::Result<T, Error>;
