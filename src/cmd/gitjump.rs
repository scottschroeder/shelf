use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::Context;
use skim::{prelude::SkimOptionsBuilder, Skim, SkimItem, SkimItemReceiver, SkimItemSender};
use terminal_size::terminal_size;

use crate::{
    argparse,
    git::{BranchStatus, GitBranch, GitCommit, GitRef},
};

const BRANCH_ICON: &str = "";
const WINDOW_SPLIT_MIN_SIZE: u16 = 160;
const RELATIVE_TIME_LOOKBACK_DAYS: i64 = 6;
const RELATIVE_TIME_LOOKBACK_HOURS: i64 = 4;
const ORIGIN_HEAD: &str = "refs/remotes/origin/HEAD";

#[derive(Debug, Clone)]
struct SkimGitTarget {
    inner: GitTarget,
    preview_details: bool,
    display_str: skim::AnsiString<'static>,
}

impl SkimGitTarget {
    fn new(target: GitTarget, preview_details: bool) -> SkimGitTarget {
        let ansi_str = format!(
            "{}",
            DisplayLine {
                target: &target,
                collapse_pushed: true,
            }
        );
        SkimGitTarget {
            inner: target,
            preview_details,
            display_str: skim::AnsiString::parse(&ansi_str),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitTarget {
    repo_path: std::path::PathBuf,
    commit: GitCommit,
    branches: Vec<GitBranch>,
    is_merged: bool,
    is_primary: bool,
}

struct DisplayLine<'a> {
    target: &'a GitTarget,
    collapse_pushed: bool,
}

const GREY: ansi_term::Color = ansi_term::Color::RGB(55, 55, 55);

impl<'a> DisplayLine<'a> {
    fn author_color(&self) -> ansi_term::Color {
        ansi_term::Color::Blue
    }
    fn branch_color(&self) -> ansi_term::Color {
        if self.target.is_primary || !self.target.is_merged {
            ansi_term::Color::Yellow
        } else {
            GREY
        }
    }
}

fn is_remote_of(local: &str, inspect: &str) -> bool {
    local
        .strip_prefix("refs/heads/")
        .zip(inspect.strip_prefix("refs/remotes/"))
        .and_then(|(l, r)| r.strip_suffix(l))
        .is_some()
}

impl<'a> std::fmt::Display for DisplayLine<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let target = self.target;
        let commit_time = DisplayTime(target.commit.time.seconds());
        let author_str = target.commit.author.as_str();
        let author_style = self.author_color();
        let author = &[
            author_style.paint("["),
            author_style.paint(author_str),
            author_style.paint("]"),
        ];
        write!(f, "{}", commit_time)?;

        if !target.branches.is_empty() {
            let mut seen: HashSet<&str> = HashSet::new();
            let branch_style = self.branch_color();
            write!(
                f,
                " {}{}",
                branch_style.paint(BRANCH_ICON),
                branch_style.paint("("),
            )?;
            'b: for (idx, branch) in target.branches.iter().enumerate() {
                for s in &seen {
                    if is_remote_of(s, branch.ref_name.as_str()) {
                        continue 'b;
                    }
                }
                seen.insert(branch.ref_name.as_str());
                if idx != 0 {
                    write!(f, ", ")?;
                }
                if branch.head {
                    write!(f, "{}", branch_style.bold().paint("*"))?;
                }
                write!(f, "{}", branch_style.paint(&branch.name))?;
            }
            write!(f, "{}", branch_style.paint(")"))?;
        }

        write!(f, " {}", target.commit.message.trim())?;

        write!(f, " {}", ansi_term::ANSIStrings(author))?;

        Ok(())
    }
}

struct DisplayTime(i64);

impl std::fmt::Display for DisplayTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let dt =
            chrono::NaiveDateTime::from_timestamp_opt(self.0, 0).expect("invalid unix timestamp");
        let ut = dt
            .and_local_timezone(chrono::prelude::Utc)
            .latest()
            .expect("unix timestamp did not make sense in UTC");
        let lt = ut.with_timezone(&chrono::prelude::Local);

        let now = chrono::prelude::Local::now();
        let dur = now.signed_duration_since(lt);
        if dur.num_minutes() < 2 {
            write!(f, "-{}s ago", dur.num_seconds())
        } else if dur.num_hours() < 2 {
            write!(f, "-{}m ago", dur.num_minutes())
        } else if dur.num_hours() < RELATIVE_TIME_LOOKBACK_HOURS {
            write!(f, "-{}h ago", dur.num_hours())
        } else if dur.num_days() < RELATIVE_TIME_LOOKBACK_DAYS {
            write!(f, "{}", lt.format("%a %I:%M%p"))
        } else {
            write!(f, "{}", lt.format("%m/%d/%y %I:%M%p"))
        }
    }
}

impl PartialOrd for GitTarget {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.commit.cmp(&other.commit))
    }
}

impl Ord for GitTarget {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.commit.cmp(&other.commit)
    }
}

