use crate::tmux::get_tmux;

mod argparse;

mod cmd {
    pub mod gitjump;
    pub mod project;
    pub mod worktree;
}
mod config;
mod git;
mod scan;
mod skim_style;
mod tmux;
mod worktree;

fn main() -> anyhow::Result<()> {
    color_backtrace::install();
    let args = argparse::get_args();
    setup_logger(args.verbose);
    log::trace!("Args: {:?}", args);

    match &args.subcmd {
        argparse::SubCommand::Project(cmd) => match cmd {
            argparse::ProjectPicker::Dirs(args) => cmd::project::dirs(args),
            argparse::ProjectPicker::Preset(args) => cmd::project::preset(args),
        },
        argparse::SubCommand::Worktree(cmd) => match cmd {
            argparse::WorktreePicker::Create(args) => cmd::worktree::create(args),
            argparse::WorktreePicker::Cleanup(args) => cmd::worktree::cleanup(args),
            argparse::WorktreePicker::CleanupAll(args) => cmd::worktree::cleanup_all(args),
        },
        argparse::SubCommand::Test(_) => {
            if let Some(tmux) = get_tmux() {
                println!(
                    "Tmux #{} [{}] panes={}",
                    tmux.get_tmux_number()?,
                    tmux.get_tmux_name()?,
                    tmux.count_tmux_panes()?,
                );
            } else {
                log::warn!("not inside tmux");
            }
            Ok(())
        }
        argparse::SubCommand::GitJump(cmd) => cmd::gitjump::jump(cmd),
    }
    .map_err(|e| {
        log::error!("{:?}", e);
        anyhow::anyhow!("unrecoverable {} failure", clap::crate_name!())
    })
}

pub fn setup_logger(level: u8) {
    let mut builder = pretty_env_logger::formatted_timed_builder();

    let noisy_modules: &[&str] = &["skim", "tuikit"];

    let log_level = match level {
        //0 => log::Level::Error,
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    if level > 1 && level < 4 {
        for module in noisy_modules {
            builder.filter_module(module, log::LevelFilter::Info);
        }
    }

    builder.filter_level(log_level);
    builder.format_timestamp_millis();
    builder.init();
}
