use std::{borrow::Cow, collections::HashMap, sync::Arc};

use anyhow::Context;
use skim::{prelude::SkimOptionsBuilder, Skim, SkimItem, SkimItemReceiver, SkimItemSender};
use terminal_size::terminal_size;

use crate::{
    argparse,
    git::{GitBranch, GitCommit, GitRef},
};

const BRANCH_ICON: &str = "";
const WINDOW_SPLIT_MIN_SIZE: u16 = 160;
const RELATIVE_TIME_LOOKBACK_DAYS: i64 = 6;
const RELATIVE_TIME_LOOKBACK_HOURS: i64 = 4;

#[derive(Debug, Clone)]
struct SkimGitTarget {
    inner: GitTarget,
    display_str: skim::AnsiString<'static>,
}

impl From<GitTarget> for SkimGitTarget {
    fn from(value: GitTarget) -> Self {
        let ansi_str = format!("{}", DisplayLine(&value));
        SkimGitTarget {
            inner: value,
            // display_str: skim::AnsiString::parse("\x1B[35mA\x1B[mB"),
            display_str: skim::AnsiString::parse(&ansi_str),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitTarget {
    repo_path: std::path::PathBuf,
    commit: GitCommit,
    branches: Vec<GitBranch>,
}

struct DisplayLine<'a>(&'a GitTarget);

impl<'a> std::fmt::Display for DisplayLine<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let target = self.0;
        let commit_time = DisplayTime(target.commit.time.seconds());
        let author_str = target.commit.author.as_str();
        let author = &[
            ansi_term::Color::Blue.paint("["),
            ansi_term::Color::Blue.paint(author_str),
            ansi_term::Color::Blue.paint("]"),
        ];
        write!(f, "{}", commit_time)?;

        if !target.branches.is_empty() {
            write!(f, " {}", ansi_term::Color::Yellow.paint("("))?;
            for (idx, branch) in target.branches.iter().enumerate() {
                if idx != 0 {
                    write!(f, ", ")?;
                }
                if branch.head {
                    write!(f, "{}", ansi_term::Color::Yellow.bold().paint("*"))?;
                }
                write!(f, "{}", ansi_term::Color::Yellow.paint(&branch.name))?;
            }
            write!(f, "{}", ansi_term::Color::Yellow.paint(")"))?;
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
        // skim::ItemPreview::Text(format!("{:#?}", self))
        skim::ItemPreview::Command(
            format!(
                "git -C {} log --color=always --graph --topo-order --pretty=format:'%C(red)%h%Creset -%C(bold yellow)%d%Creset %s %Cgreen(%cr) %C(blue)<%an>%Creset' {}",
                target.repo_path.display(),
                target.commit.id,
                )
            )
    }
    fn display<'a>(&'a self, _context: skim::DisplayContext<'a>) -> skim::AnsiString<'a> {
        self.display_str.clone()
    }
}

fn build_targets(repo: &git2::Repository) -> anyhow::Result<Vec<GitTarget>> {
    let mut target_map = HashMap::new();

    build_branches(repo, &mut target_map).context("failed to extract branches")?;

    let mut results = target_map.into_values().collect::<Vec<_>>();
    results.iter_mut().for_each(|t| t.branches.sort());
    results.sort_by(|a, b| b.cmp(a));
    Ok(results)
}

fn build_branches(
    repo: &git2::Repository,
    map: &mut HashMap<git2::Oid, GitTarget>,
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
        let branch = GitBranch {
            name,
            ref_name: GitRef::from(branch).to_string(),
            branch_type,
            head,
        };
        let entry = map.entry(c.id).or_insert(GitTarget {
            repo_path: repo.path().to_owned(),
            commit: c,
            branches: Vec::with_capacity(1),
        });
        entry.branches.push(branch);
    }
    Ok(())
}

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

    let targets = build_targets(&repo)?;

    // log::debug!("{:#?}", targets);

    let (send, recv): (SkimItemSender, SkimItemReceiver) = skim::prelude::unbounded();
    for t in targets {
        if args.use_author && Some(t.commit.author.as_str()) != name {
            log::trace!("skipping commit authored by {}", t.commit.author);
            continue;
        }
        let item = Arc::new(SkimGitTarget::from(t));
        if let Err(e) = send.send(item) {
            log::error!("unable to send item for selection: {}", e);
        }
    }
    let target = match select_and_return_first(recv) {
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

fn select_and_return_first(recv: SkimItemReceiver) -> Option<GitTarget> {
    let width_ok = terminal_size().and_then(|(w, _)| {
        if w.0 > WINDOW_SPLIT_MIN_SIZE {
            Some("yes")
        } else {
            None
        }
    });
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
