//! The queue for candidates that haven't been tested yet.

use actors::ReducerId;
use score;
use std::cmp;
use std::collections::BinaryHeap;
use std::mem;
use std::ops;
use test_case;

#[derive(PartialEq, Eq)]
struct QueuedCandidate(test_case::Candidate, ReducerId, score::Score);

impl PartialOrd for QueuedCandidate {
    fn partial_cmp(&self, rhs: &QueuedCandidate) -> Option<cmp::Ordering> {
        self.2.partial_cmp(&rhs.2)
    }
}

impl Ord for QueuedCandidate {
    fn cmp(&self, rhs: &QueuedCandidate) -> cmp::Ordering {
        self.2.cmp(&rhs.2)
    }
}

/// The queue for candidates that haven't been tested for interesting-ness yet.
pub struct CandidateQueue {
    candidates: BinaryHeap<QueuedCandidate>,
}

impl CandidateQueue {
    /// Construct a new queue with capacity for `n` candidates.
    pub fn with_capacity(n: usize) -> CandidateQueue {
        CandidateQueue {
            candidates: BinaryHeap::with_capacity(n),
        }
    }

    /// Is the queue empty?
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    /// Get the number of candidates that are queued.
    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    /// Clear the candidate queue, leaving it empty.
    pub fn clear(&mut self) {
        self.candidates.clear();
    }

    /// Insert a new candidate into the queue, that was produced by the reducer
    /// actor with the given id.
    pub fn insert(
        &mut self,
        candidate: test_case::Candidate,
        by: ReducerId,
        priority: score::Score,
    ) {
        self.candidates
            .push(QueuedCandidate(candidate, by, priority));
    }

    /// Retain only the queued candidates for which the predicate returns `true`
    /// and remove all other queued candidates.
    pub fn retain<F>(&mut self, predicate: F)
    where
        F: FnMut(&test_case::Candidate, ReducerId) -> bool,
    {
        let mut predicate = predicate;
        let retained: BinaryHeap<_> = self.candidates
            .drain()
            .filter(|&QueuedCandidate(ref candidate, by, _)| {
                predicate(candidate, by)
            })
            .collect();
        mem::replace(&mut self.candidates, retained);
    }

    /// Drain the next `..n` candidates from the front of the queue.
    pub fn drain<'a>(&'a mut self, range: ops::RangeTo<usize>) -> Drain<'a> {
        Drain {
            queue: self,
            n: range.end,
        }
    }
}

/// An iterator for the draining candidates from the front of the candidates
/// queue. See `CandidateQueue::drain`.
pub struct Drain<'a> {
    queue: &'a mut CandidateQueue,
    n: usize,
}

impl<'a> Iterator for Drain<'a> {
    type Item = (test_case::Candidate, ReducerId);

    fn next(&mut self) -> Option<(test_case::Candidate, ReducerId)> {
        if self.n == 0 {
            None
        } else {
            self.n -= 1;
            self.queue
                .candidates
                .pop()
                .map(|QueuedCandidate(candidate, by, _)| (candidate, by))
        }
    }
}
