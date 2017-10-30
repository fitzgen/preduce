extern crate preduce_reducer_script;
extern crate serde;
#[macro_use]
extern crate serde_derive;

use preduce_reducer_script::{run, Reducer};
use std::cmp;
use std::fs;
use std::io::{self, Read, Seek, Write};
use std::marker::PhantomData;
use std::ops::Range;
use std::path::PathBuf;

/// A trait for describing a set of byte offset ranges in the test case to try
/// removing.
///
/// After defining this trait for your type `MyRanges`, you can run a reducer
/// script that generates candidates with those ranges removed from
/// the seed test case with `run_ranges::<MyRanges>()`. The resulting reducer
/// script will try removing all of the given ranges at once, then half of the
/// ranges at a time, then each quarter at a time, eighth at a time, ..., and
/// finally removing each range one at a time.
///
/// ### Example
///
/// Finding the ranges of all the `//`-style comments in a file, and then
/// running a reducer that tries removing those ranges.
///
/// ```
/// use preduce_ranges_reducer::{run_ranges, RemoveRanges};
/// use std::fs;
/// use std::io::{self, BufRead};
/// use std::ops::Range;
/// use std::path::PathBuf;
///
/// struct Comments;
///
/// impl RemoveRanges for Comments {
///     fn remove_ranges(seed: PathBuf) -> io::Result<Vec<Range<u64>>> {
///         let mut ranges = vec![];
///
///         let file = fs::File::open(seed)?;
///         let mut file = io::BufReader::new(file);
///
///         let mut offset = 0u64;
///         let mut line = String::new();
///         while {
///             line.clear();
///             file.read_line(&mut line)? > 0
///         } {
///             if line.trim().starts_with("//") {
///                 ranges.push(offset..offset + line.len() as u64)
///             }
///             offset += line.len() as u64;
///         }
///
///         Ok(ranges)
///     }
/// }
///
/// fn main() {
/// #   #![allow(unreachable_code)]
/// #   return;
///     run_ranges::<Comments>()
/// }
/// ```
pub trait RemoveRanges {
    /// Generate a set of ranges to try removing from the given seed test case.
    ///
    /// For all ranges, `range.start < range.end` must hold.
    fn remove_ranges(seed: PathBuf) -> io::Result<Vec<Range<u64>>>;

    /// How should the ranges be sorted?
    ///
    /// By default, the ranges will be sorted by largest range, breaking ties
    /// with `range.start` such that we try removing from the end of the seed
    /// test case before removing from the beginning. We prefer large ranges
    /// because we want to remove the most we can from the test case, as quickly
    /// as possible. We remove from the back before the front on the assumption
    /// that it is less likely to mess with dependencies between functions
    /// defined in the test case (assuming it is a programming language source
    /// file).
    ///
    /// If you desire a different sorting behavior, override the definition of
    /// this method.
    fn sort_ranges_by(a: &Range<u64>, b: &Range<u64>) -> cmp::Ordering {
        let a_len = a.end - a.start;
        let b_len = b.end - b.start;
        let big = a_len.cmp(&b_len).reverse();
        let start = a.start.cmp(&b.start).reverse();
        big.then(start)
    }
}

/// A `preduce_reducer_script::Reducer` backed by a `RemoveRanges` implementation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct RemoveRangesReducer<R>
where
    R: RemoveRanges,
{
    remove_ranges: PhantomData<R>,
    ranges: Vec<Range<u64>>,
    chunk_size: usize,
    index: usize,
}

