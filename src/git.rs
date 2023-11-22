use anyhow::Context;

pub(crate) struct GitRef<'r>(git2::Reference<'r>);

impl<'r> std::fmt::Display for GitRef<'r> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0.name().unwrap_or(""))
    }
}

impl<'r> std::fmt::Debug for GitRef<'r> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("GitRef")
            .field(&self.0.name().unwrap_or(""))
            .finish()
    }
}

impl<'r> From<git2::Reference<'r>> for GitRef<'r> {
    fn from(value: git2::Reference<'r>) -> Self {
        GitRef(value)
    }
}

impl<'r> From<git2::Branch<'r>> for GitRef<'r> {
    fn from(value: git2::Branch<'r>) -> Self {
        GitRef(value.into_reference())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BranchStatus {
    Unique,
    Ahead,
    Behind,
    Match,
}

// #[derive(Debug, Clone, PartialEq, Eq)]
// pub(crate) struct BranchStatus {
//     pub(crate) local_only: Option<bool>,
//     // pub(crate) : bool,

// }

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitBranch {
    pub(crate) name: String,
    pub(crate) ref_name: String,
    pub(crate) branch_type: git2::BranchType,
    pub(crate) head: bool,
    pub(crate) upstream: Option<String>,
    pub(crate) status: BranchStatus,
}

impl PartialOrd for GitBranch {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GitBranch {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self.branch_type, other.branch_type) {
            (git2::BranchType::Local, git2::BranchType::Remote) => return std::cmp::Ordering::Less,
            (git2::BranchType::Remote, git2::BranchType::Local) => {
                return std::cmp::Ordering::Greater
            }
            _ => {}
        }
        match self.ref_name.cmp(&other.ref_name) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.head.cmp(&other.head)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitCommit {
    pub(crate) id: git2::Oid,
    pub(crate) message: String,
    pub(crate) time: git2::Time,
    pub(crate) author: String,
}

impl PartialOrd for GitCommit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.time.partial_cmp(&other.time)
    }
}

impl Ord for GitCommit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other)
            .expect("invalid comparison between commits")
    }
}

impl GitCommit {
    pub(crate) fn from_branch<'r>(branch: &git2::Branch<'r>) -> anyhow::Result<GitCommit> {
        let commit = branch
            .get()
            .peel_to_commit()
            .context("unable to peel commit")?;
        let author = commit.author();
        Ok(GitCommit {
            id: commit.id(),
            message: String::from_utf8_lossy(commit.message_bytes()).into_owned(),
            time: commit.time(),
            author: String::from_utf8_lossy(author.name_bytes()).into_owned(),
        })
    }
}
