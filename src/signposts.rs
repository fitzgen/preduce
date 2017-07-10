#[cfg(feature = "signpost")]
extern crate signpost;

macro_rules! define_signpost {
    ( $code:expr, $name:ident ) => {
        #[cfg(feature = "signpost")]
        pub struct $name(self::signpost::AutoTrace<'static>);

        #[cfg(feature = "signpost")]
        impl $name {
            pub fn new() -> Self {
                static ARGS: &'static [usize; 4] = &[0, 0, 0, $code];
                $name(self::signpost::AutoTrace::new($code, ARGS))
            }
        }

        #[cfg(not(feature = "signpost"))]
        pub struct $name;

        #[cfg(not(feature = "signpost"))]
        impl $name {
            #[inline(always)]
            pub fn new() -> Self {
                $name
            }
        }
    }
}

define_signpost!(100, SupervisorHandleInteresting);
define_signpost!(101, SupervisorShutdown);
define_signpost!(102, SupervisorRunLoop);

define_signpost!(200, WorkerGetNextReduction);
define_signpost!(201, WorkerJudgeInteresting);
define_signpost!(202, WorkerReportInteresting);
define_signpost!(203, WorkerTryMerging);

define_signpost!(300, ReducerNextReduction);
