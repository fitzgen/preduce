#[macro_use]
extern crate lazy_static;
extern crate preduce_chunks_reducer;
extern crate preduce_ranges_reducer;
extern crate preduce_reducer_script;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate tempdir;

use preduce_chunks_reducer::Chunks;
use preduce_ranges_reducer::RemoveRangesReducer;
use preduce_reducer_script::{get_executable, Reducer, run};
use std::fs;
use std::io;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::process;

/// A trait for defining reducer scripts that use `topformflat`.
///
/// The reducer script for a `Topformflat` implementation can be run with
/// `run_topformflat::<MyTopformflat>()`.
pub trait Topformflat {
    /// Get the number of levels to flatten with `topformflat`.
    fn flatten() -> u8;
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
enum TopformflatReducer<T: Topformflat> {
    // Did not find `topformflat`.
    NotFound,
    // Did find `topformflat`.
    Found {
        t: PhantomData<T>,
        topformflat: PathBuf,
        chunks: RemoveRangesReducer<Chunks>,
    },
}

lazy_static! {
    static ref TOPFORMFLAT: Option<PathBuf> = get_executable(&[
        "/usr/local/libexec/topformflat",
        "/usr/libexec/topformflat",
        "/usr/lib/x86_64-linux-gnu/topformflat",
        "/usr/lib/creduce/topformflat",
        "/usr/local/Cellar/creduce/2.7.0/libexec/topformflat",
    ]);
}

impl<T: Topformflat> Reducer for TopformflatReducer<T> {
    type Error = io::Error;

    fn new(seed: PathBuf) -> io::Result<Self> {
        match *TOPFORMFLAT {
            None => Ok(TopformflatReducer::NotFound),
            Some(ref topformflat) => {
                let dir = tempdir::TempDir::new("topformflat-reducer")?;
                let flattened = dir.path().join("flattened");

                {
                    let flattened_file = fs::File::create(&flattened)?;
                    let seed = fs::File::open(seed)?;
                    let status = process::Command::new(topformflat)
                        .arg(T::flatten().to_string())
                        .stdin(seed)
                        .stdout(flattened_file)
                        .status()?;
                    if !status.success() {
                        return Ok(TopformflatReducer::NotFound);
                    }
                }

                Ok(TopformflatReducer::Found {
                    t: PhantomData,
                    topformflat: topformflat.clone(),
                    chunks: RemoveRangesReducer::new(flattened)?,
                })
            }
        }
    }

    fn next(self, seed: PathBuf) -> io::Result<Option<Self>> {
        let (chunks, topformflat) = match self {
            TopformflatReducer::NotFound => return Ok(None),
            TopformflatReducer::Found {
                chunks,
                topformflat,
                ..
            } => match chunks.next(seed)? {
                None => return Ok(None),
                Some(chunks) => (chunks, topformflat),
            },
        };
        Ok(Some(TopformflatReducer::Found {
            t: PhantomData,
            topformflat,
            chunks,
        }))
    }

    fn next_on_interesting(self, old_seed: PathBuf, new_seed: PathBuf) -> io::Result<Option<Self>> {
        let (chunks, topformflat) = match self {
            TopformflatReducer::NotFound => return Ok(None),
            TopformflatReducer::Found {
                chunks,
                topformflat,
                ..
            } => match chunks.next_on_interesting(old_seed, new_seed)? {
                None => return Ok(None),
                Some(chunks) => (chunks, topformflat),
            },
        };
        Ok(Some(TopformflatReducer::Found {
            t: PhantomData,
            topformflat,
            chunks,
        }))
    }

    fn reduce(self, seed: PathBuf, dest: PathBuf) -> io::Result<bool> {
        let (chunks, topformflat) = match self {
            TopformflatReducer::NotFound => return Ok(false),
            TopformflatReducer::Found {
                chunks,
                topformflat,
                ..
            } => (chunks, topformflat),
        };

        let dir = tempdir::TempDir::new("topformflat-reducer")?;
        let flattened = dir.path().join("flattened");

        {
            let flattened_file = fs::File::create(&flattened)?;
            let seed = fs::File::open(seed)?;
            let status = process::Command::new(topformflat)
                .arg(T::flatten().to_string())
                .stdin(seed)
                .stdout(flattened_file)
                .status()?;
            if !status.success() {
                return Err(io::Error::new(io::ErrorKind::Other, "`topformflat` failed"));
            }
        }

        chunks.reduce(flattened, dest)
    }
}

/// Run a reducer script that uses `topformflat`.
pub fn run_topformflat<T: Topformflat>() -> ! {
    run::<TopformflatReducer<T>>()
}

/// Declare and run a `clang_delta` reducer script.
#[macro_export]
macro_rules! topformflat_reducer {
    ( $flatten:expr ) => {
        fn main() {
            struct Reducer;

            impl $crate::Topformflat for Reducer {
                fn flatten() -> u8 {
                    $flatten
                }
            }

            $crate::run_topformflat::<Reducer>()
        }
    }
}
