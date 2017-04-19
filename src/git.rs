//! Utilities to make working with git and the git2 crate a little more
//! bearable.

use error;
use git2;
use std::fmt;
use std::fs;
use std::marker;
use std::ops;
use std::path;
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
        Ok(self.path()
               .canonicalize()?
               .parent()
               .expect(".git/ folder should always be within the root of the repo")
               .join(TEST_CASE_FILE_NAME))
    }
}

/// TODO FITZGEN
pub struct TempRepo<'a>(git2::Repository, marker::PhantomData<&'a tempdir::TempDir>);

impl<'a> fmt::Debug for TempRepo<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TempRepo({})", self.path().display())
    }
}

impl<'a> ops::Deref for TempRepo<'a> {
    type Target = git2::Repository;

    fn deref(&self) -> &git2::Repository {
        &self.0
    }
}

impl<'a> TempRepo<'a> {
    /// TODO FITZGEN
    pub fn new(dir: &'a tempdir::TempDir) -> error::Result<TempRepo<'a>> {
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

        Ok(TempRepo(repo, marker::PhantomData))
    }

    /// TODO FITZGEN
    pub fn clone<P>(upstream: P, dir: &'a tempdir::TempDir) -> error::Result<TempRepo<'a>>
        where P: AsRef<path::Path>
    {
        let upstream = upstream.as_ref().to_string_lossy();
        let repo = git2::Repository::clone(&upstream, dir.path())?;
        Ok(TempRepo(repo, marker::PhantomData))
    }
}
