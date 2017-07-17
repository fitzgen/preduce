//! Determining the priority of potential reductions.

use score::Score;
use std::collections::HashMap;
use test_case::{self, TestCaseMethods};
use traits;

#[derive(Debug, Default)]
struct Observed {
    smallest_interesting_count: u32,
    not_smallest_interesting_count: u32,
    not_interesting_count: u32,
}

impl Observed {
    fn total(&self) -> u32 {
        self.smallest_interesting_count + self.not_smallest_interesting_count +
            self.not_interesting_count
    }

    fn interesting(&self) -> u32 {
        self.smallest_interesting_count + self.not_smallest_interesting_count
    }
}

/// An `Oracle` implementation that scores test cases based on their reduction's
/// historical rate of producing interesting test cases.
#[derive(Debug, Default)]
pub struct InterestingRate {
    observations: HashMap<String, Observed>,
}

impl traits::Oracle for InterestingRate {
    fn observe_smallest_interesting(&mut self, interesting: &test_case::Interesting) {
        self.observations
            .entry(interesting.provenance().into())
            .or_insert_with(Default::default)
            .smallest_interesting_count += 1;
    }

    fn observe_not_smallest_interesting(&mut self, interesting: &test_case::Interesting) {
        self.observations
            .entry(interesting.provenance().into())
            .or_insert_with(Default::default)
            .not_smallest_interesting_count += 1;
    }

    fn observe_not_interesting(&mut self, reduction: &test_case::PotentialReduction) {
        self.observations
            .entry(reduction.provenance().into())
            .or_insert_with(Default::default)
            .not_interesting_count += 1;
    }

    fn predict(&mut self, reduction: &test_case::PotentialReduction) -> Score {
        let observed = self.observations
            .entry(reduction.provenance().into())
            .or_insert_with(Default::default);
        let total = observed.total();
        if total == 0 {
            Score::new(0.0)
        } else {
            Score::new(observed.interesting() as f64 / total as f64)
        }
    }
}

macro_rules! creduce_pass {
    ( $pass:expr ) => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/reducers/", $pass, ".py")
    }
}

