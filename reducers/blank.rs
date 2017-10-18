extern crate preduce_reducer_script;
extern crate serde;
#[macro_use]
extern crate serde_derive;

use preduce_reducer_script::{RemoveRanges, run_ranges};
use std::fs;
use std::io::{self, BufRead};
use std::ops;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
struct Blank;

impl RemoveRanges for Blank {
    fn remove_ranges(seed: PathBuf) -> io::Result<Vec<ops::Range<u64>>> {
        let seed = fs::File::open(seed)?;
        let mut seed = io::BufReader::new(seed);

        let mut ranges = vec![];

        let mut offset = 0u64;
        let mut line = String::new();
        while {
            line.clear();
            seed.read_line(&mut line)? > 0
        } {
            if line.trim().is_empty() {
                ranges.push(offset..offset + line.len() as u64);
            }
            offset += line.len() as u64;
        }

        Ok(ranges)
    }
}

fn main() {
    run_ranges::<Blank>()
}
