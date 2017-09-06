use std::env;

fn main() {
    if env::var("TRAVIS").ok().map_or(false, |v| v == "true") {
        println!("cargo:rustc-cfg=travis_ci");
    }
}
