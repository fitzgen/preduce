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

extern crate preduce_ipc_types;
extern crate serde;
extern crate serde_json;

use preduce_ipc_types::{FastForwardRequest, NewRequest, NextOnInterestingRequest, NextRequest,
                        ReduceRequest, Request};
use preduce_ipc_types::{FastForwardResponse, NewResponse, NextOnInterestingResponse, NextResponse,
                        ReduceResponse, Response};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::{self, BufRead, Write};
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
