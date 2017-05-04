extern crate preduce;

use std::process::Command;

#[test]
fn tests_sh() {
    let mut child = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tests.sh"))
        .spawn()
        .expect("Should spawn tests.sh OK");

    let result = child.wait().expect("Should wait for tests.sh OK");
    assert!(result.success(), "tests.sh should exit OK");
}
