use std::process::Command;

use anyhow::Context;
pub struct TmuxHandle(());

pub fn get_tmux() -> Option<TmuxHandle> {
    if std::env::var("TMUX").is_ok() {
        Some(TmuxHandle(()))
    } else {
        None
    }
}

impl TmuxHandle {
    pub fn get_tmux_name(&self) -> anyhow::Result<String> {
        let output = Command::new("tmux")
            .args(["display-message", "-p", "#W"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .context("could not spawn tmux")?
            .wait_with_output()
            .context("could not get output from tmux")?;

        let name = String::from_utf8_lossy(&output.stdout);
        Ok(name.trim().to_string())
    }
    pub fn get_tmux_number(&self) -> anyhow::Result<u16> {
        let output = Command::new("tmux")
            .args(["display-message", "-p", "#I"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .context("could not spawn tmux")?
            .wait_with_output()
            .context("could not get output from tmux")?;

        let output = String::from_utf8_lossy(&output.stdout);
        let number = output
            .trim()
            .parse::<u16>()
            .with_context(|| format!("could not parse number from tmux output: {:?}", output))?;
        Ok(number)
    }
    pub fn count_tmux_panes(&self) -> anyhow::Result<usize> {
        let output = Command::new("tmux")
            .args(["list-panes"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .context("could not spawn tmux")?
            .wait_with_output()
            .context("could not get output from tmux")?;

        let output = String::from_utf8_lossy(&output.stdout);
        let number = output.trim().split('\n').count();
        Ok(number)
    }

    pub fn set_tmux_current_window_name(&self, name: &str) -> anyhow::Result<()> {
        let window_number = self.get_tmux_number()?;
        self.set_tmux_window_name(window_number, name)
    }

    fn set_tmux_window_name(&self, window_number: u16, name: &str) -> anyhow::Result<()> {
        Command::new("tmux")
            .args(["rename-window", "-t"])
            .arg(window_number.to_string())
            .arg(name)
            .spawn()
            .context("could not spawn tmux")?
            .wait()
            .context("could not get output from tmux")?;

        Ok(())
    }
}
