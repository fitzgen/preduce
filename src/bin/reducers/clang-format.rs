extern crate preduce_reducer_script;
extern crate serde;
#[macro_use]
extern crate serde_derive;

use preduce_reducer_script::{run, Reducer};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process;

#[derive(Debug, Deserialize, Serialize)]
struct ClangFormat;

impl Reducer for ClangFormat {
    type Error = io::Error;

    fn new(_seed: PathBuf) -> io::Result<Self> {
        Ok(ClangFormat)
    }

    fn next(self, _seed: PathBuf) -> io::Result<Option<Self>> {
        Ok(None)
    }

    fn next_on_interesting(
        self,
        _old_seed: PathBuf,
        _new_seed: PathBuf,
    ) -> io::Result<Option<Self>> {
        Ok(Some(self))
    }

    fn fast_forward(self, _seed: PathBuf, _n: usize) -> io::Result<Option<Self>> {
        Ok(Some(self))
    }

    fn reduce(self, seed: PathBuf, dest: PathBuf) -> io::Result<bool> {
        let dest = fs::File::create(dest)?;
        let seed = seed.display().to_string();

        let status = process::Command::new("clang-format")
            .args(&["-style", "{SpacesInAngles: true, IndentWidth: 0}", &seed])
            .stdout(dest)
            .stderr(process::Stdio::null())
            .status()?;

        Ok(status.success())
    }
}

fn main() {
    run::<ClangFormat>();
}