const CREDUCE_PASSES: &'static [&'static str] =
    &[
        creduce_pass!("blank"),
        creduce_pass!("lines"),
        creduce_pass!("topformflat-0"),
        creduce_pass!("topformflat-1"),
        creduce_pass!("topformflat-2"),
        creduce_pass!("topformflat-3"),
        creduce_pass!("topformflat-4"),
        creduce_pass!("topformflat-6"),
        creduce_pass!("topformflat-8"),
        creduce_pass!("topformflat-10"),
        creduce_pass!("clang-delta-remove-namespace"),
        creduce_pass!("clang-delta-aggregate-to-scalar"),
        creduce_pass!("clang-delta-local-to-global"),
        creduce_pass!("clang-delta-param-to-global"),
        creduce_pass!("clang-delta-param-to-local"),
        creduce_pass!("clang-delta-remove-nested-function"),
        creduce_pass!("clang-delta-rename-fun"),
        creduce_pass!("clang-delta-union-to-struct"),
        creduce_pass!("clang-delta-rename-param"),
        creduce_pass!("clang-delta-rename-var"),
        creduce_pass!("clang-delta-rename-class"),
        creduce_pass!("clang-delta-rename-cxx-method"),
        creduce_pass!("clang-delta-return-void"),
        creduce_pass!("clang-delta-simple-inliner"),
        creduce_pass!("clang-delta-reduce-pointer-level"),
        creduce_pass!("clang-delta-lift-assignment-expr"),
        creduce_pass!("clang-delta-copy-propagation"),
        creduce_pass!("clang-delta-callexpr-to-value"),
        creduce_pass!("clang-delta-replace-callexpr"),
        creduce_pass!("clang-delta-simplify-callexpr"),
        creduce_pass!("clang-delta-remove-unused-function"),
        creduce_pass!("clang-delta-remove-unused-enum-member"),
        creduce_pass!("clang-delta-remove-enum-member-value"),
        creduce_pass!("clang-delta-simplify-if"),
        creduce_pass!("clang-delta-reduce-array-dim"),
        creduce_pass!("clang-delta-reduce-array-size"),
        creduce_pass!("clang-delta-move-function-body"),
        creduce_pass!("clang-delta-simplify-comma-expr"),
        creduce_pass!("clang-delta-simplify-dependent-typedef"),
        creduce_pass!("clang-delta-replace-simple-typedef"),
        creduce_pass!("clang-delta-replace-dependent-typedef"),
        creduce_pass!("clang-delta-replace-one-level-typedef-type"),
        creduce_pass!("clang-delta-remove-unused-field"),
        creduce_pass!("clang-delta-instantiate-template-type-param-to-int"),
        creduce_pass!("clang-delta-instantiate-template-param"),
        creduce_pass!("clang-delta-template-arg-to-int"),
        creduce_pass!("clang-delta-template-non-type-arg-to-int"),
        creduce_pass!("clang-delta-reduce-class-template-param"),
        creduce_pass!("clang-delta-remove-trivial-base-template"),
        creduce_pass!("clang-delta-class-template-to-class"),
        creduce_pass!("clang-delta-remove-base-class"),
        creduce_pass!("clang-delta-replace-derived-class"),
        creduce_pass!("clang-delta-remove-unresolved-base"),
        creduce_pass!("clang-delta-remove-ctor-initializer"),
        creduce_pass!("clang-delta-replace-class-with-base-template-spec"),
        creduce_pass!("clang-delta-simplify-nested-class"),
        creduce_pass!("clang-delta-remove-unused-outer-class"),
        creduce_pass!("clang-delta-empty-struct-to-int"),
        creduce_pass!("clang-delta-remove-pointer"),
        creduce_pass!("clang-delta-reduce-pointer-pairs"),
        creduce_pass!("clang-delta-remove-array"),
        creduce_pass!("clang-delta-remove-addr-taken"),
        creduce_pass!("clang-delta-simplify-struct"),
        creduce_pass!("clang-delta-replace-undefined-function"),
        creduce_pass!("clang-delta-replace-array-index-var"),
        creduce_pass!("clang-delta-replace-array-access-with-index"),
        creduce_pass!("clang-delta-replace-dependent-name"),
        creduce_pass!("clang-delta-simplify-recursive-template-instantiation"),
        creduce_pass!("clang-delta-vector-to-array"),
        creduce_pass!("clang-delta-combine-global-var"),
        creduce_pass!("clang-delta-combine-local-var"),
        creduce_pass!("clang-delta-simplify-struct-union-decl"),
        creduce_pass!("clang-delta-move-global-var"),
        creduce_pass!("clang-delta-unify-function-decl"),
        creduce_pass!("clang-format"),
        creduce_pass!("clex-delete-string"),
        creduce_pass!("clex-rm-toks-1"),
        creduce_pass!("clex-rm-toks-2"),
        creduce_pass!("clex-rm-toks-3"),
        creduce_pass!("clex-rm-toks-4"),
        creduce_pass!("clex-rm-toks-5"),
        creduce_pass!("clex-rm-toks-6"),
        creduce_pass!("clex-rm-toks-7"),
        creduce_pass!("clex-rm-toks-8"),
        creduce_pass!("clex-rm-toks-9"),
        creduce_pass!("clex-rm-toks-10"),
        creduce_pass!("clex-rm-toks-11"),
        creduce_pass!("clex-rm-toks-12"),
        creduce_pass!("clex-rm-toks-13"),
        creduce_pass!("clex-rm-toks-14"),
        creduce_pass!("clex-rm-toks-15"),
        creduce_pass!("clex-rm-toks-16"),
        creduce_pass!("clex-rename-toks"),
        creduce_pass!("clex-delete-string"),
        creduce_pass!("clex-define"),
    ];

/// An oracle that uses leverages C-Reduce's pass ordering to give priority to
/// reductions generated by a reducer that was ported from a C-Reduce pass.
#[derive(Debug, Default)]
pub struct CreducePassPriorities {
    current_idx: Option<usize>,
}

impl traits::Oracle for CreducePassPriorities {
    fn observe_smallest_interesting(&mut self, smallest: &test_case::Interesting) {
        self.current_idx = CREDUCE_PASSES.iter().position(|p| {
            debug_assert!({
                use std::path::Path;
                let path = Path::new(p);
                path.exists()
            });
            *p == smallest.provenance()
        });
    }

    fn observe_not_smallest_interesting(&mut self, _: &test_case::Interesting) {}
    fn observe_not_interesting(&mut self, _: &test_case::PotentialReduction) {}

    fn predict(&mut self, reduction: &test_case::PotentialReduction) -> Score {
        if let Some(current_idx) = self.current_idx {
            let potential_idx = CREDUCE_PASSES
                .iter()
                .position(|p| *p == reduction.provenance());
            if let Some(potential_idx) = potential_idx {
                if current_idx <= potential_idx {
                    let delta = potential_idx as f64 - current_idx as f64;
                    return Score::new(1.0 / (delta + 1.0));
                }
            }
        }

        Score::new(0.0)
    }
}
