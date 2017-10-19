#[macro_use]
extern crate lazy_static;
extern crate preduce_reducer_script;
extern crate serde;
#[macro_use]
extern crate serde_derive;

use preduce_reducer_script::{get_executable, Reducer, run};
use std::fs;
use std::io;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::process;

/// A trait for defining reducers that use `clex`.
pub trait Clex {
    /// The `clex` command to invoke.
    fn clex_command() -> &'static str;
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct ClexReducer<C: Clex> {
    clex: PhantomData<C>,
    index: usize,
}

impl<C: Clex> Reducer for ClexReducer<C> {
    type Error = io::Error;

    fn new(_seed: PathBuf) -> io::Result<Self> {
        Ok(ClexReducer {
            clex: PhantomData,
            index: 0,
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
            static ref CLEX: Option<PathBuf> = get_executable(&[
                "/usr/local/libexec/clex",
                "/usr/libexec/clex",
                "/usr/lib/x86_64-linux-gnu/clex",
                "/usr/lib/creduce/clex",
                "/usr/local/Cellar/creduce/2.7.0/libexec/clex",
            ]);
        }
        match *CLEX {
            None => Ok(false),
            Some(ref clex) => {
                let dest = fs::File::create(dest)?;
                let index = self.index.to_string();
                let seed = seed.display().to_string();
                let status = process::Command::new(clex)
                    .args(&[C::clex_command(), &index, &seed])
                    .stdout(dest)
                    .stderr(process::Stdio::null())
                    .status()?;
                // I don't know why clex is written with these bizarre exit
                // codes...
                Ok(status.code() == Some(51))
            }
        }
    }
}

/// Run a `clex` reducer script.
pub fn run_clex<C: Clex>() -> ! {
    run::<ClexReducer<C>>()
}

/// Declare and run a `clex` reducer script.
#[macro_export]
macro_rules! clex_reducer {
    ( $command:expr ) => {
        fn main() {
            struct Reducer;

            impl $crate::Clex for Reducer {
                fn clex_command() -> &'static str {
                    $command
                }
            }

            $crate::run_clex::<Reducer>()
        }
    }
}
