//! Types related to test cases, their interestingness, and potential reductions
//! of them.

use either::{Either, Left, Right};
use error;
use std::fs;
use std::io;
use std::path;
use std::sync::Arc;
use tempdir;
use traits;

/// Methods common to all test cases.
pub trait TestCaseMethods: Into<TempFile> {
    /// Get the path to this test case.
    fn path(&self) -> &path::Path;

    /// Get the size (in bytes) of this test case.
    fn size(&self) -> u64;

    /// Get the delta size (in bytes) of this test case, compared to its parent
    /// test case that it was produced from. Or, in the case of the initial
    /// interesting test case, 0.
    fn delta(&self) -> u64;

    /// Get the provenance of this test case.
    fn provenance(&self) -> &str;
}

#[derive(Debug, Clone)]
struct TempFileInner {
    /// The test case file itself. Stored as an absolute path internally.
    file_path: path::PathBuf,

    /// The temporary directory that this test case file is within.
    ///
    /// Invariant: the `file_path` is always contained within this `dir`!
    dir: Arc<tempdir::TempDir>,
}

impl PartialEq for TempFileInner {
    fn eq(&self, rhs: &TempFileInner) -> bool {
        self.file_path == rhs.file_path
    }
}

impl Eq for TempFileInner {}

/// An immutable, temporary file within a temporary directory.
///
/// When generating reductions, we use these immutable, persistent, temporary
/// files, that are automatically cleaned up once they're no longer in use.
///
/// These temporary files and directories are atomically reference counted.
/// There are no cycles because of both the lack of internal `RefCell`s to
/// enable a cycle's construction, and because the underlying directories and
/// files have no outgoing edges which could become back-edges. We are strictly
/// dealing with a DAG, and therefore don't have to worry about leaking cycles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempFile {
    inner: Arc<TempFileInner>,
}

impl TempFile {
    /// Construct a new temporary file within the given temporary directory.
    ///
    /// The `file_path` must be a path relative to the temporary directory's
    /// path, and this function will panic if that is not the case.
    pub fn new<P>(dir: Arc<tempdir::TempDir>, file_path: P) -> error::Result<TempFile>
    where
        P: Into<path::PathBuf>,
    {
        let file_path = file_path.into();
        assert!(
            file_path.is_relative(),
            "The given file_path should be relative to the temporary directory"
        );

        let file_path = dir.path().canonicalize()?.join(file_path);
        Ok(TempFile {
            inner: Arc::new(TempFileInner {
                file_path: file_path,
                dir: dir,
            }),
        })
    }

    /// Create a new anonymous temporary file in a new temporary directory.
    pub fn anonymous() -> error::Result<TempFile> {
        let dir = Arc::new(tempdir::TempDir::new("preduce-anonymous")?);
        TempFile::new(dir, "preduce-anonymous-temp-file")
    }

    /// Get the path to this temporary file.
    pub fn path(&self) -> &path::Path {
        assert!(self.inner.file_path.is_absolute());
        &self.inner.file_path
    }
}

impl From<PotentialReduction> for TempFile {
    fn from(reduction: PotentialReduction) -> TempFile {
        reduction.test_case
    }
}

impl From<Interesting> for TempFile {
    fn from(interesting: Interesting) -> TempFile {
        interesting.kind.into()
    }
}

impl From<InterestingKind> for TempFile {
    fn from(kind: InterestingKind) -> TempFile {
        match kind {
            InterestingKind::Initial(i) => i.into(),
            InterestingKind::Reduction(r) => r.into(),
        }
    }
}

impl From<InitialInteresting> for TempFile {
    fn from(initial: InitialInteresting) -> TempFile {
        initial.test_case
    }
}

/// A test case with potential: it may or may not be smaller than our smallest
/// interesting test case, and it may or may not be interesting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PotentialReduction {
    /// From which reducer did this potential reduction came from?
    provenance: String,

    /// The temporary file containing the reduced test case.
    test_case: TempFile,

    /// The size of the test case file, in bytes.
    size: u64,

    /// The delta size from the parent test case.
    delta: u64,
}

