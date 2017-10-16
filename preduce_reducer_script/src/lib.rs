/*!

Helpers and utilities for writing `preduce` reducer scripts in Rust.

To write a reducer script in Rust, implement the `Reducer` trait and then call
`run::<MyReducer>()` in `main`.

### Example

This is a reducer script that tries removing a single line from the seed test
case. It starts by removing the first line, then the second line, etc...

```
# #![allow(dead_code, unused_variables)]
extern crate preduce_reducer_script;
extern crate serde;
#[macro_use]
extern crate serde_derive;

use preduce_reducer_script::{Reducer, run};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

/// A reducer that removes single lines from seed test cases.
#[derive(Deserialize, Serialize)]
struct Lines {
    // The line we are currently trying to remove.
    current_line: u64,

    // The number of lines in the seed test case.
    num_lines: u64,
}

impl Reducer for Lines {
    type Error = io::Error;

    fn new(seed: PathBuf) -> io::Result<Self> {
        let current_line = 0;
        let num_lines = preduce_reducer_script::count_lines(&seed)?;
        Ok(Lines {
            current_line,
            num_lines,
        })
    }

    fn next(mut self, _seed: PathBuf) -> io::Result<Option<Self>> {
        self.current_line += 1;
        if self.current_line < self.num_lines {
            Ok(Some(self))
        } else {
            Ok(None)
        }
    }

    fn next_on_interesting(
        mut self,
        _old_seed: PathBuf,
        _new_seed: PathBuf
    ) -> io::Result<Option<Self>> {
        // We removed the `current_line`^th line from `old_seed`, which produced
        // `new_seed`. Therefore, `new_seed` is one line shorter than
        // `old_seed`, so we should decrement `num_lines` by one, and its
        // `current_line`^th line is `old_seed`'s `current_line + 1`^th line, so
        // we don't need to modify `current_line`.
        self.num_lines -= 1;
        if self.current_line < self.num_lines {
            Ok(Some(self))
        } else {
            Ok(None)
        }
    }

    fn fast_forward(mut self, _seed: PathBuf, n: usize) -> io::Result<Option<Self>> {
        // We can implement `fast_forward` more efficiently than by calling
        // `self.next()` in a loop `n` times!
        self.current_line += n as u64;
        if self.current_line < self.num_lines {
            Ok(Some(self))
        } else {
            Ok(None)
        }
    }

    fn reduce(self, seed: PathBuf, dest: PathBuf) -> io::Result<bool> {
        if self.current_line >= self.num_lines {
            return Ok(false);
        }

        let seed = fs::File::open(seed)?;
        let mut seed = io::BufReader::new(seed);

        let dest = fs::File::create(dest)?;
        let mut dest = io::BufWriter::new(dest);

        let mut line = String::new();

        // Copy the first `current_line - 1` lines to `dest`.
        for _ in 0..self.current_line {
            line.clear();
            seed.read_line(&mut line)?;
            dest.write_all(line.as_bytes())?;
        }

        // Read the `current_line`^th line, but don't copy it into `dest`.
        line.clear();
        seed.read_line(&mut line)?;

        // Copy the rest of the lines in `seed` into `dest`.
        io::copy(&mut seed, &mut dest)?;
        Ok(true)
    }
}

// Finally, call `run` in `main`. That's it!
fn main() {
#   #![allow(dead_code)]
#   return;
    run::<Lines>()
}
```

*/

#![deny(missing_docs)]

extern crate is_executable;
#[macro_use]
extern crate lazy_static;
extern crate preduce_ipc_types;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use is_executable::IsExecutable;
use preduce_ipc_types::{FastForwardRequest, NewRequest, NextOnInterestingRequest, NextRequest,
                        ReduceRequest, Request};
use preduce_ipc_types::{FastForwardResponse, NewResponse, NextOnInterestingResponse, NextResponse,
                        ReduceResponse, Response};
use serde::{Deserialize, Serialize};
use std::cmp;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{self, BufRead, Read, Seek, Write};
use std::iter::FromIterator;
use std::marker::PhantomData;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process;

/// A trait for defining a reducer script.
///
/// To write a reducer script in Rust, implement this trait, and then call
/// `run::<MyReducer>()` in `main`.
///
/// Note that `reduce` is not guaranteed to be called after every call to
/// `next`. See the documentation for the `fast_forward` method below for
/// details.
pub trait Reducer: for<'de> Deserialize<'de> + Serialize {
    /// The type of error that these methods might fail with.
    type Error: fmt::Display;

