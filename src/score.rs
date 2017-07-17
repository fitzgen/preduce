//! Types for potential reductions' priority.

use std::cmp;

/// A score of a reduction's potential. Higher is better: more likely to be
/// judged interesting and a bigger reduction.
#[derive(Copy, Clone, Debug, PartialOrd, PartialEq)]
pub struct Score(f64);

impl Score {
    /// Construct a new `Score`.
    ///
    /// ### Panics
    ///
    /// Panics if `s` is NaN or infinite.
    pub fn new(s: f64) -> Score {
        assert!(!s.is_nan());
        assert!(!s.is_infinite());
        Score(s)
    }
}

impl Eq for Score {}

impl Ord for Score {
    fn cmp(&self, rhs: &Score) -> cmp::Ordering {
        if self.0 < rhs.0 {
            cmp::Ordering::Less
        } else if self.0 > rhs.0 {
            cmp::Ordering::Greater
        } else {
            cmp::Ordering::Equal
        }
    }
}
