extern crate preduce_reducer_script;

use preduce_reducer_script::{run_clang_delta, ClangDelta};

struct ReduceClassTemplateParam;

impl ClangDelta for ReduceClassTemplateParam {
    fn transformation() -> &'static str {
        "reduce-class-template-param"
    }
}

fn main() {
    run_clang_delta::<ReduceClassTemplateParam>()
}