impl SkimItem for SkimGitTarget {
    fn text(&self) -> std::borrow::Cow<str> {
        Cow::Owned(format!("{:?}", self.inner))
    }
    fn preview(&self, _context: skim::PreviewContext) -> skim::ItemPreview {
        let target = &self.inner;
        if self.preview_details {
            skim::ItemPreview::Text(format!("{:#?}", target))
        } else {
            skim::ItemPreview::Command(
            format!(
                "git -C {} log --color=always --graph --topo-order --pretty=format:'%C(red)%h%Creset -%C(bold yellow)%d%Creset %s %Cgreen(%cr) %C(blue)<%an>%Creset' {}",
                target.repo_path.display(),
                target.commit.id,
                )
            )
        }
    }
    fn display<'a>(&'a self, _context: skim::DisplayContext<'a>) -> skim::AnsiString<'a> {
        self.display_str.clone()
    }
}

struct TargetFilter<'a> {
    branch_author: Option<&'a str>,
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

fn build_targets(
    args: &argparse::GitJump,
    repo: &git2::Repository,
    filter: &TargetFilter,
) -> anyhow::Result<Vec<GitTarget>> {
    let mut target_map = HashMap::new();

    build_branches(repo, &mut target_map, filter).context("failed to extract branches")?;

    let primary = repo.refname_to_id(ORIGIN_HEAD).ok();
    let mut results = target_map.into_values().collect::<Vec<_>>();
    results.iter_mut().for_each(|t| {
        t.branches.sort();
        if !args.show_all_branches {
            t.branches.truncate(1)
        }

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

// fn annotate_branch_relationships(repo: &git2::Repository, branches: &mut [GitBranch]) {
//     let mut seen = HashMap::new();
//     for b in branches {
//         let b_oid = repo.refname_to_id(b.ref_name.as_str()).ok();
//     }
// }

pub fn jump(args: &argparse::GitJump) -> anyhow::Result<()> {
    log::trace!("{:?}", args);

    let start_dir = args
        .root
        .clone()
        .map(Ok)
        .unwrap_or_else(std::env::current_dir)?;

    let repo = git2::Repository::discover(start_dir).context("git")?;
    log::trace!("using {:?} as project dir", repo.path());

    let config = repo.config().context("get config")?;
    let user = config.get_entry("user.name").context("get user.name")?;
    let name = user.value();

    let filter = TargetFilter {
        branch_author: name.and_then(|n| args.use_author.then_some(n)),
    };

    let targets = build_targets(args, &repo, &filter)?;

    let recv = {
        let (send, recv): (SkimItemSender, SkimItemReceiver) = skim::prelude::unbounded();
        for t in targets {
            let item = Arc::new(SkimGitTarget::new(t, args.preview_commit_details));
            if let Err(e) = send.send(item) {
                log::error!("unable to send item for selection: {}", e);
            }
        }
        recv
    };
    let target = match select_and_return_first(args, recv) {
        Some(t) => t,
        None => {
            log::warn!("no selection was made");
            return Ok(());
        }
    };
    log::debug!("{:#?}", target);

    checkout_target(&repo, &target)?;

    Ok(())
}

fn checkout_target(repo: &git2::Repository, target: &GitTarget) -> anyhow::Result<()> {
    if let Some(b) = target.branches.get(0) {
        log::debug!("checkout branch: {:?}", b.name);
        let branch = repo
            .find_branch(&b.name, b.branch_type)
            .context("could not get branch by name")?;
        let tree = branch.get().peel_to_tree().context("peel branch to tree")?;
        // log::trace!("branch ref: {:?}", GitRef::from(branch.into_reference()));
        repo.checkout_tree(tree.as_object(), None)
            .context("checkout failed")?;
        repo.set_head(
            branch
                .get()
                .name()
                .ok_or_else(|| anyhow::anyhow!("invalid branch ref name"))?,
        )
        .context("could not set head to branch ref")?;
        // repo.seth
        return Ok(());
    }

    log::debug!("checkout commit: {:?}", target.commit.id);
    let o = repo
        .find_object(target.commit.id, Some(git2::ObjectType::Commit))
        .context("could not get commit from hash")?;
    repo.checkout_tree(&o, None).context("checkout failed")?;

    Ok(())
}

fn select_and_return_first(args: &argparse::GitJump, recv: SkimItemReceiver) -> Option<GitTarget> {
    let width_ok = if args.disable_preview {
        None
    } else {
        terminal_size().and_then(|(w, _)| {
            if w.0 > WINDOW_SPLIT_MIN_SIZE {
                Some("yes")
            } else {
                None
            }
        })
    };
    let options = SkimOptionsBuilder::default()
        // .height(Some("50%"))
        .multi(false)
        .preview(width_ok)
        .build()
        .unwrap();

    let result = Skim::run_with(&options, Some(recv))?;
    if result.is_abort {
        None
    } else {
        result
            .selected_items
            .get(0)?
            .as_any()
            .downcast_ref::<SkimGitTarget>()
            .map(|s| s.inner.clone())
    }
}