impl TestCaseMethods for PotentialReduction {
    fn path(&self) -> &path::Path {
        &self.test_case.path()
    }

    fn size(&self) -> u64 {
        self.size
    }

    fn delta(&self) -> u64 {
        self.delta
    }

    fn provenance(&self) -> &str {
        &self.provenance
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
    pub fn new<S, T>(
        seed: Interesting,
        provenance: S,
        test_case: T,
    ) -> error::Result<PotentialReduction>
    where
        S: Into<String>,
        T: Into<TempFile>,
    {
        let provenance = provenance.into();
        assert!(!provenance.is_empty());

        let test_case = test_case.into();
        let size = fs::metadata(test_case.path())?.len();

        Ok(PotentialReduction {
            provenance: provenance,
            test_case: test_case,
            size: size,
            delta: seed.size().saturating_sub(size),
        })
    }

    /// Try and convert this *potential* reduction into an *interesting* test
    /// case by validating whether it is interesting or not using the given
    /// `judge`.
    pub fn into_interesting<I>(
        self,
        judge: &I,
    ) -> error::Result<Either<Interesting, PotentialReduction>>
    where
        I: ?Sized + traits::IsInteresting,
    {
        assert!(self.path().is_file());

        if !judge.is_interesting(self.path())? {
            return Ok(Right(self));
        }

        Ok(Left(Interesting {
            kind: InterestingKind::Reduction(self),
        }))
    }
}

/// A test case that has been verified to be interesting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Interesting {
    /// The kind of interesting test case.
    kind: InterestingKind,
}

impl TestCaseMethods for Interesting {
    fn path(&self) -> &path::Path {
        self.kind.path()
    }

    fn size(&self) -> u64 {
        self.kind.size()
    }

    fn delta(&self) -> u64 {
        self.kind.delta()
    }

    fn provenance(&self) -> &str {
        self.kind.provenance()
    }
}

impl Interesting {
    /// Construct the initial interesting test case with the given `judge` of
    /// whether a test case is interesting or not.
    pub fn initial<P, I>(file_path: P, judge: &I) -> error::Result<Option<Interesting>>
    where
        P: AsRef<path::Path>,
        I: traits::IsInteresting,
    {
        // Create a new immutable temp file for seeding reducers with the
        // initial test case.
        let dir = Arc::new(tempdir::TempDir::new("preduce-initial")?);
        let file_name = path::PathBuf::from(file_path.as_ref().file_name().ok_or(
            error::Error::Io(io::Error::new(
                io::ErrorKind::Other,
                "Initial test case must be a file",
            )),
        )?);
        let temp_file = TempFile::new(dir, file_name)?;
        fs::copy(file_path.as_ref(), temp_file.path())?;

        if !judge.is_interesting(temp_file.path())? {
            return Ok(None);
        }

        let size = fs::metadata(temp_file.path())?.len();

        Ok(Some(Interesting {
            kind: InterestingKind::Initial(InitialInteresting {
                test_case: temp_file,
                size: size,
            }),
        }))
    }
}

/// An enumeration of the kinds of interesting test cases.
#[derive(Clone, Debug, Eq, PartialEq)]
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

    fn delta(&self) -> u64 {
        match *self {
            InterestingKind::Initial(ref initial) => initial.delta(),
            InterestingKind::Reduction(ref reduction) => reduction.delta(),
        }
    }

    fn provenance(&self) -> &str {
        match *self {
            InterestingKind::Initial(ref i) => i.provenance(),
            InterestingKind::Reduction(ref r) => r.provenance(),
        }
    }
}

/// The initial test case, after it has been validated to have passed the
/// is-interesting test.
#[derive(Clone, Debug, Eq, PartialEq)]
struct InitialInteresting {
    /// The path to the initial test case file.
    test_case: TempFile,

    /// The size of the file.
    size: u64,
}

impl TestCaseMethods for InitialInteresting {
    fn path(&self) -> &path::Path {
        self.test_case.path()
    }

    fn size(&self) -> u64 {
        self.size
    }

    fn delta(&self) -> u64 {
        0
    }

    fn provenance(&self) -> &str {
        "<initial>"
    }
}

