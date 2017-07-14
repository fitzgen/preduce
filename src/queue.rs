//! The queue for reductions that haven't been tested yet.

use test_case;
use std::collections::{vec_deque, VecDeque};
use std::ops;
use actors::ReducerId;

/// The queue for reductions that haven't been tested for interesting-ness yet.
pub struct ReductionQueue {
    reductions: VecDeque<(test_case::PotentialReduction, ReducerId)>,
}

impl ReductionQueue {
    /// Construct a new queue with capacity for `n` reductions.
    pub fn with_capacity(n: usize) -> ReductionQueue {
        ReductionQueue {
            reductions: VecDeque::with_capacity(n)
        }
    }

    /// Is the queue empty?
    pub fn is_empty(&self) -> bool {
        self.reductions.is_empty()
    }

    /// Get the number of reductions that are queued.
    pub fn len(&self) -> usize {
        self.reductions.len()
    }

    /// Insert a new reduction into the queue, that was produced by the reducer
    /// actor with the given id.
    pub fn insert(&mut self, reduction: test_case::PotentialReduction, by: ReducerId) {
        self.reductions.push_back((reduction, by));
    }

    /// Retain only the queued reductions for which the predicate returns `true`
    /// and remove all other queued reductions.
    pub fn retain<F>(&mut self, predicate: F)
        where F: FnMut(&test_case::PotentialReduction, ReducerId) -> bool
    {
        let mut predicate = predicate;
        self.reductions.retain(|&(ref reduction, id)| predicate(reduction, id));
    }

    /// Drain the next `..n` reductions from the front of the queue.
    pub fn drain<'a>(&'a mut self, range: ops::RangeTo<usize>) -> Drain<'a> {
        Drain {
            inner: self.reductions.drain(range)
        }
    }
}

/// An iterator for the draining reductions from the front of the reductions
/// queue. See `ReductionQueue::drain`.
pub struct Drain<'a> {
    inner: vec_deque::Drain<'a, (test_case::PotentialReduction, ReducerId)>,
}

impl<'a> Iterator for Drain<'a> {
    type Item = (test_case::PotentialReduction, ReducerId);

    fn next(&mut self) -> Option<(test_case::PotentialReduction, ReducerId)> {
        self.inner.next()
    }
}
