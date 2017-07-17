//! Determining the priority of potential reductions.

use score::Score;
use std::collections::HashMap;
use test_case::{self, TestCaseMethods};
use traits;

#[derive(Debug, Default)]
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

/// An `Oracle` implementation that scores test cases based on their reduction's
/// historical rate of producing interesting test cases.
#[derive(Debug, Default)]
pub struct InterestingRate {
    observations: HashMap<String, Observed>,
}

impl traits::Oracle for InterestingRate {
    fn observe_smallest_interesting(&mut self, interesting: &test_case::Interesting) {
        self.observations
            .entry(interesting.provenance().into())
            .or_insert_with(Default::default)
            .smallest_interesting_count += 1;
    }

    fn observe_not_smallest_interesting(&mut self, interesting: &test_case::Interesting) {
        self.observations
            .entry(interesting.provenance().into())
            .or_insert_with(Default::default)
            .not_smallest_interesting_count += 1;
    }

    fn observe_not_interesting(&mut self, reduction: &test_case::PotentialReduction) {
        self.observations
            .entry(reduction.provenance().into())
            .or_insert_with(Default::default)
            .not_interesting_count += 1;
    }

    fn predict(&mut self, reduction: &test_case::PotentialReduction) -> Score {
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
