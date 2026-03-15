use std::path::PathBuf;

use clap::Parser;

pub fn get_args() -> CliOpts {
    CliOpts::parse()
}

#[derive(Parser, Debug)]
#[clap(version = clap::crate_version!(), author = "Scott S. <scottschroeder@sent.com>")]
pub struct CliOpts {
    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u8,
    #[clap(subcommand)]
    pub subcmd: SubCommand,
}

#[derive(Parser, Debug)]
pub enum SubCommand {
    Test(Test),
    /// Scan for project dirs
    #[clap(subcommand)]
    Project(ProjectPicker),
    /// Manage git worktrees
    #[clap(subcommand)]
    Worktree(WorktreePicker),
    /// Git Jump
    GitJump(GitJump),
}

#[derive(Parser, Debug)]
pub enum ProjectPicker {
    Dirs(ProjectDirs),
    Preset(ProjectPreset),
}

#[derive(Parser, Debug)]
pub enum WorktreePicker {
    Create(WorktreeCreate),
    Cleanup(WorktreeCleanup),
}

#[derive(Parser, Debug)]
pub struct ProjectDirs {
    /// Root directories to scan from
    pub roots: Vec<PathBuf>,
    /// Keep recursing inside git repositories
    #[clap(long)]
    pub git_recurse: bool,
    /// Rename tmux window behavior
    #[clap(long, value_enum)]
    pub tmux_rename: Option<TmuxRename>,
}

#[derive(Parser, Debug)]
pub struct ProjectPreset {
    /// Override config path
    #[clap(long)]
    pub config: Option<PathBuf>,
    /// Rename tmux window behavior
    #[clap(long, value_enum)]
    pub tmux_rename: Option<TmuxRename>,
}

#[derive(Parser, Debug, Clone, clap::ValueEnum)]
pub enum TmuxRename {
    DefaultOnly,
    Force,
}

#[derive(Parser, Debug)]
pub struct GitJump {
    /// Root directories to scan from
    pub root: Option<PathBuf>,
    /// Only show commits by the configured author
    #[clap(long)]
    pub use_author: bool,
    /// Show all branches pointing to a commit, instead of just the first
    #[clap(long)]
    pub show_all_branches: bool,
    /// Do not show preview window for each target
    #[clap(long)]
    pub disable_preview: bool,
    /// Instead of showing the commit log, preview details about the commit
    #[clap(long)]
    pub preview_commit_details: bool,
}

#[derive(Parser, Debug)]
pub struct Test {}

#[derive(Parser, Debug)]
pub struct WorktreeCreate {
    /// Worktree directory name
    pub name: String,
    /// Create or checkout this branch name in the new worktree
    #[clap(short = 'b', long, conflicts_with = "detach")]
    pub branch: Option<String>,
    /// Detach HEAD in the new worktree
    #[clap(short = 'd', long, conflicts_with = "branch")]
    pub detach: bool,
    /// Optional commit-ish (branch, tag, or commit)
    pub commitish: Option<String>,
    /// Override config path
    #[clap(long)]
    pub config: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct WorktreeCleanup {
    /// Override config path
    #[clap(long)]
    pub config: Option<PathBuf>,
}
