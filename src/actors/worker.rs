//! The worker actor pulls potentially interesting test cases from the
//! supervisor and tests them for interestingness.

use super::{Logger, Supervisor};
use either::{Either, Left, Right};
use error;
use git::{self, RepoExt};
use git2;
use signposts;
use std::fmt;
use std::fs;
use std::panic;
use std::path;
use std::sync::mpsc;
use std::thread;
use test_case::{self, TestCaseMethods};
use traits;

/// An identifier for a worker actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkerId(usize);

impl WorkerId {
    pub fn new(id: usize) -> WorkerId {
        WorkerId(id)
    }
}

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Messages that can be sent to worker actors.
#[derive(Debug)]
enum WorkerMessage {
    NextReduction(test_case::PotentialReduction),
    Shutdown,
    TryMerge(u64, git2::Oid),
}

/// A client handle to a worker actor.
#[derive(Clone, Debug)]
pub struct Worker {
    id: WorkerId,
    sender: mpsc::Sender<WorkerMessage>,
}

/// Worker client API.
impl Worker {
    /// Spawn a new worker actor.
    pub fn spawn(
        id: WorkerId,
        predicate: Box<traits::IsInteresting>,
        supervisor: Supervisor,
        logger: Logger,
        upstream: &path::Path,
    ) -> error::Result<Worker> {
        logger.spawning_worker(id);

        let upstream = upstream.into();
        let (sender, receiver) = mpsc::channel();

        let me = Worker {
            id: id,
            sender: sender,
        };
        let me2 = me.clone();

        thread::Builder::new()
            .name(format!("preduce-worker-{}", id))
            .spawn(move || {
                WorkerActor::run(id, me2, predicate, receiver, supervisor, logger, upstream);
            })?;

        Ok(me)
    }

    /// Get the id of this worker.
    pub fn id(&self) -> WorkerId {
        self.id
    }

    // For communication with this worker from the supervisor, don't unwrap the
    // mpsc sends. Instead of panicking the supervisor, let the catch_unwind'ing
    // of the worker inform the supervisor of a worker's early, unexpected
    // demise.

    /// Tell this worker to shutdown.
    pub fn shutdown(self) {
        let _ = self.sender.send(WorkerMessage::Shutdown);
    }

    /// Send the worker the response to its request for another potential
    /// reduction.
    pub fn next_reduction(&self, reduction: test_case::PotentialReduction) {
        let _ = self.sender.send(WorkerMessage::NextReduction(reduction));
    }

    /// Tell the worker to try and merge the upstream's test case at the given
    /// commit ID into its interesting (but not globally smallest) test case.
    pub fn try_merge(&self, upstream_size: u64, commit_id: git2::Oid) {
        let _ = self.sender
            .send(WorkerMessage::TryMerge(upstream_size, commit_id));
    }
}

// Worker actor implementation.

struct WorkerActor {
    id: WorkerId,
    me: Worker,
    predicate: Box<traits::IsInteresting>,
    incoming: mpsc::Receiver<WorkerMessage>,
    supervisor: Supervisor,
    logger: Logger,
    repo: git::TempRepo,
    tests_since_gc: usize,
}

impl fmt::Debug for WorkerActor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "WorkerActor")
    }
}

#[derive(Debug)]
struct Test {
    worker: WorkerActor,
    reduction: test_case::PotentialReduction,
}

#[derive(Debug)]
struct Interesting {
    worker: WorkerActor,
    interesting: test_case::Interesting,
}

#[derive(Debug)]
struct TryMerge {
    worker: WorkerActor,
    interesting: test_case::Interesting,
    upstream_size: u64,
    commit_id: git2::Oid,
}

