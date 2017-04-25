#![allow(missing_docs)]

use std::env;
use std::path;

pub fn get_predicate(s: &str) -> path::PathBuf {
    let mut script = path::PathBuf::new();
    if let Ok(dir) = env::var("CARGO_MANIFEST_DIR") {
        script.push(dir);
    }
    script.push("tests");
    script.push("predicates");
    script.push(s);
    assert!(script.is_file(), "get_predicate called on missing file");
    script.canonicalize().unwrap()
}

pub fn get_reducer(s: &str) -> path::PathBuf {
    let mut script = path::PathBuf::new();
    if let Ok(dir) = env::var("CARGO_MANIFEST_DIR") {
        script.push(dir);
    }
    script.push("tests");
    script.push("reducers");
    script.push(s);
    assert!(script.is_file(), "get_reducer called on missing file");
    script.canonicalize().unwrap()
}

pub fn get_exit_0() -> path::PathBuf {
    get_predicate("exit_0.sh")
}

pub fn get_exit_1() -> path::PathBuf {
    get_predicate("exit_1.sh")
}
