use std::env;
use std::fs;

fn main() {
    if env::var("TRAVIS").ok().map_or(false, |v| v == "true") {
        println!("cargo:rustc-cfg=travis_ci");
    }

    match env::var("PROFILE")
        .expect("should have PROFILE env var")
        .as_ref()
    {
        "debug" => println!(
            "cargo:rustc-env=PREDUCE_TARGET_DIR={}",
            fs::canonicalize("./target/debug").unwrap().display()
        ),
        "release" => println!(
            "cargo:rustc-env=PREDUCE_TARGET_DIR={}",
            fs::canonicalize("./target/release").unwrap().display()
        ),
        otherwise => panic!("Unknown $PROFILE: '{}'", otherwise),
    }
}