    /// Construct a new reducer for the given seed test case.
    fn new(seed: PathBuf) -> Result<Self, Self::Error>;

    /// Advance to the next reduction state, assuming that the potential
    /// reduction generated from the current `self` was not interesting.
    fn next(self, seed: PathBuf) -> Result<Option<Self>, Self::Error>;

    /// Advance to the next reduction state, given that the reduction generated
    /// from the current `self` was judged to be interesting.
    fn next_on_interesting(
        self,
        old_seed: PathBuf,
        new_seed: PathBuf,
    ) -> Result<Option<Self>, Self::Error>;

    /// Skip over the next `n` reduction states.
    ///
    /// By default, this is implemented by calling `self.next()` in a loop `n`
    /// times. This default implementation is `O(n)`, so if your reducer can
    /// fast forward more efficiently than that, you should specialize this
    /// trait.
    fn fast_forward(self, seed: PathBuf, n: usize) -> Result<Option<Self>, Self::Error> {
        let mut s = self;
        for _ in 0..n {
            s = match s.next(seed.clone())? {
                None => return Ok(None),
                Some(s) => s,
            }
        }
        Ok(Some(s))
    }

    /// Generate a reduction into a file at the destination path `dest`.
    ///
    /// Although it is not a reducer's responsibility to name paths, it *is* the
    /// reducer's responsibility to create the file at the given `dest` path.
    ///
    /// If the reduction state in `self` can't be used to generate a reduction
    /// for whatever reason, maybe it is easier to check in `reduce` than `next`
    /// for your reducer, then return `Ok(false)`. Upon successfully generating
    /// a reduction, return `Ok(true)`.
    fn reduce(self, seed: PathBuf, dest: PathBuf) -> Result<bool, Self::Error>;
}

trait InfallibleReducer: Reducer {
    fn infallible_new(seed: PathBuf) -> Self {
        match Self::new(seed) {
            Ok(s) => s,
            Err(e) => {
                let stderr = io::stderr();
                let mut stderr = stderr.lock();
                let _ = writeln!(&mut stderr, "Reducer script error: {}", e);
                process::exit(1);
            }
        }
    }

    fn infallible_next(self, seed: PathBuf) -> Option<Self> {
        match self.next(seed) {
            Ok(n) => n,
            Err(e) => {
                let stderr = io::stderr();
                let mut stderr = stderr.lock();
                let _ = writeln!(&mut stderr, "Reducer script error: {}", e);
                process::exit(1);
            }
        }
    }

    fn infallible_next_on_interesting(self, old_seed: PathBuf, new_seed: PathBuf) -> Option<Self> {
        match self.next_on_interesting(old_seed, new_seed) {
            Ok(n) => n,
            Err(e) => {
                let stderr = io::stderr();
                let mut stderr = stderr.lock();
                let _ = writeln!(&mut stderr, "Reducer script error: {}", e);
                process::exit(1);
            }
        }
    }

    fn infallible_fast_forward(self, seed: PathBuf, n: usize) -> Option<Self> {
        match self.fast_forward(seed, n) {
            Ok(n) => n,
            Err(e) => {
                let stderr = io::stderr();
                let mut stderr = stderr.lock();
                let _ = writeln!(&mut stderr, "Reducer script error: {}", e);
                process::exit(1);
            }
        }
    }

    fn infallible_reduce(self, seed: PathBuf, dest: PathBuf) -> bool {
        match self.reduce(seed, dest) {
            Ok(b) => b,
            Err(e) => {
                let stderr = io::stderr();
                let mut stderr = stderr.lock();
                let _ = writeln!(&mut stderr, "Reducer script error: {}", e);
                false
            }
        }
    }
}

impl<T: Reducer> InfallibleReducer for T {}

