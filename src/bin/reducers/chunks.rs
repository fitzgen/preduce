extern crate preduce_chunks_reducer;
extern crate preduce_ranges_reducer;

use preduce_chunks_reducer::Chunks;
use preduce_ranges_reducer::run_ranges;

fn main() {
    run_ranges::<Chunks>()
}
