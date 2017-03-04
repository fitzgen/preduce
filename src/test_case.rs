//! Types related to test cases, their interestingness, and potential reductions
//! of them.

use error;
use git::RepoExt;
use git2;
use std::fs;
use std::path;
use traits;

/// Methods common to all test cases.
pub trait TestCaseMethods {
    /// Get the path to this test case.
    fn path(&self) -> &path::Path;

    /// Get the size (in bytes) of this test case.
    fn size(&self) -> u64;
}

/// A test case with potential: it may or may not be smaller than our smallest
/// interesting test case, and it may or may not be interesting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PotentialReduction {
    /// From which reducer did this potential reduction came from?
    provenance: String,

    /// The commit id of the seed test case from which this potential reduction
    /// was generated.
    parent: git2::Oid,

    /// The path to the potentially reduced test case file.
    path: path::PathBuf,

    /// The size of the file.
    size: u64,
}

impl TestCaseMethods for PotentialReduction {
    fn path(&self) -> &path::Path {
        &self.path
    }

    fn size(&self) -> u64 {
        self.size
    }
}

impl PotentialReduction {
    /// Construct a new potential reduction.
    ///
    /// The `seed` must be the interesting test case from which a reducer
    /// produced this potential reduction.
    ///
    /// The `provenance` must be a diagnostic describing the reducer that
    /// produced this potential reduction.
    ///
    /// The `test_case` must be the file path of the potential reduction's test
    /// case.
    pub fn new<S, P>(seed: Interesting,
                     provenance: S,
                     test_case: P)
                     -> error::Result<PotentialReduction>
        where S: Into<String>,
              P: AsRef<path::Path>
    {
        let provenance = provenance.into();
        assert!(!provenance.is_empty());

        let path = test_case.as_ref().canonicalize()?;
        assert!(path.is_absolute());
        assert!(path.is_file());

        let size = fs::metadata(&path)?.len();

        Ok(PotentialReduction {
            provenance: provenance,
            parent: seed.commit_id(),
            path: path,
            size: size,
        })
    }

    fn make_commit_message(&self) -> String {
        format!("{} - {} - {}",
                self.provenance,
                self.size(),
                self.path().display())
    }

    /// Try and convert this *potential* reduction into an *interesting* test
    /// case by validating whether it is interesting or not using the given
    /// `judge`.
    pub fn into_interesting<I>(mut self,
                               judge: &I,
                               repo: &git2::Repository)
                               -> error::Result<Option<Interesting>>
        where I: traits::IsInteresting
    {
        assert_eq!(repo.state(), git2::RepositoryState::Clean);

        if !judge.is_interesting(self.path())? {
            return Ok(None);
        }

        let repo_test_case_path = repo.test_case_path()?;
        fs::copy(self.path(), &repo_test_case_path)?;
        self.path = repo_test_case_path;

        let msg = self.make_commit_message();
        let commit_id = repo.commit_test_case(&msg)?;

        Ok(Some(Interesting {
            kind: InterestingKind::Reduction(self),
            commit_id: commit_id,
        }))
    }
}

/// A test case that has been verified to be interesting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interesting {
    /// The kind of interesting test case.
    kind: InterestingKind,

    /// The commit id for this test case.
    commit_id: git2::Oid,
}

impl TestCaseMethods for Interesting {
    fn path(&self) -> &path::Path {
        self.kind.path()
    }

    fn size(&self) -> u64 {
        self.kind.size()
    }
}

impl Interesting {
    /// Construct the initial interesting test case with the given `judge` of
    /// whether a test case is interesting or not.
    pub fn initial<P, I>(file_path: P,
                         judge: &I,
                         repo: &git2::Repository)
                         -> error::Result<Option<Interesting>>
        where P: AsRef<path::Path>,
              I: traits::IsInteresting
    {
        assert_eq!(repo.state(), git2::RepositoryState::Clean);

        if !judge.is_interesting(file_path.as_ref())? {
            return Ok(None);
        }

        let size = fs::metadata(&file_path)?.len();
        let repo_test_case_path = repo.test_case_path()?;

        fs::copy(file_path.as_ref(), &repo_test_case_path)?;

        let msg = format!("Initial - {} - {}", size, file_path.as_ref().display());
        let commit_id = repo.commit_test_case(&msg)?;

        Ok(Some(Interesting {
            kind: InterestingKind::Initial(InitialInteresting {
                path: repo_test_case_path,
                size: size,
            }),
            commit_id: commit_id,
        }))
    }

    /// Get the commit id of this interesting test case.
    pub fn commit_id(&self) -> git2::Oid {
        self.commit_id
    }
}

/// An enumeration of the kinds of interesting test cases.
#[derive(Clone, Debug, PartialEq, Eq)]
enum InterestingKind {
    /// The initial interesting test case.
    Initial(InitialInteresting),

    /// A potential reduction of the initial test case that has been found to be
    /// interesting.
    Reduction(PotentialReduction),
}

