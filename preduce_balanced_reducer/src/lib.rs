extern crate preduce_ranges_reducer;

use preduce_ranges_reducer::{run_ranges, RemoveRanges};
use std::fs;
use std::io::{self, Read};
use std::marker::PhantomData;
use std::ops::Range;
use std::path::PathBuf;

/// A trait for defining reducer scripts that remove the contents within
/// balanced parens/brackets/braces/etc.
///
/// To run a reducer script implemented by a `RemoveBalanced` implementation,
/// call `run_balanced::<MyRemoveBalanced>()`.
///
/// ### Example
///
/// A reducer script that removes the contents of balanced parentheses.
///
/// ```
/// extern crate preduce_balanced_reducer;
/// use preduce_balanced_reducer::{RemoveBalanced, run_balanced};
///
/// struct Parens;
///
/// impl RemoveBalanced for Parens {
///     fn remove_balanced() -> (u8, u8) {
///         (b'(', b')')
///     }
/// }
///
/// fn main() {
/// #   #![allow(unreachable_code)]
/// #   return;
///     run_balanced::<Parens>()
/// }
/// ```
pub trait RemoveBalanced {
    /// Return the open and closing bytes.
    fn remove_balanced() -> (u8, u8);
}

struct RemoveBalancedReducer<R: RemoveBalanced>(PhantomData<R>);

impl<R: RemoveBalanced> RemoveRanges for RemoveBalancedReducer<R> {
    fn remove_ranges(seed: PathBuf) -> io::Result<Vec<Range<u64>>> {
        let (open, close) = R::remove_balanced();
        let mut ranges = vec![];
        let mut stack = vec![];
        let mut offset = 0u64;

        const BUF_SIZE: usize = 1024 * 1024;
        let mut buf = vec![0; BUF_SIZE];

        let mut file = fs::File::open(seed)?;
        let mut bytes_read;
        while {
            bytes_read = file.read(&mut buf)?;
            bytes_read > 0
        } {
            for b in &buf[0..bytes_read] {
                if *b == open {
                    stack.push(offset);
                } else if *b == close {
                    if let Some(start) = stack.pop() {
                        debug_assert!(start < offset);
                        ranges.push(start..offset + 1);

                        let inner_start = start + 1;
                        let inner_end = offset;
                        if inner_start < inner_end {
                            ranges.push(inner_start..inner_end);
                        }
                    }
                }
                offset += 1;
            }
        }

        Ok(ranges)
    }
}

/// Run a reducer script that removes text within balanced brackets/parens/etc
/// from the seed test case.
///
/// See `RemoveBalanced` for details.
pub fn run_balanced<R: RemoveBalanced>() -> ! {
    run_ranges::<RemoveBalancedReducer<R>>()
}
