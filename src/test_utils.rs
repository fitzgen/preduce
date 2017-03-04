#![allow(missing_docs)]

use git::RepoExt;
use git2;
use std::env;
use std::fs;
use std::marker;
use std::ops;
use std::path;
use tempdir;

pub fn get_script(s: &str) -> path::PathBuf {
    let mut script = path::PathBuf::new();
    if let Ok(dir) = env::var("CARGO_MANIFEST_DIR") {
        script.push(dir);
    }
    script.push("tests");
    script.push(s);
    script
}

pub fn get_exit_0() -> path::PathBuf {
    get_script("exit_0.sh")
}

pub fn get_exit_1() -> path::PathBuf {
    get_script("exit_1.sh")
}

pub struct TestRepo<'a>(git2::Repository, marker::PhantomData<&'a [u8]>);

impl<'a> ops::Deref for TestRepo<'a> {
    type Target = git2::Repository;

    fn deref(&self) -> &git2::Repository {
        &self.0
    }
}

impl<'a> TestRepo<'a> {
    pub fn new(dir: &'a tempdir::TempDir) -> TestRepo<'a> {
        let repo = git2::Repository::init(dir.path()).expect("should init new test repo");

        {
            let test_case_path = repo.test_case_path().expect("should get test case path");
            let file = fs::File::create(&test_case_path)
                .expect("should create test case file in repo");
            file.sync_all()
                .expect("should sync file to disk");

            let mut index = repo.index()
                .expect("should get repo's index");
            index.add_path(path::Path::new(test_case_path.file_name().unwrap()))
                .expect("should add test case to index");

            let tree = repo.treebuilder(None)
                .expect("should create tree builder")
                .write()
                .expect("should write tree to disk");
            let tree = repo.find_tree(tree)
                .expect("should get tree from repo");

            let sig = git2::Signature::now("test", "test@test.com")
                .expect("should make new signature");

            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .expect("should make initial commit");
        }

        TestRepo(repo, marker::PhantomData)
    }
}
