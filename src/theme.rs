//! Semantic styles. Colour is never the only signal (every state also has a
//! symbol or word), and `NO_COLOR` turns colour off entirely.

use ratatui::style::{Color, Modifier, Style};

use crate::events::{DotfileState, ItemState, Status};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub color: bool,
}

impl Theme {
    /// Colour unless `NO_COLOR` is set to a non-empty value (no-color.org).
    pub fn from_env() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        Theme { color: !no_color }
    }

    pub fn plain() -> Self {
        Theme { color: false }
    }

    fn fg(self, c: Color) -> Style {
        if self.color {
            Style::new().fg(c)
        } else {
            Style::new()
        }
    }

    pub fn item_state(self, s: ItemState) -> Style {
        match s {
            ItemState::Installed => self.fg(Color::Green),
            ItemState::Missing => self.fg(Color::Red),
            ItemState::Outside => self.fg(Color::Cyan),
            ItemState::Mismatch => self.fg(Color::Yellow),
            ItemState::Unknown => self.dim(),
        }
    }

    pub fn status(self, s: Status) -> Style {
        match s {
            Status::Ok => self.fg(Color::Green),
            Status::Skip | Status::Unknown => self.dim(),
            Status::Warn | Status::Manual => self.fg(Color::Yellow),
            Status::Fail => self.fg(Color::Red),
        }
    }

    pub fn dotfile(self, s: DotfileState) -> Style {
        match s {
            DotfileState::Differs => self.fg(Color::Yellow),
            DotfileState::Pending => self.fg(Color::Cyan),
            DotfileState::Unknown => self.dim(),
        }
    }

    pub fn dim(self) -> Style {
        self.fg(Color::DarkGray)
    }

    pub fn error(self) -> Style {
        self.fg(Color::Red).add_modifier(Modifier::BOLD)
    }

    pub fn heading(self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    pub fn key(self) -> Style {
        self.fg(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    /// The selected row: reversed, so it works without colour.
    pub fn selected(self) -> Style {
        Style::new().add_modifier(Modifier::REVERSED)
    }

    pub fn tab_active(self) -> Style {
        Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
    }
}

/// One-cell marker for a step's status.
pub fn status_symbol(s: Status) -> &'static str {
    match s {
        Status::Ok => "✓",
        Status::Skip => "·",
        Status::Warn => "!",
        Status::Manual => "→",
        Status::Fail => "✗",
        Status::Unknown => "?",
    }
}

/// One-cell marker for a dotfile's state.
pub fn dotfile_symbol(s: DotfileState) -> &'static str {
    match s {
        DotfileState::Differs => "≠",
        DotfileState::Pending => "+",
        DotfileState::Unknown => "?",
    }
}
