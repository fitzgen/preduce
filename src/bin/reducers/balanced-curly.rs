extern crate preduce_reducer_script;
use preduce_reducer_script::{run_balanced, RemoveBalanced};

struct Curlies;

impl RemoveBalanced for Curlies {
    fn remove_balanced() -> (u8, u8) {
        (b'{', b'}')
    }
}

fn main() {
    run_balanced::<Curlies>()
}
