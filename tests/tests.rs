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

    let status = Command::new(concat!(env!("PREDUCE_TARGET_DIR"), "/preduce"))
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
                candidates by [
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
                ][..]);
            }
        )*
    }
}

test_preduce_runs! {
    lorem_ipsum => {
        "tests/fixtures/lorem-ipsum.txt",
        judged by "tests/predicates/has-lorem.sh",
        candidates by [
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-chunks"),
        ]
    }
    cannot_reduce => {
        "tests/fixtures/cannot-reduce.txt",
        judged by "tests/predicates/is-cannot-reduce.sh",
        candidates by [
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-balanced-angle"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-balanced-curly"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-balanced-paren"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-balanced-square"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-chunks"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-1"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-2"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-3"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-4"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-5"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-6"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-7"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-8"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-9"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-10"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-11"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-12"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-13"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-14"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-15"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-16"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-aggregate-to-scalar"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-callexpr-to-value"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-combine-global-var"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-combine-local-var"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-copy-propagation"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-empty-struct-to-int"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-instantiate-template-param"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-instantiate-template-type-param-to-int"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-lift-assignment-expr"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-local-to-global"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-move-function-body"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-move-global-var"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-param-to-global"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-param-to-local"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-reduce-array-dim"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-reduce-array-size"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-reduce-class-template-param"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-reduce-pointer-level"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-reduce-pointer-pairs"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-addr-taken"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-array"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-base-class"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-ctor-initializer"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-enum-member-value"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-namespace"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-nested-function"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-pointer"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-trivial-base-template"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-unresolved-base"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-unused-enum-member"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-unused-field"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-unused-function"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-unused-outer-class"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-rename-class"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-rename-cxx-method"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-rename-fun"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-rename-param"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-rename-var"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-array-access-with-index"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-array-index-var"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-callexpr"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-class-with-base-template-spec"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-dependent-name"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-dependent-typedef"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-derived-class"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-function-def-with-decl"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-one-level-typedef-type"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-simple-typedef"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-replace-undefined-function"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-return-void"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-simple-inliner"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-simplify-callexpr"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-simplify-comma-expr"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-simplify-dependent-typedef"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-simplify-if"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-simplify-nested-class"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-simplify-recursive-template-instantiation"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-simplify-struct-union-decl"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-simplify-struct"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-template-arg-to-int"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-template-non-type-arg-to-int"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-unify-function-decl"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-union-to-struct"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-vector-to-array"),
        ]
    }
    class_nine => {
        "tests/fixtures/nested-classes.cpp",
        judged by "tests/predicates/class-nine-compiles.sh",
        candidates by [
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-balanced-curly"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-chunks"),
            concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-reduce-class-template-param"),
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
    let state = reducer
        .new_state(&seed)
        .expect("reducer should create new state");
    let mut state = Some(state);

    for expected in expecteds {
        let next_state = {
            let state_ref = state
                .as_ref()
                .expect("Expecting another candidate, should have state");

            let candidate = reducer
                .reduce(&seed, state_ref)
                .expect("should generate next candidate OK")
                .expect("should not be exhausted");

            let expected = expected.as_ref().display().to_string();
            let actual = candidate.path().display().to_string();

            let output = Command::new("diff")
                .args(&["-U100", &seed_string, &actual])
                .output()
                .expect("should run diff OK");

            println!();
            println!();
            println!();
            println!("=======================================================");
            println!("Actual candidate generated is:");
            println!("-------------------------------------------------------");
            println!("{}", String::from_utf8_lossy(&output.stdout));
            println!("=======================================================");
            println!();
            println!();
            println!();
            println!("=======================================================");
            println!("Diff with expected candidate generated is:");
            println!("-------------------------------------------------------");

            let output = Command::new("diff")
                .args(&["-U100", &expected, &actual])
                .output()
                .expect("should run diff OK");

            println!("{}", String::from_utf8_lossy(&output.stdout));
            println!("=======================================================");

            assert!(output.status.success(), "diff should exit OK");

            reducer
                .next_state(&seed, state_ref)
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
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-balanced-angle"),
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/balanced-angle-0",
            "tests/expectations/balanced-angle-1",
        ]
    }
    balanced_curly => {
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-balanced-curly"),
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
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-balanced-paren"),
        seeded with "tests/fixtures/parens.txt",
        generates [
            "tests/expectations/balanced-paren-0",
            "tests/expectations/balanced-paren-1",
            "tests/expectations/balanced-paren-2",
            "tests/expectations/balanced-paren-3",
            "tests/expectations/balanced-paren-4",
            "tests/expectations/balanced-paren-5",
            "tests/expectations/balanced-paren-6",
            "tests/expectations/balanced-paren-7",
            "tests/expectations/balanced-paren-8",
            "tests/expectations/balanced-paren-9",
        ]
    }
    balanced_square => {
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-balanced-square"),
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
    clang_format => {
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-format"),
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/clang-format-0",
        ]
    }
    clang_delta_reduce_class_template_param => {
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-reduce-class-template-param"),
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/clang-delta-reduce-class-template-param-0",
        ]
    }
    clang_delta_remove_unused_outer_class => {
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clang-delta-remove-unused-outer-class"),
        seeded with "tests/fixtures/wow.cpp",
        generates [
            "tests/expectations/clang-delta-remove-unused-outer-class-0",
        ]
    }
    includes => {
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-includes"),
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
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rename-toks"),
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
        concat!(env!("PREDUCE_TARGET_DIR"), "/preduce-reducer-clex-rm-toks-1"),
        seeded with "tests/fixtures/nested-classes.cpp",
        generates [
            "tests/expectations/clex-rm-toks-1-0",
            "tests/expectations/clex-rm-toks-1-1",
            "tests/expectations/clex-rm-toks-1-2",
            "tests/expectations/clex-rm-toks-1-3",
        ]
    }
}
