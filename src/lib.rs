//! A generic and parallel test case reducer.

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

extern crate tempdir;
extern crate git2;

pub mod error;
mod git;
pub mod interesting;
pub mod reducers;
pub mod test_case;
pub mod traits;

#[cfg(test)]
mod test_utils;
