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
    // Test(Test),
    /// Scan for project dirs
    #[clap(subcommand)]
    Project(ProjectPicker),
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
}

#[derive(Parser, Debug)]
pub struct ProjectPreset {
    /// Override config path
    pub config: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct Test {}