impl WorkerActor {
    fn run(
        id: WorkerId,
        me: Worker,
        predicate: Box<traits::IsInteresting>,
        incoming: mpsc::Receiver<WorkerMessage>,
        supervisor: Supervisor,
        logger: Logger,
        upstream: path::PathBuf,
    ) {
        match {
            let supervisor2 = supervisor.clone();
            let logger2 = logger.clone();
            panic::catch_unwind(panic::AssertUnwindSafe(move || {
                WorkerActor::try_run(id, me, predicate, incoming, supervisor2, logger2, upstream)
            }))
        } {
            Err(p) => {
                supervisor.worker_panicked(id, p);
            }
            Ok(Err(e)) => {
                supervisor.worker_errored(id, e);
            }
            Ok(Ok(())) => {}
        }
    }

    fn try_run(
        id: WorkerId,
        me: Worker,
        predicate: Box<traits::IsInteresting>,
        incoming: mpsc::Receiver<WorkerMessage>,
        supervisor: Supervisor,
        logger: Logger,
        upstream: path::PathBuf,
    ) -> error::Result<()> {
        logger.spawned_worker(id);

        let prefix = format!("preduce-worker-{}", id);
        let repo = git::TempRepo::clone(&upstream, &prefix)?;

        let worker = WorkerActor {
            id: id,
            me: me,
            predicate: predicate,
            incoming: incoming,
            supervisor: supervisor,
            logger: logger,
            repo: repo,
            tests_since_gc: 0,
        };

        let mut test = match worker.get_next_reduction(None) {
            Some(test) => test,
            None => return Ok(()),
        };

        loop {
            match test.judge()? {
                Left(interesting) => {
                    // The test case was judged interesting -- tell the
                    // supervisor!
                    match interesting.report_to_supervisor() {
                        Right(None) => {
                            // That interesting test case became the new
                            // globally smallest interesting test case, but when
                            // we tried to reduce it further, we found it was
                            // already so minimal that the reducer couldn't
                            // produce any potential reductions. Time to
                            // shutdown.
                            return Ok(());
                        }
                        Right(Some(new_test)) => {
                            // That interesting test case became the new
                            // globally smallest interesting test case, and this
                            // is a new reduction based on it for us to test.
                            test = new_test;
                        }
                        Left(try_merge) => {
                            // This test case was not the new smallest, but is
                            // worth trying to merge with the current global
                            // smallest, and retesting it to see if that merged
                            // test case becomes the new smallest.
                            test = match try_merge.into_test()? {
                                Some(test) => test,
                                None => return Ok(()),
                            };
                        }
                    }
                }
                Right((worker, not_interesting)) => {
                    // The test case was judged not interesting; grab a new
                    // potential reduction to test.
                    test = match worker.get_next_reduction(Some(not_interesting)) {
                        Some(test) => test,
                        None => return Ok(()),
                    };
                }
            }
        }
    }

    fn shutdown<T>(self) -> Option<T> {
        self.logger.shutdown_worker(self.id);
        None
    }

    fn get_next_reduction(
        self,
        not_interesting: Option<test_case::PotentialReduction>,
    ) -> Option<Test> {
        let _signpost = signposts::WorkerGetNextReduction::new();

        self.supervisor
            .request_next_reduction(self.me.clone(), not_interesting);
        match self.incoming.recv().unwrap() {
            WorkerMessage::Shutdown => self.shutdown(),
            WorkerMessage::NextReduction(reduction) => {
                Some(Test {
                    worker: self,
                    reduction: reduction,
                })
            }
            otherwise => {
                panic!(
                    "Unexpected response to next-reduction request: {:?}",
                    otherwise
                );
            }
        }
    }
}

