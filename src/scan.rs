use std::{
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
};

use walkdir::{DirEntry, WalkDir};

fn is_dir(entry: &DirEntry) -> bool {
    entry.file_type().is_dir()
}

fn is_git_repo(entry: &DirEntry) -> bool {
    let gp = entry.path().join(".git");
    log::trace!("{:?}", gp);
    gp.exists()
}

struct GitRepoWalker {
    root: PathBuf,
    inner: walkdir::IntoIter,
    ignore: regex::bytes::RegexSet,
}

impl Iterator for GitRepoWalker {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.inner.next() {
                Some(Ok(entry)) => {
                    if !is_dir(&entry) {
                        continue;
                    }
                    if entry.path() == self.root {
                        continue;
                    }
                    if self.ignore.is_match(entry.path().as_os_str().as_bytes()) {
                        self.inner.skip_current_dir();
                        continue;
                    }
                    if !is_git_repo(&entry) {
                        continue;
                    }
                    self.inner.skip_current_dir();
                    return Some(entry.into_path());
                }
                Some(Err(e)) => {
                    log::debug!("could not read `{:?}`: {}", e.path(), e);
                }
                None => return None,
            };
        }
    }
}

pub fn scan_git_repos<P: AsRef<Path>>(
    root: P,
    ignore: regex::bytes::RegexSet,
) -> impl Iterator<Item = PathBuf> {
    let root = root.as_ref().to_path_buf();
    let it = WalkDir::new(&root).into_iter();
    GitRepoWalker {
        root,
        inner: it,
        ignore,
    }
}
