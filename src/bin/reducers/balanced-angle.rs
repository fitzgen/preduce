extern crate preduce_balanced_reducer;
use preduce_balanced_reducer::{run_balanced, RemoveBalanced};

struct Angles;

impl RemoveBalanced for Angles {
    fn remove_balanced() -> (u8, u8) {
        (b'<', b'>')
    }
}

fn main() {
    run_balanced::<Angles>()
}
