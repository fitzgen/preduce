//! Types related to test cases, their interestingness, and candidates
//! of them.

use blake2::{Blake2b, Digest};
use either::{Either, Left, Right};
use error;
use generic_array;
use std::fs;
use std::hash;
use std::io::{self, Read};
use std::path;
use std::process;
use std::sync::Arc;
use tempdir;
use traits;
use typenum;

/// The result of `Blake2b::digest`.
///
/// This is terrible.
pub type Blake2Hash = generic_array::GenericArray<
    u8,
    typenum::uint::UInt<
        typenum::uint::UInt<
            typenum::uint::UInt<
                typenum::uint::UInt<
                    typenum::uint::UInt<
                        typenum::uint::UInt<
                            typenum::uint::UInt<typenum::uint::UTerm, typenum::bit::B1>,
                            typenum::bit::B0,
                        >,
                        typenum::bit::B0,
                    >,
                    typenum::bit::B0,
                >,
                typenum::bit::B0,
            >,
            typenum::bit::B0,
        >,
        typenum::bit::B0,
    >,
>;

/// Methods common to all test cases.
pub trait TestCaseMethods: Into<TempFile> + hash::Hash {
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

    /// The hash of the full test case contents.
    fn full_hash(&self) -> Blake2Hash;

    /// The hash of the diff with the seed from which this test case was
    /// generated, or hash of an empty diff if this is an initial test case.
    fn diff_hash(&self) -> Blake2Hash;
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
/// When generating candidates, we use these immutable, persistent, temporary
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

impl hash::Hash for TempFile {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.path().hash(state);
    }
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

impl From<Candidate> for TempFile {
    fn from(candidate: Candidate) -> TempFile {
        candidate.test_case
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
            InterestingKind::Candidate(r) => r.into(),
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
pub struct Candidate {
    /// From which reducer did this candidate come from?
    provenance: String,

    /// The temporary file containing the reduced test case.
    test_case: TempFile,

    /// The size of the test case file, in bytes.
    size: u64,

    /// The delta size from the parent test case.
    delta: u64,

    /// The hash of the full contents.
    full_hash: Blake2Hash,

    /// The hash of the diff with the seed.
    diff_hash: Blake2Hash,
}

impl hash::Hash for Candidate {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.path().hash(state);
    }
}

impl TestCaseMethods for Candidate {
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

    fn full_hash(&self) -> Blake2Hash {
        self.full_hash
    }

    fn diff_hash(&self) -> Blake2Hash {
        self.diff_hash
    }
}

fn hash<R: Read>(mut src: R) -> error::Result<Blake2Hash> {
    let mut hasher = Blake2b::default();
    let mut buf = vec![0; 1024 * 1024];
    loop {
        let bytes_read = match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e.into());
            }
        };
        hasher.input(&buf[..bytes_read]);
    }
    Ok(hasher.result())
}

impl Candidate {
    /// Construct a new candidate.
    ///
    /// The `seed` must be the interesting test case from which a reducer
    /// produced this candidate.
    ///
    /// The `provenance` must be a diagnostic describing the reducer that
    /// produced this candidate.
    ///
    /// The `test_case` must be the file path of the candidate's test
    /// case.
    pub fn new<S, T>(
        seed: Interesting,
        provenance: S,
        test_case: T,
    ) -> error::Result<Candidate>
    where
        S: Into<String>,
        T: Into<TempFile>,
    {
        let provenance = provenance.into();
        assert!(!provenance.is_empty());

        let test_case = test_case.into();
        let size;
        let full_hash;
        let diff_hash;
        {
            let path = test_case.path();
            size = fs::metadata(&path)?.len();

            let file = fs::File::open(&path)?;
            full_hash = hash(file)?;

            let diff = process::Command::new("diff")
                .args(&[
                    "--unchanged-line-format=''",
                    "--new-line-format='+%L'",
                    "--old-line-format='-%L'",
                    &provenance,
                    &path.display().to_string(),
                ])
                .stdout(process::Stdio::piped())
                .stderr(process::Stdio::null())
                .stdin(process::Stdio::null())
                .output()?
                .stdout;
            diff_hash = hash(&diff[..])?;
        }
        Ok(Candidate {
            provenance: provenance,
            test_case,
            size: size,
            delta: seed.size().saturating_sub(size),
            full_hash,
            diff_hash,
        })
    }

    /// Try and convert this *potentially interesting* candidate into a *known
    /// interesting* test case by validating whether it is interesting or not
    /// using the given `judge`.
    pub fn into_interesting<I>(
        self,
        judge: &I,
    ) -> error::Result<Either<Interesting, Candidate>>
    where
        I: ?Sized + traits::IsInteresting,
    {
        assert!(self.path().is_file());

        if !judge.is_interesting(self.path())? {
            return Ok(Right(self));
        }

        Ok(Left(Interesting {
            kind: InterestingKind::Candidate(self),
        }))
    }
}

