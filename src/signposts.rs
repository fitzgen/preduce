#[cfg(feature = "signpost")]
extern crate signpost;

macro_rules! define_signpost {
    ( $name:ident , $code:expr ) => {
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

define_signpost!(SupervisorHandleInteresting, 100);
define_signpost!(SupervisorNextReduction,     101);
define_signpost!(SupervisorShutdown,          102);
define_signpost!(SupervisorRunLoop,           103);

define_signpost!(WorkerGetNextReduction,      200);
define_signpost!(WorkerJudgeInteresting,      201);
define_signpost!(WorkerReportInteresting,     202);
define_signpost!(WorkerTryMerging,            203);