impl<R> RemoveRangesReducer<R>
where
    R: RemoveRanges,
{
    fn get_ranges_in_chunk(&self) -> &[Range<u64>] {
        let start = self.index;
        let end = self.index + self.chunk_size;
        &self.ranges[start..end]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OrdByStart(Range<u64>);

impl cmp::PartialOrd for OrdByStart {
    #[inline]
    fn partial_cmp(&self, rhs: &Self) -> Option<cmp::Ordering> {
        Some(cmp::Ord::cmp(self, rhs))
    }
}

impl cmp::Ord for OrdByStart {
    #[inline]
    fn cmp(&self, rhs: &Self) -> cmp::Ordering {
        self.0
            .start
            .cmp(&rhs.0.start)
            .then(self.0.end.cmp(&rhs.0.end))
    }
}

impl<R> Reducer for RemoveRangesReducer<R>
where
    R: RemoveRanges,
{
    type Error = io::Error;

    fn new(seed: PathBuf) -> io::Result<Self> {
        let mut ranges = R::remove_ranges(seed)?;
        assert!(
            ranges.iter().all(|r| r.start < r.end),
            "Empty and big..little ranges are not allowed"
        );

        ranges.sort_unstable_by(R::sort_ranges_by);
        ranges.dedup();

        let chunk_size = ranges.len();
        let index = 0;

        Ok(RemoveRangesReducer {
            remove_ranges: PhantomData,
            ranges,
            chunk_size,
            index,
        })
    }

    fn next(mut self, _seed: PathBuf) -> io::Result<Option<Self>> {
        assert!(self.chunk_size <= self.ranges.len());
        if self.chunk_size == 0 {
            return Ok(None);
        }

        self.index += 1;

        if self.index == self.ranges.len() - (self.chunk_size - 1) {
            if self.chunk_size == 1 {
                Ok(None)
            } else {
                self.chunk_size /= 2;
                self.index = 0;
                Ok(Some(self))
            }
        } else {
            Ok(Some(self))
        }
    }

    fn next_on_interesting(
        mut self,
        _old_seed: PathBuf,
        new_seed: PathBuf,
    ) -> io::Result<Option<Self>> {
        assert!(self.chunk_size <= self.ranges.len());

        if self.chunk_size == 0 {
            return Ok(None);
        }

        let start_removed = self.index;
        let end_removed = self.index + self.chunk_size;
        let (mut removed, mut ranges) = self.ranges
            .drain(..)
            .map(OrdByStart)
            .enumerate()
            .partition::<Vec<_>, _>(|&(i, _)| start_removed <= i && i < end_removed);

        // TODO: what follows is a pretty terrible algorithm. This would be
        // better with some sort of interval tree that could be used to find out
        // the delta for a particular offset. Then the algorithm could be `O(n *
        // log n)` instead of this terrible `O(n^2)` abomination. However, none
        // of the interval trees on crates.io are quite what we need and I don't
        // feel like writing one myself right now...

        let mut ranges: Vec<_> = ranges.drain(..).map(|(_, r)| r).collect();
        ranges.sort_unstable();
        if ranges.is_empty() {
            return Ok(None);
        }

        let new_seed_len = fs::metadata(new_seed)?.len();

        let mut removed: Vec<_> = removed.drain(..).map(|(_, r)| r).collect();
        removed.sort_unstable();
        assert!(!removed.is_empty());

        // It is important to merge ranges so that we don't double-count ranges'
        // intersections when counting the deltas.
        let mut removed = removed.into_iter();
        let first = removed.next().unwrap();
        let removed = removed.fold(vec![first], |mut rs, r| {
            let merged = {
                let last = rs.last_mut().unwrap();
                if r.0.start <= last.0.end {
                    last.0.end = r.0.end;
                    true
                } else {
                    false
                }
            };

            if !merged {
                rs.push(r);
            }

            rs
        });

        self.ranges = ranges
            .drain(..)
            .filter_map(|r| {
                let mut delta_start = 0;
                let mut delta_end = 0;

                for s in &removed {
                    // Range is past the end of the file.
                    if r.0.start >= new_seed_len || r.0.end >= new_seed_len {
                        return None;
                    }

                    if s.0.start >= r.0.end {
                        break;
                    }

                    let s_len = s.0.end - s.0.start;

                    if s.0.start < r.0.start {
                        delta_start += cmp::min(r.0.start - s.0.start, s_len);
                    }

                    if s.0.start < r.0.end {
                        delta_end += cmp::min(r.0.end - s.0.start, s_len);
                    }
                }

                let new_start = r.0.start - delta_start;
                let new_end = r.0.end - delta_end;
                assert!(
                    new_start <= new_end,
                    "new_start <= new_end; start = {}; end = {}; new_start = {}; new_end = {}; removed = {:?}",
                    r.0.start,
                    r.0.end,
                    new_start,
                    new_end,
                    &removed
                );

                if new_start < new_end && new_end <= new_seed_len {
                    Some(new_start..new_end)
                } else {
                    None
                }
            })
            .collect();

        if self.ranges.is_empty() {
            return Ok(None);
        }

        self.ranges.sort_unstable_by(R::sort_ranges_by);

        if self.chunk_size > self.ranges.len() {
            self.chunk_size = self.ranges.len();
        }

        if self.index >= self.ranges.len() - (self.chunk_size - 1) {
            self.index = 0;
        }

        Ok(Some(self))
    }

    fn reduce(self, seed: PathBuf, dest: PathBuf) -> io::Result<bool> {
        assert!(self.chunk_size <= self.ranges.len());
        if self.chunk_size == 0 {
            return Ok(false);
        }

        let mut ranges: Vec<_> = self.get_ranges_in_chunk().iter().cloned().collect();
        ranges.sort_unstable_by(|a, b| OrdByStart(a.clone()).cmp(&OrdByStart(b.clone())));

        let seed_len = if cfg!(debug_assertions) {
            fs::metadata(&seed)?.len()
        } else {
            0
        };

        let mut seed = fs::File::open(seed)?;
        let mut dest = fs::File::create(dest)?;

        const BUF_SIZE: usize = 1024 * 1024;
        let mut buf: Vec<u8> = vec![0; BUF_SIZE];

        let mut offset = 0;
        for r in ranges {
            debug_assert!(r.start < seed_len);
            debug_assert!(r.end <= seed_len);

            if offset < r.start {
                let to_write = r.start - offset;
                let mut to_write = to_write as usize;

                while to_write > BUF_SIZE {
                    seed.read_exact(&mut buf)?;
                    dest.write_all(&buf)?;
                    to_write -= BUF_SIZE;
                }

                seed.read_exact(&mut buf[..to_write])?;
                dest.write_all(&buf[..to_write])?;
            }

            if offset < r.end {
                seed.seek(io::SeekFrom::Start(r.end))?;
                offset = r.end;
            }
        }

        io::copy(&mut seed, &mut dest)?;
        Ok(true)
    }
}

/// Run a reducer script that removes ranges defined by `R`.
///
/// See `RemoveRanges` for details.
pub fn run_ranges<R: RemoveRanges>() -> ! {
    run::<RemoveRangesReducer<R>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::RemoveRangesReducer;
    use std::marker::PhantomData;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct TestRanges;

    impl RemoveRanges for TestRanges {
        fn remove_ranges(_: PathBuf) -> io::Result<Vec<Range<u64>>> {
            Ok(vec![0..10, 3..5, 3..5, 5..16, 7..11])
        }
    }

    #[test]
    fn remove_ranges_next() {
        let path = PathBuf::from("/dev/null");

        let mut reducer = RemoveRangesReducer::<TestRanges>::new(path.clone()).unwrap();
        assert_eq!(
            reducer,
            RemoveRangesReducer {
                remove_ranges: PhantomData,
                ranges: vec![5..16, 0..10, 7..11, 3..5],
                chunk_size: 4,
                index: 0,
            }
        );

        for i in 0..3 {
            reducer = reducer
                .next(path.clone())
                .expect("no error on next")
                .expect("is some on next");
            assert_eq!(
                reducer,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![5..16, 0..10, 7..11, 3..5],
                    chunk_size: 2,
                    index: i,
                }
            )
        }

        for i in 0..4 {
            reducer = reducer
                .next(path.clone())
                .expect("no error on next")
                .expect("is some on next");
            assert_eq!(
                reducer,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![5..16, 0..10, 7..11, 3..5],
                    chunk_size: 1,
                    index: i,
                }
            )
        }

        assert!(reducer.next(path.clone()).expect("next is OK").is_none());
    }

    #[test]
    fn remove_ranges_next_on_interesting() {
        let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../README.md"));

        let reducer = RemoveRangesReducer::<TestRanges>::new(path.clone()).unwrap();

        {
            //                      [--------- s --------)
            // [------- r ----------)
            let mut reducer = reducer.clone();
            reducer.ranges = vec![10..30, 0..10];
            reducer.chunk_size = 1;
            reducer.index = 0;

            let next = reducer
                .next_on_interesting(path.clone(), path.clone())
                .expect("next_on_interesting should be OK")
                .expect("next_on_interesting should be some");

            assert_eq!(
                next,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![0..10],
                    chunk_size: 1,
                    index: 0,
                }
            );
        }

        {
            // [------ s -----)
            //                [------- r -------)
            let mut reducer = reducer.clone();
            reducer.ranges = vec![0..10, 10..15];
            reducer.chunk_size = 1;
            reducer.index = 0;

            let next = reducer
                .next_on_interesting(path.clone(), path.clone())
                .expect("next_on_interesting should be OK")
                .expect("next_on_interesting should be some");

            assert_eq!(
                next,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![0..5],
                    chunk_size: 1,
                    index: 0,
                }
            );
        }

        //      [------ s -----)
        // [----------- r -----------)
        {
            let mut reducer = reducer.clone();
            reducer.ranges = vec![5..10, 0..15];
            reducer.chunk_size = 1;
            reducer.index = 0;

            let next = reducer
                .next_on_interesting(path.clone(), path.clone())
                .expect("next_on_interesting should be OK")
                .expect("next_on_interesting should be some");

            assert_eq!(
                next,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![0..10],
                    chunk_size: 1,
                    index: 0,
                }
            );
        }

        {
            // [----------- s -----------)
            //      [------ r -----)
            let mut reducer = reducer.clone();
            reducer.ranges = vec![0..10, 5..7];
            reducer.chunk_size = 1;
            reducer.index = 0;

            assert!(
                reducer
                    .next_on_interesting(path.clone(), path.clone())
                    .expect("next_on_interesting should be OK")
                    .is_none()
            );
        }

        {
            // [--------- s ---------)
            //                 [---------- r ---------)
            let mut reducer = reducer.clone();
            reducer.ranges = vec![0..10, 8..12];
            reducer.chunk_size = 1;
            reducer.index = 0;

            let next = reducer
                .next_on_interesting(path.clone(), path.clone())
                .expect("next_on_interesting should be OK")
                .expect("next_on_interesting should be some");

            assert_eq!(
                next,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![0..2],
                    chunk_size: 1,
                    index: 0,
                }
            );
        }

        {
            //                 [--------- s ---------)
            // [---------- r ---------)
            let mut reducer = reducer.clone();
            reducer.ranges = vec![8..15, 5..10];
            reducer.chunk_size = 1;
            reducer.index = 0;

            let next = reducer
                .next_on_interesting(path.clone(), path.clone())
                .expect("next_on_interesting should be OK")
                .expect("next_on_interesting should be some");

            assert_eq!(
                next,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![5..8],
                    chunk_size: 1,
                    index: 0,
                }
            );
        }

        {
            // Removing multiple ranges from the middle of the set.
            let mut reducer = reducer.clone();
            reducer.ranges = vec![30..41, 20..30, 10..20, 5..10, 0..3, 3..5];
            // Removing these two:                ~~~~~~  ~~~~~
            reducer.chunk_size = 2;
            reducer.index = 2;

            let next = reducer
                .next_on_interesting(path.clone(), path.clone())
                .expect("next_on_interesting should be OK")
                .expect("next_on_interesting should be some");

            assert_eq!(
                next,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![15..26, 5..15, 0..3, 3..5],
                    chunk_size: 2,
                    index: 2,
                }
            );
        }

        {
            // [---- s ----)
            //             [----- s' -----)
            // [------------ r -----------)
            let mut reducer = reducer.clone();
            reducer.ranges = vec![100..200, 0..20, 10..20, 0..10];
            reducer.chunk_size = 2;
            reducer.index = 2;

            let next = reducer
                .next_on_interesting(path.clone(), path.clone())
                .expect("next_on_interesting should be OK")
                .expect("next_on_interesting should be some");

            assert_eq!(
                next,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![80..180],
                    chunk_size: 1,
                    index: 0,
                }
            );
        }

        {
            // Ranges beyond the new seed's length are dropped.
            let mut reducer = reducer.clone();
            reducer.ranges = vec![10..20, 0..10, 8_888_888_888..9_999_999_999];
            reducer.chunk_size = 1;
            reducer.index = 0;

            let next = reducer
                .next_on_interesting(path.clone(), path.clone())
                .expect("next_on_interesting should be OK")
                .expect("next_on_interesting should be some");

            assert_eq!(
                next,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![0..10],
                    chunk_size: 1,
                    index: 0,
                }
            );
        }

        {
            // [---- s ----)
            //          [---- s' ----)
            //                   [----- r -----)
            let mut reducer = reducer.clone();
            reducer.ranges = vec![0..10, 5..20, 15..40];
            reducer.chunk_size = 2;
            reducer.index = 0;

            let next = reducer
                .next_on_interesting(path.clone(), path.clone())
                .expect("next_on_interesting should be OK")
                .expect("next_on_interesting should be some");

            assert_eq!(
                next,
                RemoveRangesReducer {
                    remove_ranges: PhantomData,
                    ranges: vec![0..20],
                    chunk_size: 1,
                    index: 0,
                }
            );
        }
    }
}