impl Test {
    fn judge(
        mut self,
    ) -> error::Result<Either<Interesting, (WorkerActor, test_case::PotentialReduction)>> {
        let _signpost = signposts::WorkerJudgeInteresting::new();

        {
            if self.worker.tests_since_gc > 20 {
                self.worker.repo.gc()?;
                self.worker.tests_since_gc = 0;
            }
            self.worker.tests_since_gc += 1;
            self.worker.repo.fetch_origin()?;
            let object = self.worker
                .repo
                .find_object(self.reduction.parent(), Some(git2::ObjectType::Commit))?;
            self.worker.repo.reset(
                &object,
                git2::ResetType::Hard,
                Some(git2::build::CheckoutBuilder::new().force()),
            )?;
        }

        self.worker.logger.start_judging_interesting(self.worker.id);
        match self.reduction
            .into_interesting(&self.worker.predicate, &self.worker.repo)? {
            Left(interesting) => {
                self.worker
                    .logger
                    .judged_interesting(self.worker.id, interesting.size());
                Ok(Left(Interesting {
                    worker: self.worker,
                    interesting: interesting,
                }))
            }
            Right(not_interesting) => {
                let provenance = not_interesting.provenance().into();
                self.worker
                    .logger
                    .judged_not_interesting(self.worker.id, provenance);
                Ok(Right((self.worker, not_interesting)))
            }
        }
    }
}

impl Interesting {
    fn report_to_supervisor(self) -> Either<TryMerge, Option<Test>> {
        let _signpost = signposts::WorkerReportInteresting::new();

        self.worker.supervisor.report_interesting(
            self.worker.me.clone(),
            self.worker.repo.path().into(),
            self.interesting.clone(),
        );

        match self.worker.incoming.recv().unwrap() {
            WorkerMessage::Shutdown => Right(self.worker.shutdown()),
            WorkerMessage::NextReduction(reduction) => {
                Right(Some(Test {
                    worker: self.worker,
                    reduction: reduction,
                }))
            }
            WorkerMessage::TryMerge(upstream_size, commit_id) => {
                assert!(
                    upstream_size <= self.interesting.size(),
                    "Should only merge if we are not the new globally smallest test case"
                );
                Left(TryMerge {
                    worker: self.worker,
                    interesting: self.interesting,
                    upstream_size: upstream_size,
                    commit_id: commit_id,
                })
            }
        }
    }
}

impl TryMerge {
    fn try_merge(self) -> error::Result<Either<Test, WorkerActor>> {
        self.worker
            .logger
            .try_merging(self.worker.id, self.commit_id, self.interesting.commit_id());

        let our_commit = self.worker.repo.head_id()?;

        self.worker.repo.fetch_origin()?;
        let their_commit = self.commit_id;

        assert!(our_commit != their_commit);
        if self.worker
            .repo
            .merge_and_commit(our_commit, their_commit)?
            .is_none()
        {
            // Merging conflicted; move along.
            return Ok(Right(self.worker));
        }

        let merged_file = test_case::TempFile::anonymous()?;
        fs::copy(self.worker.repo.test_case_path()?, merged_file.path())?;

        let merged =
            test_case::PotentialReduction::new(self.interesting.clone(), "merge", merged_file)?;

        self.worker
            .logger
            .finished_merging(self.worker.id, merged.size(), self.upstream_size);

        Ok(if merged.size() < self.upstream_size {
            Left(Test {
                worker: self.worker,
                reduction: merged,
            })
        } else {
            Right(self.worker)
        })
    }

    fn into_test(self) -> error::Result<Option<Test>> {
        let _signpost = signposts::WorkerTryMerging::new();

        debug_assert!({
            self.worker.repo.head_id()? == self.interesting.commit_id()
        });

        // Merges are only useful when we lose the race to become the new
        // smallest interesting test case to another worker that produces a
        // smaller interesting test case than we did. In such a situation, the
        // race-winner's commit is not in our repository, only upstream.
        // Therefore, if we already have the commit in our repository *before*
        // we fetch, then we can't be in the race scenario where merging makes
        // sense.
        if self.worker.repo.find_commit(self.commit_id).is_ok() {
            return Ok(self.worker.get_next_reduction(None));
        }

        Ok(match self.try_merge()? {
            Left(merged) => Some(merged),
            Right(worker) => worker.get_next_reduction(None),
        })
    }
}
