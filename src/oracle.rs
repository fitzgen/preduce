//! Determining the priority of potential reductions.

use std::collections::HashMap;
use test_case::{self, TestCaseMethods};

#[derive(Default)]
struct Observed {
    smallest_interesting_count: u32,
    not_smallest_interesting_count: u32,
    not_interesting_count: u32,
}

impl Observed {
    fn total(&self) -> u32 {
        self.smallest_interesting_count + self.not_smallest_interesting_count +
            self.not_interesting_count
    }

    fn interesting(&self) -> u32 {
        self.smallest_interesting_count + self.not_smallest_interesting_count
    }
}

/// The oracle observes the results of interesting-ness judgements of reductions
/// and then predicts the interesting-ness of future reductions by scoring
/// them. The resulting scores are ultimately used by the supervisor actor to
/// prioritize and schedule work.
#[derive(Default)]
pub struct ReductionOracle {
    observations: HashMap<String, Observed>,
}

impl ReductionOracle {
    /// Tell the oracle that we found a new smallest interesting test case.
    pub fn observe_smallest_interesting(&mut self, interesting: &test_case::Interesting) {
        self.observations
            .entry(interesting.provenance().into())
            .or_insert_with(Default::default)
            .smallest_interesting_count += 1;
    }

    /// Tell the oracle that we found a new interesting test case, but that it
    /// is not the smallest.
    pub fn observe_not_smallest_interesting(&mut self, interesting: &test_case::Interesting) {
        self.observations
            .entry(interesting.provenance().into())
            .or_insert_with(Default::default)
            .not_smallest_interesting_count += 1;
    }

    /// Tell the oracle that we found the given reduction unininteresting.
    pub fn observe_not_interesting(&mut self, reduction: &test_case::PotentialReduction) {
        self.observations
            .entry(reduction.provenance().into())
            .or_insert_with(Default::default)
            .not_interesting_count += 1;
    }

    /// Ask the oracle's to score the given potential reduction, so we know how
    /// to prioritize testing it.
    pub fn predict(&mut self, reduction: &test_case::PotentialReduction) -> Score {
        let observed = self.observations
            .entry(reduction.provenance().into())
            .or_insert_with(Default::default);
        let total = observed.total();
        if total == 0 {
            Score::new(0.0)
        } else {
            Score::new(observed.interesting() as f64 / total as f64)
        }
    }
}

pub use self::score::Score;
mod score {
    use std::cmp;

    /// A score of a reduction's potential. Higher is better: more likely to be
    /// judged interesting and a bigger reduction.
    #[derive(Copy, Clone, PartialOrd, PartialEq)]
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
}
