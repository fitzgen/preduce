//! The worker actor pulls potentially interesting test cases from the
//! supervisor and tests them for interestingness.

use super::{Logger, Supervisor};
use either::{Either, Left, Right};
use error;
use git::{self, RepoExt};
use git2;
use std::fmt;
use std::panic;
use std::path;
use std::sync::mpsc;
use std::thread;
use tempdir;
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
    ) -> Self {
        logger.spawning_worker(id);

        let upstream = upstream.into();
        let (sender, receiver) = mpsc::channel();

        let me = Worker {
            id: id,
            sender: sender,
        };
        let me2 = me.clone();

        thread::spawn(
            move || {
                WorkerActor::run(id, me2, predicate, receiver, supervisor, logger, upstream);
            },
        );

        me
    }

    /// Get the id of this worker.
    pub fn id(&self) -> WorkerId {
        self.id
    }

    /// Tell this worker to shutdown.
    pub fn shutdown(self) {
        self.sender.send(WorkerMessage::Shutdown).unwrap();
    }

    /// Send the worker the response to its request for another potential
    /// reduction.
    pub fn next_reduction(&self, reduction: test_case::PotentialReduction) {
        self.sender
            .send(WorkerMessage::NextReduction(reduction))
            .unwrap();
    }

    /// Tell the worker to try and merge the upstream's test case at the given
    /// commit ID into its interesting (but not globally smallest) test case.
    pub fn try_merge(&self, upstream_size: u64, commit_id: git2::Oid) {
        self.sender
            .send(WorkerMessage::TryMerge(upstream_size, commit_id))
            .unwrap();
    }
}

// Worker actor implementation.

struct WorkerActor<'a> {
    id: WorkerId,
    me: Worker,
    predicate: Box<traits::IsInteresting>,
    incoming: mpsc::Receiver<WorkerMessage>,
    supervisor: Supervisor,
    logger: Logger,
    repo: git::TempRepo<'a>,
}

impl<'a> fmt::Debug for WorkerActor<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "WorkerActor")
    }
}

#[derive(Debug)]
struct Test<'a> {
    worker: WorkerActor<'a>,
    reduction: test_case::PotentialReduction,
}

#[derive(Debug)]
struct Interesting<'a> {
    worker: WorkerActor<'a>,
    interesting: test_case::Interesting,
}

#[derive(Debug)]
struct TryMerge<'a> {
    worker: WorkerActor<'a>,
    interesting: test_case::Interesting,
    upstream_size: u64,
    commit_id: git2::Oid,
}

impl<'a> WorkerActor<'a> {
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
                  panic::catch_unwind(
                panic::AssertUnwindSafe(
                    move || {
                        WorkerActor::try_run(
                            id,
                            me,
                            predicate,
                            incoming,
                            supervisor2,
                            logger2,
                            upstream,
                        )
                    },
                ),
            )
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
        let tempdir = tempdir::TempDir::new(&prefix)?;
        let repo = git::TempRepo::clone(&upstream, &tempdir)?;

        let worker = WorkerActor {
            id: id,
            me: me,
            predicate: predicate,
            incoming: incoming,
            supervisor: supervisor,
            logger: logger,
            repo: repo,
        };

        let mut test = match worker.get_next_reduction() {
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
                Right(worker) => {
                    // The test case was judged not interesting; grab a new
                    // potential reduction to test.
                    test = match worker.get_next_reduction() {
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

    fn get_next_reduction(self) -> Option<Test<'a>> {
        self.supervisor.request_next_reduction(self.me.clone());
        match self.incoming.recv().unwrap() {
            WorkerMessage::Shutdown => self.shutdown(),
            WorkerMessage::NextReduction(reduction) => {
                Some(
                    Test {
                        worker: self,
                        reduction: reduction,
                    },
                )
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

impl<'a> Test<'a> {
    fn judge(self) -> error::Result<Either<Interesting<'a>, WorkerActor<'a>>> {
        self.worker
            .logger
            .start_judging_interesting(self.worker.id);
        let maybe_interesting = self.reduction
            .into_interesting(&self.worker.predicate, &self.worker.repo)?;
        if let Some(interesting) = maybe_interesting {
            self.worker
                .logger
                .judged_interesting(self.worker.id, interesting.size());
            Ok(
                Left(
                    Interesting {
                        worker: self.worker,
                        interesting: interesting,
                    },
                ),
            )
        } else {
            self.worker
                .logger
                .judged_not_interesting(self.worker.id);
            Ok(Right(self.worker))
        }
    }
}

impl<'a> Interesting<'a> {
    fn report_to_supervisor(self) -> Either<TryMerge<'a>, Option<Test<'a>>> {
        self.worker
            .supervisor
            .report_interesting(self.worker.me.clone(), self.interesting.clone());

        match self.worker.incoming.recv().unwrap() {
            WorkerMessage::Shutdown => Right(self.worker.shutdown()),
            WorkerMessage::NextReduction(reduction) => {
                Right(
                    Some(
                        Test {
                            worker: self.worker,
                            reduction: reduction,
                        },
                    ),
                )
            }
            WorkerMessage::TryMerge(upstream_size, commit_id) => {
                assert!(
                    upstream_size < self.interesting.size(),
                    "Should only merge if we are not the globally smallest test case"
                );
                Left(
                    TryMerge {
                        worker: self.worker,
                        interesting: self.interesting,
                        upstream_size: upstream_size,
                        commit_id: commit_id,
                    },
                )
            }
        }
    }
}

impl<'a> TryMerge<'a> {
    fn into_test(self) -> error::Result<Option<Test<'a>>> {
        // Should split this out into `into_merged_test`, call that new function
        // here, and then inspect its errors for merge failures and recover from
        // them gracefully? Right now, if there is a merge conflict, we will
        // kill the whole worker and rely on the supervisor to spawn a new one
        // in its stead, which seems fairly heavy for something we expect to
        // happen fairly often.

        let merged;
        {
            let our_commit = self.worker.repo.head_commit()?;

            self.worker.repo.fetch_origin()?;
            let their_commit = self.worker.repo.find_commit(self.commit_id)?;

            self.worker
                .repo
                .merge_commits(&our_commit, &their_commit, None)?;

            merged = test_case::PotentialReduction::new(
                self.interesting,
                "merge",
                self.worker.repo.test_case_path()?,
            )?;

            let msg = format!("merge - {} - {}", merged.size(), merged.path().display());
            self.worker.repo.commit_test_case(&msg)?;
        }

        Ok(
            if merged.size() < self.upstream_size {
                Some(
                    Test {
                        worker: self.worker,
                        reduction: merged,
                    },
                )
            } else {
                None
            },
        )
    }
}