impl TestCaseMethods for InterestingKind {
    fn path(&self) -> &path::Path {
        match *self {
            InterestingKind::Initial(ref initial) => initial.path(),
            InterestingKind::Reduction(ref reduction) => reduction.path(),
        }
    }

    fn size(&self) -> u64 {
        match *self {
            InterestingKind::Initial(ref initial) => initial.size(),
            InterestingKind::Reduction(ref reduction) => reduction.size(),
        }
    }
}

/// The initial test case, after it has been validated to have passed the
/// is-interesting test.
#[derive(Clone, Debug, PartialEq, Eq)]
struct InitialInteresting {
    /// The path to the initial test case file.
    path: path::PathBuf,

    /// The size of the file.
    size: u64,
}

impl TestCaseMethods for InitialInteresting {
    fn path(&self) -> &path::Path {
        &self.path
    }

    fn size(&self) -> u64 {
        self.size
    }
}

#[cfg(test)]
impl PotentialReduction {
    pub fn testing_only_new<P>(path: P) -> PotentialReduction
        where P: AsRef<path::Path>
    {
        PotentialReduction {
            provenance: "PotentialReduction::testing_only_new".into(),
            parent: git2::Oid::from_bytes(&[0; 20]).unwrap(),
            path: path.as_ref().into(),
            size: 0,
        }
    }
}

#[cfg(test)]
impl Interesting {
    pub fn testing_only_new<P>(path: P) -> Interesting
        where P: AsRef<path::Path>
    {
        Interesting {
            kind: InterestingKind::Initial(InitialInteresting {
                path: path.as_ref().into(),
                size: 0,
            }),
            commit_id: git2::Oid::from_bytes(&[0; 20]).unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git::RepoExt;
    use std::fs;
    use std::io::{Read, Write};
    use std::path;
    use tempdir;
    use test_utils;

    #[test]
    fn interesting_initial_true() {
        let dir = tempdir::TempDir::new("into_interesting").unwrap();
        let repo = test_utils::TestRepo::new(&dir);

        let path = dir.path().join("initial");
        {
            let mut initial_file = fs::File::create(&path).unwrap();
            writeln!(&mut initial_file, "la la la la la").unwrap();
        }

        let judge = |_: &path::Path| Ok(true);

        let interesting = Interesting::initial(path, &judge, &repo)
            .expect("should not error")
            .expect("and should find the initial test case interesting");

        assert_eq!(interesting.path(),
                   repo.test_case_path().unwrap(),
                   "The repo path should become the canonical test case path");

        let mut file = fs::File::open(interesting.path())
            .expect("The repo test case path should have a file");

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("And we should read from that file");
        assert_eq!(contents,
                   "la la la la la\n",
                   "And it should have the expected contents");

        assert_eq!(interesting.size(),
                   contents.len() as _,
                   "And the test case should have the expected size");
    }

    #[test]
    fn interesting_initial_false() {
        let dir = tempdir::TempDir::new("into_interesting").unwrap();
        let repo = test_utils::TestRepo::new(&dir);
        let path = dir.path().join("initial");
        let judge = |_: &path::Path| Ok(false);
        let interesting = Interesting::initial(path, &judge, &repo).expect("should not error");

        assert_eq!(interesting, None);
    }

    #[test]
    fn interesting_initial_error() {
        let dir = tempdir::TempDir::new("into_interesting").unwrap();
        let repo = test_utils::TestRepo::new(&dir);
        let path = dir.path().join("initial");
        let judge = |_: &path::Path| Err(error::Error::Git(git2::Error::from_str("woops")));
        let result = Interesting::initial(path, &judge, &repo);
        assert!(result.is_err());
    }


    #[test]
    fn into_interesting() {
        let dir = tempdir::TempDir::new("into_interesting").unwrap();
        let repo = test_utils::TestRepo::new(&dir);

        let initial_path = dir.path().join("initial");
        {
            let mut initial_file = fs::File::create(&initial_path).unwrap();
            writeln!(&mut initial_file, "la la la la la").unwrap();
        }

        let judge = |_: &path::Path| Ok(true);

        let interesting = Interesting::initial(initial_path, &judge, &repo)
            .expect("interesting should be ok")
            .expect("interesting should be some");

        let reduction_path = dir.path().join("reduction");
        {
            let mut reduction_file = fs::File::create(&reduction_path).unwrap();
            writeln!(&mut reduction_file, "la").unwrap();
        }

        let reduction = PotentialReduction::new(interesting, "test", reduction_path)
            .expect("should create potenetial reduction");

        let interesting_reduction = reduction.into_interesting(&judge, &repo)
            .expect("interesting reduction should be ok")
            .expect("interesting reduction should be some");

        assert_eq!(interesting_reduction.path(),
                   repo.test_case_path().unwrap(),
                   "The interesting reduction's path should be the repo test case path");

        let mut file = fs::File::open(interesting_reduction.path())
            .expect("The repo test case path should have a file");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("And we should read from that file");
        assert_eq!(contents, "la\n", "And it should have the expected contents");

        assert_eq!(interesting_reduction.size(),
                   contents.len() as _,
                   "And the test case should have the expected size");
    }
}