/// A test case that has been verified to be interesting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Interesting {
    /// The kind of interesting test case.
    kind: InterestingKind,
}

impl hash::Hash for Interesting {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.path().hash(state);
    }
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

    fn full_hash(&self) -> Blake2Hash {
        self.kind.full_hash()
    }

    fn diff_hash(&self) -> Blake2Hash {
        self.kind.diff_hash()
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
        let full_hash = hash(fs::File::open(file_path.as_ref())?)?;

        Ok(Some(Interesting {
            kind: InterestingKind::Initial(InitialInteresting {
                test_case: temp_file,
                size: size,
                full_hash,
            }),
        }))
    }

    /// If this interesting test case was created from a candidate, rather than
    /// the initial interesting test case, coerce it to a `Candidate`.
    pub fn as_candidate(&self) -> Option<&Candidate> {
        match self.kind {
            InterestingKind::Initial(..) => None,
            InterestingKind::Candidate(ref r) => Some(r),
        }
    }
}

/// An enumeration of the kinds of interesting test cases.
#[derive(Clone, Debug, Eq, PartialEq)]
enum InterestingKind {
    /// The initial interesting test case.
    Initial(InitialInteresting),

    /// A candidate of the initial test case that has been found to be
    /// interesting.
    Candidate(Candidate),
}

impl hash::Hash for InterestingKind {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.path().hash(state);
    }
}

impl TestCaseMethods for InterestingKind {
    fn path(&self) -> &path::Path {
        match *self {
            InterestingKind::Initial(ref initial) => initial.path(),
            InterestingKind::Candidate(ref candidate) => candidate.path(),
        }
    }

    fn size(&self) -> u64 {
        match *self {
            InterestingKind::Initial(ref initial) => initial.size(),
            InterestingKind::Candidate(ref candidate) => candidate.size(),
        }
    }

    fn delta(&self) -> u64 {
        match *self {
            InterestingKind::Initial(ref initial) => initial.delta(),
            InterestingKind::Candidate(ref candidate) => candidate.delta(),
        }
    }

    fn provenance(&self) -> &str {
        match *self {
            InterestingKind::Initial(ref i) => i.provenance(),
            InterestingKind::Candidate(ref r) => r.provenance(),
        }
    }

    fn full_hash(&self) -> Blake2Hash {
        match *self {
            InterestingKind::Initial(ref i) => i.full_hash(),
            InterestingKind::Candidate(ref r) => r.full_hash(),
        }
    }

    fn diff_hash(&self) -> Blake2Hash {
        match *self {
            InterestingKind::Initial(ref i) => i.diff_hash(),
            InterestingKind::Candidate(ref r) => r.diff_hash(),
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

    /// The hash of the full file contents.
    full_hash: Blake2Hash,
}

impl hash::Hash for InitialInteresting {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.path().hash(state);
    }
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

    fn full_hash(&self) -> Blake2Hash {
        self.full_hash
    }

    fn diff_hash(&self) -> Blake2Hash {
        hash(&[][..]).expect("reading from an empty slice cannot fail")
    }
}

#[cfg(test)]
impl Candidate {
    pub fn testing_only_new() -> Candidate {
        Candidate {
            provenance: "Candidate::testing_only_new".into(),
            test_case: TempFile::anonymous().unwrap(),
            size: 0,
            delta: 0,
            full_hash: Default::default(),
            diff_hash: Default::default(),
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
                full_hash: Default::default(),
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
            contents.len() as u64,
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

        let candidate = Candidate::testing_only_new();
        {
            let mut candidate_file = fs::File::create(candidate.path()).unwrap();
            writeln!(&mut candidate_file, "la").unwrap();
        }

        let candidate = Candidate::new(interesting, "test", candidate)
            .expect("should create potenetial candidate");

        let interesting_candidate = candidate
            .clone()
            .into_interesting(&judge)
            .expect("interesting candidate should be ok")
            .left()
            .expect("interesting candidate should be some");

        assert_eq!(
            interesting_candidate.path(),
            candidate.path(),
            "The interesting candidate's path should be the same as the candidate's path"
        );

        assert!(
            interesting_candidate.path().is_file(),
            "The interesting candidate's path should have a file"
        );

        let mut file = fs::File::open(&interesting_candidate.path())
            .expect("The interesting candidate path should have a file");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("And we should read from that file");
        assert_eq!(contents, "la\n", "And it should have the expected contents");

        assert_eq!(
            interesting_candidate.size(),
            contents.len() as u64,
            "And the test case should have the expected size"
        );
    }
}
