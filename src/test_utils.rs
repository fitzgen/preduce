#![allow(missing_docs)]

use std::env;
use std::path;

pub fn get_script(s: &str) -> path::PathBuf {
    let mut script = path::PathBuf::new();
    if let Ok(dir) = env::var("CARGO_MANIFEST_DIR") {
        script.push(dir);
    }
    script.push("tests");
    script.push(s);
    script
}

pub fn get_exit_0() -> path::PathBuf {
    get_script("exit_0.sh")
}

pub fn get_exit_1() -> path::PathBuf {
    get_script("exit_1.sh")
}