#[cfg(test)]
impl PotentialReduction {
    pub fn testing_only_new() -> PotentialReduction {
        PotentialReduction {
            provenance: "PotentialReduction::testing_only_new".into(),
            test_case: TempFile::anonymous().unwrap(),
            size: 0,
            delta: 0,
        }
    }
}

#[cfg(test)]
impl Interesting {
    pub fn testing_only_new() -> Interesting {
        Interesting {
            kind: InterestingKind::Initial(InitialInteresting {
                test_case: TempFile::anonymous().unwrap(),
                size: 0,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Read, Write};
    use std::path;
    use tempdir::TempDir;

    #[test]
    fn interesting_initial_true() {
        let dir = TempDir::new("interesting_initial_true").expect("should create temp dir");

        let mut path = path::PathBuf::from(dir.path());
        path.pop();
        path.push("initial");
        {
            let mut initial_file = fs::File::create(&path).unwrap();
            writeln!(&mut initial_file, "la la la la la").unwrap();
        }

        let judge = |_: &path::Path| Ok(true);
        let judge = &judge;

        let interesting = Interesting::initial(&path, &judge)
            .expect("should not error")
            .expect("and should find the initial test case interesting");

        assert!(
            interesting.path() != &path,
            "the initial file should be copied into a temp file once found interesting"
        );
        assert!(
            interesting.path().is_file(),
            "The dir path should have a file now"
        );

        let mut file =
            fs::File::open(interesting.path()).expect("The dir test case file should open");

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("And we should read from that file");
        assert_eq!(
            contents,
            "la la la la la\n",
            "And it should have the expected contents"
        );

        assert_eq!(
            interesting.size(),
            contents.len() as _,
            "And the test case should have the expected size"
        );
    }

    #[test]
    fn interesting_initial_false() {
        let temp_file = TempFile::anonymous().unwrap();
        fs::File::create(temp_file.path()).unwrap();

        let judge = |_: &path::Path| Ok(false);
        let judge = &judge;

        let interesting = Interesting::initial(temp_file.path(), &judge).expect("should not error");
        assert!(interesting.is_none());
    }

    #[test]
    fn interesting_initial_error() {
        let temp_file = TempFile::anonymous().unwrap();
        fs::File::create(temp_file.path()).unwrap();

        let judge = |_: &path::Path| Err(error::Error::InitialTestCaseNotInteresting);
        let judge = &judge;

        let result = Interesting::initial(temp_file.path(), &judge);
        assert!(result.is_err());
    }


    #[test]
    fn into_interesting() {
        let dir = TempDir::new("into_interesting").expect("should create temp dir");

        let mut initial_path = path::PathBuf::from(dir.path());
        initial_path.pop();
        initial_path.push("initial");
        {
            let mut initial_file = fs::File::create(&initial_path).unwrap();
            writeln!(&mut initial_file, "la la la la la").unwrap();
        }

        let judge = |_: &path::Path| Ok(true);
        let judge = &judge;

        let interesting = Interesting::initial(initial_path, &judge)
            .expect("interesting should be ok")
            .expect("interesting should be some");

        let reduction = PotentialReduction::testing_only_new();
        {
            let mut reduction_file = fs::File::create(reduction.path()).unwrap();
            writeln!(&mut reduction_file, "la").unwrap();
        }

        let reduction = PotentialReduction::new(interesting, "test", reduction)
            .expect("should create potenetial reduction");

        let interesting_reduction = reduction
            .clone()
            .into_interesting(&judge)
            .expect("interesting reduction should be ok")
            .left()
            .expect("interesting reduction should be some");

        assert_eq!(
            interesting_reduction.path(),
            reduction.path(),
            "The interesting reduction's path should be the same as the potential reduction's path"
        );

        assert!(
            interesting_reduction.path().is_file(),
            "The interesting reduction's path should have a file"
        );

        let mut file = fs::File::open(&interesting_reduction.path())
            .expect("The interesting reduction path should have a file");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("And we should read from that file");
        assert_eq!(contents, "la\n", "And it should have the expected contents");

        assert_eq!(
            interesting_reduction.size(),
            contents.len() as _,
            "And the test case should have the expected size"
        );
    }
}
