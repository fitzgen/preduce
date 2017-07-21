//! Determining the priority of potential reductions.

use fixedbitset::FixedBitSet;
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

    fn observe_exhausted(&mut self, _: &str) {}

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

const CREDUCE_PASSES: &'static [&'static str] = &[
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
    creduce_pass!("clex-rm-tok-pattern-4"),
    creduce_pass!("clex-rename-toks"),
    creduce_pass!("clex-delete-string"),
    creduce_pass!("clex-define"),
];

/// An oracle that uses leverages C-Reduce's pass ordering to give priority to
/// reductions generated by a reducer that was ported from a C-Reduce pass.
#[derive(Debug)]
pub struct CreducePassPriorities {
    current_idx: usize,
    exhausted: FixedBitSet,
}

impl Default for CreducePassPriorities {
    fn default() -> CreducePassPriorities {
        CreducePassPriorities {
            current_idx: 0,
            exhausted: FixedBitSet::with_capacity(CREDUCE_PASSES.len()),
        }
    }
}

impl traits::Oracle for CreducePassPriorities {
    fn observe_smallest_interesting(&mut self, _: &test_case::Interesting) {
        self.exhausted.clear();

        debug_assert!(CREDUCE_PASSES.iter().all(|p| {
            use std::path::Path;
            let path = Path::new(p);
            path.exists()
        }));
    }

    fn observe_not_smallest_interesting(&mut self, _: &test_case::Interesting) {}
    fn observe_not_interesting(&mut self, _: &test_case::PotentialReduction) {}

    fn observe_exhausted(&mut self, reducer_name: &str) {
        let idx = CREDUCE_PASSES.iter().position(|p| *p == reducer_name);
        if let Some(idx) = idx {
            self.exhausted.insert(idx);

            if idx == self.current_idx {
                if self.exhausted.ones().count() == CREDUCE_PASSES.len() {
                    self.current_idx = 0;
                } else {
                    loop {
                        self.current_idx = (self.current_idx + 1) % CREDUCE_PASSES.len();
                        if !self.exhausted[self.current_idx] {
                            break;
                        }
                    }
                }
            }
        }
    }

    fn predict(&mut self, reduction: &test_case::PotentialReduction) -> Score {
        let potential_idx = CREDUCE_PASSES
            .iter()
            .position(|p| *p == reduction.provenance());
        if let Some(mut potential_idx) = potential_idx {
            if potential_idx < self.current_idx {
                potential_idx += CREDUCE_PASSES.len();
            }
            let delta = potential_idx as f64 - self.current_idx as f64;
            return Score::new(1.0 / (delta + 1.0));
        }

        Score::new(0.0)
    }
}

/// An `Oracle` that scores potential reductions by how much they were able to
/// shave off of the current smallest test case.
#[derive(Debug)]
pub struct PercentReduced {
    smallest: Option<test_case::Interesting>,
}

impl traits::Oracle for PercentReduced {
    fn observe_smallest_interesting(&mut self, interesting: &test_case::Interesting) {
        self.smallest = Some(interesting.clone());
    }

    fn observe_not_smallest_interesting(&mut self, _: &test_case::Interesting) {}
    fn observe_not_interesting(&mut self, _: &test_case::PotentialReduction) {}
    fn observe_exhausted(&mut self, _: &str) {}

    fn predict(&mut self, reduction: &test_case::PotentialReduction) -> Score {
        if let Some(ref smallest) = self.smallest {
            if reduction.size() <= smallest.size() {
                return Score::new(1.0 - (reduction.size() as f64 / smallest.size() as f64));
            }
        }

        Score::new(0.0)
    }
}

macro_rules! define_join_combinator {
    (
        $name:ident {
            $(
                $inner:ident : $generic:ident ,
            )+
        }
    ) => {
        /// Join multiple `Oracle`s into a single `Oracle` implementation.
        #[derive(Debug)]
        pub struct $name < $( $generic , )+ > {
            $( $inner : $generic , )+
        }

        impl < $( $generic , )+ > $name < $( $generic , )+ > {
            /// Construct a new joined `Oracle` from the given `Oracle`s.
            pub fn new( $( $inner : $generic , )+ ) -> Self {
                $name {
                    $( $inner , )+
                }
            }
        }

        impl < $( $generic , )+ > traits::Oracle for $name < $( $generic , )+ >
            where $( $generic : traits::Oracle , )+
        {
            fn observe_smallest_interesting(&mut self, interesting: &test_case::Interesting) {
                $( self.$inner.observe_smallest_interesting(interesting); )+
            }

            fn observe_not_smallest_interesting(&mut self, interesting: &test_case::Interesting) {
                $( self.$inner.observe_not_smallest_interesting(interesting); )+
            }

            fn observe_not_interesting(&mut self, reduction: &test_case::PotentialReduction) {
                $( self.$inner.observe_not_interesting(reduction); )+
            }

            fn observe_exhausted(&mut self, reducer: &str) {
                $( self.$inner.observe_exhausted(reducer); )+
            }

            fn predict(&mut self, reduction: &test_case::PotentialReduction) -> Score {
                Score::new(0.0 $( + f64::from(self.$inner.predict(reduction)) )+ )
            }
        }
    }
}

define_join_combinator! {
    Join2 {
        oracle1: T,
        oracle2: U,
    }
}
define_join_combinator! {
    Join3 {
        oracle1: T,
        oracle2: U,
        oracle3: V,
    }
}
define_join_combinator! {
    Join4 {
        oracle1: T,
        oracle2: U,
        oracle3: V,
        oracle4: W,
    }
}
define_join_combinator! {
    Join5 {
        oracle1: T,
        oracle2: U,
        oracle3: V,
        oracle4: W,
        oracle5: X,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use score::Score;
    use test_case;
    use traits::Oracle;

    struct Constant(Score);

    impl traits::Oracle for Constant {
        fn observe_smallest_interesting(&mut self, _: &test_case::Interesting) {}
        fn observe_not_smallest_interesting(&mut self, _: &test_case::Interesting) {}
        fn observe_not_interesting(&mut self, _: &test_case::PotentialReduction) {}
        fn observe_exhausted(&mut self, _: &str) {}
        fn predict(&mut self, _: &test_case::PotentialReduction) -> Score {
            self.0
        }
    }

    #[test]
    fn test_joining_oracles() {
        let mut joined = Join3::new(
            Constant(Score::new(1.0)),
            Constant(Score::new(2.0)),
            Constant(Score::new(3.0)),
        );
        let reduction = test_case::PotentialReduction::testing_only_new();
        assert_eq!(joined.predict(&reduction), Score::new(6.0));
    }
}
