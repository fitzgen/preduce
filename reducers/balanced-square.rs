extern crate preduce_reducer_script;
use preduce_reducer_script::{RemoveBalanced, run_balanced};

struct Squares;

impl RemoveBalanced for Squares {
    fn remove_balanced() -> (u8, u8) {
        (b'[', b']')
    }
}

fn main() {
    run_balanced::<Squares>()
}
