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
                        seed.display().to_string(),
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

/// Declare and run a `clang_delta` reducer script.
#[macro_export]
macro_rules! clang_delta_reducer {
    ( $transformation:expr ) => {
        fn main() {
            struct Reducer;

            impl $crate::ClangDelta for Reducer {
                fn transformation() -> &'static str {
                    $transformation
                }
            }

            $crate::run_clang_delta::<Reducer>()
        }
    }
}
