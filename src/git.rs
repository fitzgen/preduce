//! Utilities to make working with git and the git2 crate a little more
//! bearable.

use error;
use git2;
use std::fmt;
use std::fs;
use std::ops;
use std::path;
use std::sync::Arc;
use tempdir;

/// The file name for test cases within a git repository.
pub static TEST_CASE_FILE_NAME: &'static str = "test_case";

static COMMIT_SIGNATURE_NAME: &'static str = "preduce";
static COMMIT_SIGNATURE_EMAIL: &'static str = "preduce@noreply.github.com";

/// The git signature for preduce.
pub fn signature() -> git2::Signature<'static> {
    git2::Signature::now(COMMIT_SIGNATURE_NAME, COMMIT_SIGNATURE_EMAIL).unwrap()
}

/// Extension methods for `git2::Repository`.
pub trait RepoExt {
    /// Get the object id for HEAD.
    fn head_id(&self) -> error::Result<git2::Oid>;

    /// Get the commit for HEAD.
    fn head_commit(&self) -> error::Result<git2::Commit>;

    /// Get the tree for HEAD.
    fn head_tree(&self) -> error::Result<git2::Tree>;

    /// Stage the test case and commit it.
    fn commit_test_case(&self, msg: &str) -> error::Result<git2::Oid>;

    /// Get the path to the test case file within this repo.
    fn test_case_path(&self) -> error::Result<path::PathBuf>;

    /// Fetch the origin remote.
    fn fetch_origin(&self) -> error::Result<()>;

    /// Fetch the remote and reset this repository to the given commit id.
    fn fetch_and_reset_hard<P>(&self, remote: P, commit_id: git2::Oid) -> error::Result<()>
        where P: AsRef<path::Path>;
}

impl RepoExt for git2::Repository {
    fn head_id(&self) -> error::Result<git2::Oid> {
        self.find_reference("HEAD")?
            .resolve()?
            .target()
            .ok_or_else(|| git2::Error::from_str("HEAD reference has no target Oid").into())
    }

    fn head_commit(&self) -> error::Result<git2::Commit> {
        let head = self.head_id()?;
        let head = self.find_commit(head)?;
        Ok(head)
    }

    fn head_tree(&self) -> error::Result<git2::Tree> {
        let tree = self.head_commit()?.tree()?;
        Ok(tree)
    }

    fn commit_test_case(&self, msg: &str) -> error::Result<git2::Oid> {
        let mut index = self.index()?;
        index.add_path(path::Path::new(TEST_CASE_FILE_NAME))?;

        let sig = signature();
        let head = self.head_commit()?;
        let tree = self.head_tree()?;
        let parents = [&head];
        let commit_id = self.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parents[..])?;
        Ok(commit_id)
    }

    fn test_case_path(&self) -> error::Result<path::PathBuf> {
        Ok(
            self.path()
                .canonicalize()?
                .parent()
                .expect(".git/ folder should always be within the root of the repo")
                .join(TEST_CASE_FILE_NAME)
        )
    }

    fn fetch_origin(&self) -> error::Result<()> {
        let mut origin = self.find_remote("origin")?;
        origin.fetch(&["master"], None, None)?;
        Ok(())
    }

    fn fetch_and_reset_hard<P>(&self, remote: P, commit_id: git2::Oid) -> error::Result<()>
        where P: AsRef<path::Path>
    {
        let remote = remote.as_ref();
        let remote = remote.to_string_lossy();
        let mut remote = self.remote_anonymous(&remote)?;
        remote.fetch(&["master"], None, None)?;
        let object = self
            .find_object(commit_id, Some(git2::ObjectType::Commit))?;
        self.reset(&object, git2::ResetType::Hard, None)?;
        Ok(())
    }
}

/// A temporary git repository.
pub struct TempRepo {
    // Only an `Option` so we can force its destruction before the temp dir
    // disappears under its feet.
    repo: Option<git2::Repository>,
    _dir: Arc<tempdir::TempDir>,
}

impl fmt::Debug for TempRepo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TempRepo({})", self.path().display())
    }
}

impl ops::Deref for TempRepo {
    type Target = git2::Repository;

    fn deref(&self) -> &git2::Repository {
        self.repo.as_ref().unwrap()
    }
}

impl TempRepo {
    /// Create a new temporary git repository, with an initial commit
    /// introducing an empty test case file within it.
    pub fn new(prefix: &str) -> error::Result<TempRepo> {
        let dir = Arc::new(tempdir::TempDir::new(prefix)?);
        let repo = git2::Repository::init(dir.path())?;

        {
            let test_case_path = repo.test_case_path()?;
            let file = fs::File::create(&test_case_path)?;
            file.sync_all()?;

            let mut index = repo.index()?;
            index
                .add_path(path::Path::new(test_case_path.file_name().unwrap()))?;

            let tree = repo.treebuilder(None)?.write()?;
            let tree = repo.find_tree(tree)?;

            let sig = signature();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;
        }

        Ok(TempRepo {
            repo: Some(repo),
            _dir: dir,
        })
    }

    /// Create a temporary clone repository of a local upstream git repository.
    pub fn clone<P>(upstream: P, prefix: &str) -> error::Result<TempRepo>
    where
        P: AsRef<path::Path>,
    {
        let upstream = upstream.as_ref().to_string_lossy();
        let dir = Arc::new(tempdir::TempDir::new(prefix)?);
        let repo = git2::Repository::clone(&upstream, dir.path())?;
        Ok(TempRepo {
            repo: Some(repo),
            _dir: dir,
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        // Make sure we drop the git repo before the temporary directory goes
        // away.
        drop(self.repo.take());
    }
}
