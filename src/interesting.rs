//! Implementations of the `IsInteresting` trait.

use error;
use std::fs;
use std::path;
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

#[cfg(test)]
mod tests {
    extern crate tempfile;

    use std::io::Write;
    use super::*;

    #[test]
    fn non_empty_file_is_interesting() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(&mut *tmp, "z").unwrap();
        let is_interesting = NonEmpty.is_interesting(tmp.path()).unwrap();
        assert_eq!(is_interesting, true);
    }

    #[test]
    fn empty_file_is_not_interesting() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let is_interesting = NonEmpty.is_interesting(tmp.path()).unwrap();
        assert_eq!(is_interesting, false);
    }
}
