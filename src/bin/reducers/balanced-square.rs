extern crate preduce_reducer_script;
use preduce_reducer_script::{run_balanced, RemoveBalanced};

struct Squares;

impl RemoveBalanced for Squares {
    fn remove_balanced() -> (u8, u8) {
        (b'[', b']')
    }
}

fn main() {
    run_balanced::<Squares>()
}
