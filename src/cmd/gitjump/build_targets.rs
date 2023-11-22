use anyhow::Context;
use std::collections::HashMap;

use crate::git::{BranchStatus, GitBranch, GitCommit, GitRef};

use super::{GitTarget, ORIGIN_HEAD};

pub(crate) struct TargetFilter<'a> {
    pub(crate) branch_author: Option<&'a str>,
}

impl<'a> TargetFilter<'a> {
    fn include_branch(&self, b: &git2::Branch, c: &GitCommit) -> bool {
        if let Some(author) = self.branch_author {
            if c.author != author {
                log::trace!("skipping commit authored by {}", c.author);
                return false;
            }
        }

        if let Some(bref) = b.get().name() {
            if bref == ORIGIN_HEAD {
                return false;
            }
            // log::info!("branch ref: {:?}", bref);
        }

        true
    }
}

pub(crate) fn build_targets(
    repo: &git2::Repository,
    filter: &TargetFilter,
) -> anyhow::Result<Vec<GitTarget>> {
    let mut target_map = HashMap::new();

    build_branches(repo, &mut target_map, filter).context("failed to extract branches")?;

    let primary = repo.refname_to_id(ORIGIN_HEAD).ok();
    let mut results = target_map.into_values().collect::<Vec<_>>();
    results.iter_mut().for_each(|t| {
        t.branches.sort();

        if let Some(primary) = primary {
            // This is SLOW
            if let Ok(x) = repo.merge_base(primary, t.commit.id) {
                t.is_merged = x == t.commit.id;
                t.is_primary = primary == t.commit.id;
            }
        }
    });
    results.sort_by(|a, b| b.cmp(a));
    Ok(results)
}

fn build_branches(
    repo: &git2::Repository,
    map: &mut HashMap<git2::Oid, GitTarget>,
    filter: &TargetFilter,
) -> anyhow::Result<()> {
    for branch_result in repo.branches(None)? {
        let (branch, branch_type) = branch_result?;
        let name = match branch.name() {
            Ok(Some(name)) => name.to_owned(),
            Ok(None) => {
                log::warn!("branch name was `None`");
                continue;
            }
            Err(e) => {
                log::error!("could not read branch name: {}", e);
                continue;
            }
        };
        let head = branch.is_head();
        let c = match GitCommit::from_branch(&branch) {
            Ok(c) => c,
            Err(e) => {
                log::error!("could not get commit from branch: {}", e);
                continue;
            }
        };

        if !filter.include_branch(&branch, &c) {
            continue;
        }

        let mut status = BranchStatus::Unique;
        if let Some(upstream_commit) = branch
            .upstream()
            .ok()
            .and_then(|u| GitCommit::from_branch(&u).ok())
        {
            if upstream_commit.id == c.id {
                status = BranchStatus::Match
            } else if let Ok(base) = repo.merge_base(upstream_commit.id, c.id) {
                if base == upstream_commit.id {
                    status = BranchStatus::Ahead
                } else {
                    status = BranchStatus::Behind
                }
            } else {
                status = BranchStatus::Behind
            }
        }

        let branch = GitBranch {
            name,
            upstream: branch.upstream().ok().map(|u| GitRef::from(u).to_string()),
            ref_name: GitRef::from(branch).to_string(),
            branch_type,
            head,
            status,
        };
        let entry = map.entry(c.id).or_insert(GitTarget {
            repo_path: repo.path().to_owned(),
            commit: c,
            branches: Vec::with_capacity(1),
            is_merged: false,
            is_primary: false,
        });
        entry.branches.push(branch);
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use anyhow::Context;
    use tempfile::{tempdir, TempDir};

    use super::*;

    fn create_test_repo(
        origin_path: impl AsRef<std::path::Path>,
    ) -> Result<git2::Repository, anyhow::Error> {
        let origin_path = origin_path.as_ref();
        std::fs::create_dir_all(origin_path)?;

        let origin_repo = git2::Repository::init(origin_path).context("init repo")?;
        std::fs::write(origin_path.join("README.md"), "Hello World!")?;

        let mut index = origin_repo.index()?;
        index.add_path(std::path::Path::new("README.md"))?;
        index.write().context("write index")?;

        let oid = index.write_tree().context("write tree")?;
        let signature =
            git2::Signature::now("author1", "author1@example.com").context("create signature")?;
        // let signature = origin_repo.signature().context("signature")?;

        // let parent_commit = find_last_commit(&origin_repo).context("find last commit")?;
        origin_repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "Initial commit",
                &origin_repo.find_tree(oid)?,
                &[],
            )
            .context("commit")?;

        Ok(origin_repo)
    }

    #[test]
    fn read_repo() -> Result<(), anyhow::Error> {
        let dir = tempdir()?;
        let origin_path = dir.path().join("remote/origin/myrepo.git");

        let r = create_test_repo(origin_path).context("create repo")?;

        // let p = dir.into_path();
        // anyhow::bail!("repo {:?}", p);

        Ok(())
    }

    fn find_last_commit(repo: &git2::Repository) -> Result<git2::Commit, anyhow::Error> {
        let obj = repo
            .head()
            .context("get repo head")?
            .resolve()
            .context("resolve head")?
            .peel(git2::ObjectType::Commit)
            .context("peel oid to commit")?;
        obj.into_commit()
            .map_err(|_| anyhow::anyhow!("couldn't find commit"))
    }
}
