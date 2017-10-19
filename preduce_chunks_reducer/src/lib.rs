extern crate preduce_ranges_reducer;
extern crate serde;
#[macro_use]
extern crate serde_derive;

use preduce_ranges_reducer::RemoveRanges;
use std::fs;
use std::io::{self, Read};
use std::ops::Range;
use std::path::PathBuf;

/// A `RemoveRanges` implementation that removes chunks of lines from the seed
/// file.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Chunks;

impl RemoveRanges for Chunks {
    fn remove_ranges(seed: PathBuf) -> io::Result<Vec<Range<u64>>> {
        let mut seed = fs::File::open(seed)?;
        let mut ranges = vec![];

        const BUF_SIZE: usize = 1024 * 1024;
        let mut buf: Vec<u8> = vec![0; BUF_SIZE];

        let mut start_of_line = 0;
        let mut current_index = 0;
        let mut bytes_read;
        while {
            bytes_read = seed.read(&mut buf)?;
            bytes_read > 0
        } {
            for b in &buf[0..bytes_read] {
                current_index += 1;
                if *b == b'\n' {
                    ranges.push(start_of_line..current_index);
                    start_of_line = current_index;
                }
            }
        }

        Ok(ranges)
    }
}
