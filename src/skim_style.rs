use ansi_term::{Color, Style};

use crate::config::NamedColor;

const MUTED_GREY: Color = Color::RGB(120, 120, 120);
const COMMIT_GREY: Color = Color::RGB(170, 170, 170);

pub(crate) fn project_tag_style(color: Option<NamedColor>) -> Style {
    Style::new()
        .fg(color.map(color_from_name).unwrap_or(Color::Blue))
        .bold()
}

pub(crate) fn worktree_suffix_style() -> Style {
    Style::new().fg(Color::Cyan)
}

pub(crate) fn worktree_name_style(dirty: bool) -> Style {
    if dirty {
        Style::new().fg(Color::Red).bold()
    } else {
        Style::new().fg(Color::Blue).bold()
    }
}

pub(crate) fn branch_style() -> Style {
    Style::new().fg(Color::Yellow)
}

pub(crate) fn upstream_style() -> Style {
    Style::new().fg(Color::Cyan)
}

pub(crate) fn clean_style() -> Style {
    Style::new().fg(Color::Green)
}

pub(crate) fn dirty_style() -> Style {
    Style::new().fg(Color::Red).bold()
}

pub(crate) fn detached_style() -> Style {
    Style::new().fg(Color::Yellow).bold()
}

pub(crate) fn locked_style() -> Style {
    Style::new().fg(Color::Purple)
}

pub(crate) fn prunable_style() -> Style {
    Style::new().fg(MUTED_GREY)
}

pub(crate) fn commit_message_style() -> Style {
    Style::new().fg(COMMIT_GREY)
}

pub(crate) fn color_from_name(name: NamedColor) -> Color {
    match name {
        NamedColor::Blue => Color::Blue,
        NamedColor::Cyan => Color::Cyan,
        NamedColor::Green => Color::Green,
        NamedColor::Yellow => Color::Yellow,
        NamedColor::Red => Color::Red,
        NamedColor::Magenta => Color::Purple,
        NamedColor::White => Color::White,
    }
}
