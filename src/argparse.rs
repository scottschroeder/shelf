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
    /// Git Jump
    GitJump(GitJump),
}

#[derive(Parser, Debug)]
pub enum ProjectPicker {
    Dirs(ProjectDirs),
    Preset(ProjectPreset),
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
