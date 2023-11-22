use anyhow::Context;
use std::collections::HashMap;

use crate::git::{GitCommit, BranchStatus, GitBranch, GitRef};

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