/// A trait for describing a set of byte offset ranges in the test case to try
/// removing.
///
/// After defining this trait for your type `MyRanges`, you can run a reducer
/// script that generates potential reductions with those ranges removed from
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
/// use preduce_reducer_script::{run_ranges, RemoveRanges};
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

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct RemoveRangesReducer<R>
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
        assert!(self.chunk_size > 0);
        assert!(self.chunk_size <= self.ranges.len());

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
        _new_seed: PathBuf,
    ) -> io::Result<Option<Self>> {
        let start_removed = self.index;
        let end_removed = self.index + self.chunk_size;
        let (mut removed, mut ranges) = self.ranges
            .drain(..)
            .map(OrdByStart)
            .enumerate()
            .partition::<Vec<_>, _>(|&(i, _)| start_removed <= i && i < end_removed);

        let mut ranges: Vec<_> = ranges.drain(..).map(|(_, r)| r).collect();
        ranges.sort_unstable();
        if ranges.is_empty() {
            return Ok(None);
        }

        let removed: Vec<_> = removed.drain(..).map(|(_, r)| r).collect();
        ranges.sort_unstable();
        assert!(!removed.is_empty());

        self.ranges = ranges
            .drain(..)
            .filter_map(|r| {
                let mut delta_start = 0;
                let mut delta_end = 0;

                for s in &removed {
                    assert!(r != *s);

                    //                      [--------- s --------)
                    // [------- r ----------)
                    if r.0.end <= s.0.start {
                        break;
                    }

                    // [------ s -----)
                    //                [------- r -------)
                    if s.0.end <= r.0.start {
                        let s_len = s.0.end - s.0.start;
                        delta_start += s_len;
                        delta_end += s_len;
                        continue;
                    }

                    //      [------ s -----)
                    // [----------- r -----------)
                    if r.0.start <= s.0.start && s.0.end < r.0.end {
                        delta_end += s.0.end - s.0.start;
                        continue;
                    }

                    // Either
                    //
                    // [----------- s -----------)
                    //      [------ r -----)
                    //
                    // or
                    //
                    //     [--------- s ---------)
                    //                     [---------- r ---------)
                    //
                    // or
                    //
                    //                     [--------- s ---------)
                    //     [---------- r ---------)
                    return None;
                }

                let new_start = r.0.start - delta_start;
                let new_end = r.0.end - delta_end;
                assert!(new_start < new_end);

                Some(new_start..new_end)
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
        assert!(self.chunk_size > 0);
        assert!(self.chunk_size <= self.ranges.len());

        let ranges =
            BTreeSet::from_iter(self.get_ranges_in_chunk().iter().cloned().map(OrdByStart));

        let seed = fs::File::open(seed)?;
        let mut seed = io::BufReader::new(seed);

        let dest = fs::File::create(dest)?;
        let mut dest = io::BufWriter::new(dest);

        const BUF_SIZE: usize = 1024 * 1024;
        let mut buf: Vec<u8> = vec![0; BUF_SIZE];

        let mut offset = 0;
        for r in ranges {
            if offset < r.0.start {
                let mut to_write = r.0.start - offset;

                while to_write > BUF_SIZE as u64 {
                    seed.read_exact(&mut buf)?;
                    dest.write_all(&buf)?;
                    to_write -= BUF_SIZE as u64;
                }

                seed.read_exact(&mut buf[0..to_write as usize])?;
                dest.write_all(&buf[0..to_write as usize])?;
            }

            seed.seek(io::SeekFrom::Start(r.0.end))?;
            offset = r.0.end;
        }

        io::copy(&mut seed, &mut dest)?;
        Ok(true)
    }
}

/// Drives a `Reducer` to completion.
///
/// Deserializes incoming IPC requests, calls the appropriate method on the
/// `Reducer`, and then serializes the result back as an outgoing IPC response.
pub fn run<R: Reducer>() -> ! {
    if let Err(e) = try_run::<R>() {
        eprintln!("error: {}", e);
        process::exit(1);
    }
    process::exit(0);
}

fn try_run<R: Reducer>() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();

    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    let mut line = String::new();

    while {
        line.clear();
        stdin.read_line(&mut line)? > 0
    } {
        let request: Request = serde_json::from_str(&line)?;

        let response = match request {
            Request::Shutdown => {
                return Ok(());
            }
            Request::New(NewRequest { seed }) => {
                let reducer = R::infallible_new(seed);
                Response::New(NewResponse {
                    state: serde_json::to_value(reducer)?,
                })
            }
            Request::Next(NextRequest { seed, state }) => {
                let reducer: R = serde_json::from_value(state)?;
                let next_state = match reducer.infallible_next(seed) {
                    None => None,
                    Some(r) => Some(serde_json::to_value(r)?),
                };
                Response::Next(NextResponse { next_state })
            }
            Request::NextOnInteresting(NextOnInterestingRequest {
                old_seed,
                new_seed,
                state,
            }) => {
                let reducer: R = serde_json::from_value(state)?;
                let next_state = match reducer.infallible_next_on_interesting(old_seed, new_seed) {
                    None => None,
                    Some(r) => Some(serde_json::to_value(r)?),
                };
                Response::NextOnInteresting(NextOnInterestingResponse { next_state })
            }
            Request::FastForward(FastForwardRequest { seed, n, state }) => {
                let reducer: R = serde_json::from_value(state)?;
                let next_state = match reducer.infallible_fast_forward(seed, n) {
                    None => None,
                    Some(r) => Some(serde_json::to_value(r)?),
                };
                Response::FastForward(FastForwardResponse { next_state })
            }
            Request::Reduce(ReduceRequest { seed, state, dest }) => {
                let reducer: R = serde_json::from_value(state)?;
                Response::Reduce(ReduceResponse {
                    reduced: reducer.infallible_reduce(seed, dest),
                })
            }
        };

        serde_json::to_writer(&mut stdout, &response)?;
        writeln!(&mut stdout)?
    }

    Ok(())
}

