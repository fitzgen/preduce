extern crate preduce_reducer_script;
extern crate serde;
#[macro_use]
extern crate serde_derive;

use preduce_reducer_script::{Reducer, run};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Chunks {
    num_lines: u64,
    chunk_size: u64,
    index: u64
}

impl Reducer for Chunks {
    type Error = io::Error;

    fn new(seed: PathBuf) -> io::Result<Self> {
        let num_lines = preduce_reducer_script::count_lines(seed)?;
        let chunk_size = num_lines;
        let index = 0;

        Ok(Chunks {
            num_lines,
            chunk_size,
            index,
        })
    }

    fn next(mut self, _seed: PathBuf) -> io::Result<Option<Self>> {
        assert!(self.chunk_size > 0);
        assert!(self.chunk_size <= self.num_lines);

        self.index += 1;
        Ok(if self.index == self.num_lines - (self.chunk_size - 1) {
            if self.chunk_size == 1 {
                None
            } else {
                self.chunk_size /= 2;
                self.index = 0;
                Some(self)
            }
        } else {
            Some(self)
        })
    }

    fn next_on_interesting(mut self, _old_seed: PathBuf, _new_seed: PathBuf) -> io::Result<Option<Self>> {
        assert!(self.chunk_size > 0);
        assert!(self.chunk_size <= self.num_lines);

        self.num_lines -= self.chunk_size;
        if self.num_lines == 0 {
            return Ok(None);
        }

        if self.index >= self.num_lines - (self.chunk_size - 1) {
            self.index = 0;
        }

        Ok(Some(self))
    }

    fn reduce(self, seed: PathBuf, dest: PathBuf) -> io::Result<bool> {
        assert!(self.chunk_size > 0);
        assert!(self.chunk_size <= self.num_lines);

        if self.index >= self.num_lines {
            return Ok(false);
        }

        let seed = fs::File::open(seed)?;
        let mut seed = io::BufReader::new(seed);

        let dest = fs::File::create(dest)?;
        let mut dest = io::BufWriter::new(dest);

        let mut line = String::new();

        for _ in 0..self.index {
            line.clear();
            seed.read_line(&mut line)?;
            dest.write_all(line.as_bytes())?;
        }

        for _ in 0..self.chunk_size {
            line.clear();
            seed.read_line(&mut line)?;
        }

        io::copy(&mut seed, &mut dest)?;
        Ok(true)
    }
}

fn main() {
    run::<Chunks>()
}
