//! A generic and parallel test case reducer.

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod error;
pub mod interesting;
pub mod reducers;
pub mod traits;

#[cfg(test)]
mod test_utils;
