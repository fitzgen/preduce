extern crate preduce_balanced_reducer;
use preduce_balanced_reducer::{run_balanced, RemoveBalanced};

struct Curlies;

impl RemoveBalanced for Curlies {
    fn remove_balanced() -> (u8, u8) {
        (b'{', b'}')
    }
}

fn main() {
    run_balanced::<Curlies>()
}
