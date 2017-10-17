extern crate preduce_reducer_script;
use preduce_reducer_script::{RemoveBalanced, run_balanced};

struct Angles;

impl RemoveBalanced for Angles {
    fn remove_balanced() -> (u8, u8) {
        (b'<', b'>')
    }
}

fn main() {
    run_balanced::<Angles>()
}