/// Run a reducer script that removes ranges defined by `R`.
///
/// See `RemoveRanges` for details.
pub fn run_ranges<R: RemoveRanges>() -> ! {
    run::<RemoveRangesReducer<R>>()
}

/// Count the number of lines in the file at the given path.
pub fn count_lines<P: AsRef<Path>>(path: P) -> io::Result<u64> {
    // TODO: this should really just read big buffers of bytes and then use the
    // `bytecount` crate to count how many '\n' bytes are in there. Right now,
    // we're paying the cost of UTF-8 decoding, which isn't necessary.

    let mut num_lines = 0;

    let file = fs::File::open(path)?;
    let mut file = io::BufReader::new(file);
    let mut line = String::new();
    while {
        line.clear();
        file.read_line(&mut line)? > 0
    } {
        num_lines += 1;
    }

    Ok(num_lines)
}

/// Return the first path which has an executable file located at it.
pub fn get_executable<I, P>(paths: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    for p in paths {
        if p.as_ref().is_executable() {
            return Some(p.as_ref().into());
        }
    }

    None
}

/// A `clang_delta` transformation, that we can implement a reducer script with.
///
/// Run the reducer script via `run_clang_delta::<MyClangDelta>()`.
pub trait ClangDelta {
    /// Which `clang_delta` transformation?
    ///
    /// See `clang_delta --verbose-transformations` for details.
    fn transformation() -> &'static str;
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct ClangDeltaReducer<C: ClangDelta> {
    clang_delta: PhantomData<C>,
    index: usize,
}

impl<C: ClangDelta> Reducer for ClangDeltaReducer<C> {
    type Error = io::Error;

    fn new(_seed: PathBuf) -> io::Result<Self> {
        Ok(ClangDeltaReducer {
            clang_delta: PhantomData,
            index: 1,
        })
    }

    fn next(mut self, _seed: PathBuf) -> io::Result<Option<Self>> {
        self.index += 1;
        Ok(Some(self))
    }

    fn next_on_interesting(
        self,
        _old_seed: PathBuf,
        _new_seed: PathBuf,
    ) -> Result<Option<Self>, Self::Error> {
        Ok(Some(self))
    }

    fn fast_forward(mut self, _seed: PathBuf, n: usize) -> io::Result<Option<Self>> {
        self.index += n;
        Ok(Some(self))
    }

    fn reduce(self, seed: PathBuf, dest: PathBuf) -> io::Result<bool> {
        lazy_static! {
            static ref CLANG_DELTA: Option<PathBuf> = get_executable(&[
                "/usr/local/libexec/clang_delta",
                "/usr/libexec/clang_delta",
                "/usr/lib/x86_64-linux-gnu/clang_delta",
                "/usr/lib/creduce/clang_delta",
                "/usr/local/Cellar/creduce/2.7.0/libexec/clang_delta",
            ]);
        }
        match *CLANG_DELTA {
            None => Ok(false),
            Some(ref clang_delta) => {
                let dest = fs::File::create(dest)?;

                let status = process::Command::new(clang_delta)
                    .args(&[
                        format!("--transformation={}", C::transformation()),
                        format!("--counter={}", self.index),
                        seed.display().to_string()
                    ])
                    .stdout(dest)
                    .stderr(process::Stdio::null())
                    .status()?;

                Ok(status.success())
            }
        }
    }
}

/// Run a `clang_delta` reducer script.
pub fn run_clang_delta<C: ClangDelta>() -> ! {
    run::<ClangDeltaReducer<C>>()
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
        let path = PathBuf::from("/dev/null");

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

            assert!(
                reducer
                    .next_on_interesting(path.clone(), path.clone())
                    .expect("next_on_interesting should be OK")
                    .is_none()
            );
        }

        {
            //                 [--------- s ---------)
            // [---------- r ---------)
            let mut reducer = reducer.clone();
            reducer.ranges = vec![8..15, 5..10];
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
    }
}
