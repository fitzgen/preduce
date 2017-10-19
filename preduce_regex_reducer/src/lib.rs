extern crate preduce_ranges_reducer;
extern crate regex;

use preduce_ranges_reducer::{RemoveRanges, run_ranges};

use std::fs;
use std::io::{self, Read};
use std::marker::PhantomData;
use std::ops::Range;
use std::path::PathBuf;

/// A trait for defining a regex that we use to implement a reducer that tries
/// removing the regex's matches' capture groups from the seed test case.
///
/// To run the reducer script, use `run_regex::<MyRemoveRegex>()`.
///
/// ### Example
///
/// A reducer script that truncates strings like `"blah blah blah"` into `""`.
///
/// ```
/// #[macro_use]
/// extern crate lazy_static;
/// extern crate preduce_regex_reducer;
/// extern crate regex;
///
/// use preduce_regex_reducer::{RemoveRegex, run_regex};
/// use regex::bytes::Regex;
///
/// struct Strings;
///
/// impl RemoveRegex for Strings {
///     fn remove_regex() -> &'static Regex {
///         lazy_static! {
///             static ref RE: Regex = Regex::new(r#""([^"]+)""#).unwrap();
///         }
///         &*RE
///     }
/// }
///
/// fn main() {
/// #   #![allow(unreachable_code)]
/// #   return;
///     run_regex::<Strings>()
/// }
/// ```
pub trait RemoveRegex {
    /// Return a static reference to the regex.
    fn remove_regex() -> &'static regex::bytes::Regex;
}

struct RemoveRegexReducer<R: RemoveRegex>(PhantomData<R>);

impl<R: RemoveRegex> RemoveRanges for RemoveRegexReducer<R> {
    fn remove_ranges(seed: PathBuf) -> io::Result<Vec<Range<u64>>> {
        let mut buf = vec![];

        {
            let mut file = fs::File::open(seed)?;
            file.read_to_end(&mut buf)?;
        }

        let mut ranges = vec![];

        let regex = R::remove_regex();
        for mat in regex.captures_iter(&buf) {
            // Skip the first capture, as that's the whole match.
            for cap in mat.iter().skip(1).filter_map(|c| c) {
                let start = cap.start() as u64;
                let end = cap.end() as u64;
                ranges.push(start..end);
            }
        }

        Ok(ranges)
    }
}

/// Run a reducer script that removes `R`'s regex's matches from the seed test
/// case.
///
/// See `RemoveRegex` for details.
pub fn run_regex<R: RemoveRegex>() -> ! {
    run_ranges::<RemoveRegexReducer<R>>()
}
