//! The queue for reductions that haven't been tested yet.

use actors::ReducerId;
use score;
use std::cmp;
use std::collections::BinaryHeap;
use std::mem;
use std::ops;
use test_case;

#[derive(PartialEq, Eq)]
struct QueuedReduction(test_case::PotentialReduction, ReducerId, score::Score);

impl PartialOrd for QueuedReduction {
    fn partial_cmp(&self, rhs: &QueuedReduction) -> Option<cmp::Ordering> {
        self.2.partial_cmp(&rhs.2)
    }
}

impl Ord for QueuedReduction {
    fn cmp(&self, rhs: &QueuedReduction) -> cmp::Ordering {
        self.2.cmp(&rhs.2)
    }
}

/// The queue for reductions that haven't been tested for interesting-ness yet.
pub struct ReductionQueue {
    reductions: BinaryHeap<QueuedReduction>,
}

impl ReductionQueue {
    /// Construct a new queue with capacity for `n` reductions.
    pub fn with_capacity(n: usize) -> ReductionQueue {
        ReductionQueue {
            reductions: BinaryHeap::with_capacity(n),
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

    /// Clear the reduction queue, leaving it empty.
    pub fn clear(&mut self) {
        self.reductions.clear();
    }

    /// Insert a new reduction into the queue, that was produced by the reducer
    /// actor with the given id.
    pub fn insert(
        &mut self,
        reduction: test_case::PotentialReduction,
        by: ReducerId,
        priority: score::Score,
    ) {
        self.reductions
            .push(QueuedReduction(reduction, by, priority));
    }

    /// Retain only the queued reductions for which the predicate returns `true`
    /// and remove all other queued reductions.
    pub fn retain<F>(&mut self, predicate: F)
    where
        F: FnMut(&test_case::PotentialReduction, ReducerId) -> bool,
    {
        let mut predicate = predicate;
        let retained: BinaryHeap<_> = self.reductions
            .drain()
            .filter(|&QueuedReduction(ref reduction, by, _)| {
                predicate(reduction, by)
            })
            .collect();
        mem::replace(&mut self.reductions, retained);
    }

    /// Drain the next `..n` reductions from the front of the queue.
    pub fn drain<'a>(&'a mut self, range: ops::RangeTo<usize>) -> Drain<'a> {
        Drain {
            queue: self,
            n: range.end,
        }
    }
}

/// An iterator for the draining reductions from the front of the reductions
/// queue. See `ReductionQueue::drain`.
pub struct Drain<'a> {
    queue: &'a mut ReductionQueue,
    n: usize,
}

impl<'a> Iterator for Drain<'a> {
    type Item = (test_case::PotentialReduction, ReducerId);

    fn next(&mut self) -> Option<(test_case::PotentialReduction, ReducerId)> {
        if self.n == 0 {
            None
        } else {
            self.n -= 1;
            self.queue
                .reductions
                .pop()
                .map(|QueuedReduction(reduction, by, _)| (reduction, by))
        }
    }
}
