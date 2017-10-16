extern crate preduce;

use preduce::test_case::TestCaseMethods;
use preduce::traits::Reducer;
use std::io;
use std::path;
use std::process::Command;

fn test_preduce_run<P, Q, R, I>(test_case: P, predicate: Q, reducers: I)
where
    P: AsRef<path::Path>,
    Q: AsRef<path::Path>,
    R: AsRef<path::Path>,
    I: IntoIterator<Item = R>,
{
    let stdout = io::stdout();
    let _stdout = stdout.lock();

    let test_case = test_case.as_ref();
    let copy_of_test_case = test_case.with_extension("preduce-run");
    let status = Command::new("cp")
        .args(&[
            test_case.display().to_string(),
            copy_of_test_case.display().to_string(),
        ])
        .status()
        .expect("should run cp OK");
    assert!(status.success(), "cp should exit OK");

    let status = Command::new("cargo")
        .arg("run")
        .arg(copy_of_test_case.display().to_string())
        .arg(predicate.as_ref().display().to_string())
        .args(
            reducers
                .into_iter()
                .map(|r| r.as_ref().display().to_string()),
        )
        .status()
        .expect("should run preduce OK");
    assert!(status.success(), "preduce should exit OK");

    let expected = path::Path::new("tests/expectations")
        .join(test_case.file_name().unwrap())
        .display()
        .to_string();
    let actual = copy_of_test_case.display().to_string();

    let status = Command::new("diff")
        .args(&["-U8", &expected, &actual])
        .status()
        .expect("should run diff OK");
    assert!(status.success(), "diff should exit OK");
}

macro_rules! test_preduce_runs {
    (
        $(
            $name:ident => {
                $test_case:expr,
                judged by $predicate:expr,
                reductions by [
                    $(
                        $reducer:expr ,
                    )*
                ]
            }
        )*
    ) => {
        $(
            #[test]
            fn $name() {
                test_preduce_run($test_case, $predicate, &[
                    $(
                        $reducer ,
                    )*
                ]);
            }
        )*
    }
}

test_preduce_runs! {
    lorem_ipsum => {
        "tests/fixtures/lorem-ipsum.txt",
        judged by "tests/predicates/has-lorem.sh",
        reductions by [
            "reducers/chunks.py",
            "reducers/lines.py",
        ]
    }
    class_nine => {
        "tests/fixtures/nested-classes.cpp",
        judged by "tests/predicates/class-nine-compiles.sh",
        reductions by [
            "reducers/balanced-curly.py",
            "reducers/lines.py",
            "reducers/clang-delta-reduce-class-template-param.py",
        ]
    }
}

fn test_reducer<P, Q, I, R>(reducer: P, seed: Q, expecteds: I)
where
    P: AsRef<path::Path>,
    Q: AsRef<path::Path>,
    R: AsRef<path::Path>,
    I: IntoIterator<Item = R>,
{
    let judge = preduce::interesting::NonEmpty;

    let seed_string = seed.as_ref().display().to_string();
    let seed = preduce::test_case::Interesting::initial(seed, &judge)
        .expect("should run interesting test OK")
        .expect("should be interesting");

    let mut reducer = preduce::reducers::Script::new(reducer).expect("should create reducer OK");
    let state = reducer.new_state(&seed).expect("reducer should create new state");
    let mut state = Some(state);

    for expected in expecteds {
        let next_state = {
            let state_ref = state.as_ref().expect("Expecting another reduction, should have state");

            let reduction = reducer.reduce(&seed, state_ref)
                .expect("should generate next reduction OK")
                .expect("should not be exhausted");

            let expected = expected.as_ref().display().to_string();
            let actual = reduction.path().display().to_string();

            let output = Command::new("diff")
                .args(&["-U8", &seed_string, &actual])
                .output()
                .expect("should run diff OK");

            println!();
            println!();
            println!();
            println!("=======================================================");
            println!("Actual reduction generated is:");
            println!("-------------------------------------------------------");
            println!("{}", String::from_utf8_lossy(&output.stdout));
            println!("=======================================================");
            println!();
            println!();
            println!();
            println!("=======================================================");
            println!("Diff with expected reduction generated is:");
            println!("-------------------------------------------------------");

            let output = Command::new("diff")
                .args(&["-U8", &expected, &actual])
                .output()
                .expect("should run diff OK");

            println!("{}", String::from_utf8_lossy(&output.stdout));
            println!("=======================================================");

            assert!(output.status.success(), "diff should exit OK");

            reducer.next_state(&seed, state_ref)
                .expect("should call next_state OK")
        };
        state = next_state;
    }
}

