#[macro_use]
extern crate lazy_static;
extern crate preduce_regex_reducer;
extern crate regex;

use preduce_regex_reducer::{run_regex, RemoveRegex};
use regex::bytes::Regex;

struct Includes;

impl RemoveRegex for Includes {
    fn remove_regex() -> &'static Regex {
        lazy_static! {
            static ref RE: Regex = Regex::new(r#"(?m)(^\s*#\s*include.*$)"#).unwrap();
        }
        &*RE
    }
}

fn main() {
    run_regex::<Includes>()
}
