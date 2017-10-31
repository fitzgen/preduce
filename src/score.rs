//! Types for candidates' priority.

use std::cmp;
use std::ops;

/// A score of a candidate's potential.
#[derive(Copy, Clone, Debug, PartialOrd, PartialEq)]
pub enum Score {
    /// We should try testing this candidate.
    ///
    /// Higher is better: more likely to be judged interesting and a bigger
    /// candidate.
    TryIt(f64),

    /// This candidate isn't even worth testing.
    SkipIt,
}

use self::Score::*;

impl Score {
    /// Construct a new `Score`.
    ///
    /// ### Panics
    ///
    /// Panics if `s` is NaN or infinite.
    pub fn new(s: f64) -> Score {
        assert!(!s.is_nan());
        assert!(!s.is_infinite());
        TryIt(s)
    }

    /// Construct a new `Score` that will result in the candidate being ignored.
    pub fn skip() -> Score {
        SkipIt
    }
}

impl Eq for Score {}

impl Ord for Score {
    fn cmp(&self, rhs: &Score) -> cmp::Ordering {
        match (*self, *rhs) {
            (x, y) if x == y => cmp::Ordering::Equal,
            (SkipIt, _) => cmp::Ordering::Less,
            (_, SkipIt) => cmp::Ordering::Greater,
            (TryIt(x), TryIt(y)) => {
                assert!(!x.is_nan());
                assert!(!x.is_infinite());
                assert!(!y.is_nan());
                assert!(!y.is_infinite());
                assert!(x != y);
                if x < y {
                    cmp::Ordering::Less
                } else {
                    cmp::Ordering::Greater
                }
            }
        }
    }
}

impl ops::Add for Score {
    type Output = Score;

    fn add(self, rhs: Score) -> Score {
        match (self, rhs) {
            (SkipIt, _) | (_, SkipIt) => SkipIt,
            (TryIt(x), TryIt(y)) => TryIt(x + y),
        }
    }
}