macro_rules! test_reducers {
    (
        $(
            $name:ident => {
                $reducer:expr ,
                seeded with $seed:expr ,
                generates [ $( $expected:expr , )* ]
            }
        )*
    ) => {
        $(
            #[test]
            fn $name() {
                test_reducer(
                    $reducer,
                    $seed,
                    &[
                        $( $expected , )*
                    ]
                );
            }
        )*
    }
}

test_reducers! {
    balanced_angle => {
        "reducers/balanced-angle.py",
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/balanced-angle-0",
            "tests/expectations/balanced-angle-1",
        ]
    }
    balanced_curly => {
        "reducers/balanced-curly.py",
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/balanced-curly-0",
            "tests/expectations/balanced-curly-1",
            "tests/expectations/balanced-curly-2",
            "tests/expectations/balanced-curly-3",
            "tests/expectations/balanced-curly-4",
            "tests/expectations/balanced-curly-5",
        ]
    }
    balanced_paren => {
        "reducers/balanced-paren.py",
        seeded with "tests/fixtures/some-includes.cpp",
        generates [
            "tests/expectations/balanced-paren-0",
            "tests/expectations/balanced-paren-1",
        ]
    }
    balanced_square => {
        "reducers/balanced-square.py",
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/balanced-square-0",
            "tests/expectations/balanced-square-1",
        ]
    }
    blank => {
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-blank"),
        seeded with "tests/fixtures/wow.cpp",
        generates [
            "tests/expectations/blank-0",
            "tests/expectations/blank-1",
            "tests/expectations/blank-2",
        ]
    }
    chunks => {
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-chunks"),
        seeded with "tests/fixtures/lines.txt",
        generates [
            "tests/expectations/chunks-0",
            "tests/expectations/chunks-1",
            "tests/expectations/chunks-2",
            "tests/expectations/chunks-3",
            "tests/expectations/chunks-4",
            "tests/expectations/chunks-5",
            "tests/expectations/chunks-6",
            "tests/expectations/chunks-7",
            "tests/expectations/chunks-8",
        ]
    }
    lines => {
        "reducers/lines.py",
        seeded with "tests/fixtures/lorem-ipsum.txt",
        generates [
            "tests/expectations/lines-0",
            "tests/expectations/lines-1",
            "tests/expectations/lines-2",
            "tests/expectations/lines-3",
        ]
    }
    clang_format => {
        "reducers/clang-format.py",
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/clang-format-0",
        ]
    }
    clang_delta_reduce_class_template_param => {
        "reducers/clang-delta-reduce-class-template-param.py",
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/clang-delta-reduce-class-template-param-0",
        ]
    }
    clang_delta_remove_unused_outer_class => {
        "reducers/clang-delta-remove-unused-outer-class.py",
        seeded with "tests/fixtures/wow.cpp",
        generates [
            "tests/expectations/clang-delta-remove-unused-outer-class-0",
        ]
    }
    includes => {
        "reducers/includes.py",
        seeded with "tests/fixtures/some-includes.cpp",
        generates [
            "tests/expectations/includes-0",
            "tests/expectations/includes-1",
            "tests/expectations/includes-2",
            "tests/expectations/includes-3",
        ]
    }
}

// For whatever reason, we can't find `clex` on Travis CI.
#[cfg(not(travis_ci))]
test_reducers! {
    clex_rename_toks => {
        "reducers/clex-rename-toks.py",
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/clex-rename-toks-0",
            "tests/expectations/clex-rename-toks-1",
            "tests/expectations/clex-rename-toks-2",
            "tests/expectations/clex-rename-toks-3",
            "tests/expectations/clex-rename-toks-4",
        ]
    }
    clex_rm_toks_1 => {
        "reducers/clex-rm-toks-1.py",
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/clex-rm-toks-1-0",
            "tests/expectations/clex-rm-toks-1-1",
            "tests/expectations/clex-rm-toks-1-2",
            "tests/expectations/clex-rm-toks-1-3",
        ]
    }
}
