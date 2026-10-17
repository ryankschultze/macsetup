//! Actions the dashboard can ask the engine to take, and the exact `setup`
//! command each one runs. Edits to the repo's package lists always pass
//! `--commit`, so the engine commits them (and never pushes).

use crate::model::JobKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewChoice {
    Keep,
    Take,
    Later,
    Merge,
}

impl ReviewChoice {
    pub fn arg(self) -> &'static str {
        match self {
            ReviewChoice::Keep => "keep",
            ReviewChoice::Take => "take",
            ReviewChoice::Later => "later",
            ReviewChoice::Merge => "merge",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Install { ids: Vec<String> },
    Uninstall { ids: Vec<String>, drop: bool },
    Move { id: String, to: String },
    Drop { ids: Vec<String> },
    CaptureAdd { id: String, to: String },
    Review { file: String, choice: ReviewChoice },
}

impl Action {
    /// The arguments after `setup` (the dashboard adds `--json` or `--events`).
    pub fn argv(&self, dry_run: bool) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        if dry_run {
            v.push("--dry-run".into());
        }
        let mut push = |s: &str| v.push(s.to_string());
        match self {
            Action::Install { ids } => {
                push("install");
                push("--items");
                push(&ids.join(","));
            }
            Action::Uninstall { ids, drop } => {
                push("uninstall");
                push(&ids.join(","));
                if *drop {
                    push("--drop");
                    push("--commit");
                }
            }
            Action::Move { id, to } => {
                push("move");
                push(id);
                push("--to");
                push(to);
                push("--commit");
            }
            Action::Drop { ids } => {
                push("drop");
                push(&ids.join(","));
                push("--commit");
            }
            Action::CaptureAdd { id, to } => {
                push("capture");
                push("--add");
                push(id);
                push("--to");
                push(to);
                push("--commit");
            }
            Action::Review { file, choice } => {
                push("review");
                push("--file");
                push(file);
                push("--action");
                push(choice.arg());
            }
        }
        v
    }

    /// The command as shown to the user.
    pub fn command_line(&self, dry_run: bool) -> String {
        let mut s = String::from("setup");
        for a in self.argv(dry_run) {
            s.push(' ');
            if a.is_empty() || a.contains(|c: char| c.is_whitespace() || "'\"\\$`".contains(c)) {
                s.push('\'');
                s.push_str(&a.replace('\'', r"'\''"));
                s.push('\'');
            } else {
                s.push_str(&a);
            }
        }
        s
    }

    /// Whether this runs attached to the terminal instead of in the background:
    /// Homebrew may ask for a password for pkg-based casks, App Store installs
    /// can prompt, vendor installers are interactive, and merges open a tool.
    pub fn needs_terminal(&self) -> bool {
        let interactive = |id: &String| {
            id.starts_with("cask:") || id.starts_with("mas:") || id.starts_with("vendor:")
        };
        match self {
            Action::Install { ids } | Action::Uninstall { ids, .. } => ids.iter().any(interactive),
            Action::Review { choice, .. } => *choice == ReviewChoice::Merge,
            Action::Move { .. } | Action::Drop { .. } | Action::CaptureAdd { .. } => false,
        }
    }

    /// A merge opens an interactive tool, so it has nothing to preview.
    pub fn can_dry_run(&self) -> bool {
        !matches!(
            self,
            Action::Review {
                choice: ReviewChoice::Merge,
                ..
            }
        )
    }

    /// A short title, e.g. "Uninstall brew:jq".
    pub fn title(&self) -> String {
        let list = |ids: &[String]| match ids {
            [one] => one.clone(),
            many => format!("{} items", many.len()),
        };
        match self {
            Action::Install { ids } => format!("Install {}", list(ids)),
            Action::Uninstall { ids, drop: false } => format!("Uninstall {}", list(ids)),
            Action::Uninstall { ids, drop: true } => {
                format!("Uninstall and drop {}", list(ids))
            }
            Action::Move { id, to } => format!("Move {id} to {to}"),
            Action::Drop { ids } => format!("Drop {} from the repo", list(ids)),
            Action::CaptureAdd { id, to } => format!("Add {id} to {to}"),
            Action::Review { file, choice } => match choice {
                ReviewChoice::Keep => format!("Keep your {file}"),
                ReviewChoice::Take => format!("Take the repo's {file}"),
                ReviewChoice::Later => format!("Decide on {file} later"),
                ReviewChoice::Merge => format!("Merge {file}"),
            },
        }
    }

    /// One line on what happens, for the confirm dialog.
    pub fn explain(&self) -> &'static str {
        match self {
            Action::Install { .. } => "Installs what's missing; nothing is upgraded.",
            Action::Uninstall { drop: false, .. } => {
                "Removes it from this Mac but keeps it in the repo. Refused if another formula needs it or the app came from outside Homebrew."
            }
            Action::Uninstall { drop: true, .. } => {
                "Removes it from this Mac and from its Brewfile, then commits the Brewfile. Refused if another formula needs it or the app came from outside Homebrew."
            }
            Action::Move { .. } => "Moves the Brewfile entry and commits both Brewfiles.",
            Action::Drop { .. } => {
                "Removes the entry from its Brewfile and commits it. Nothing is uninstalled."
            }
            Action::CaptureAdd { .. } => "Adds it to the group's Brewfile and commits it.",
            Action::Review {
                choice: ReviewChoice::Take,
                ..
            } => "Replaces your file with the repo's version; yours is backed up first.",
            Action::Review {
                choice: ReviewChoice::Keep,
                ..
            } => "Keeps your file and stops listing it.",
            Action::Review {
                choice: ReviewChoice::Later,
                ..
            } => "Leaves your file as it is and keeps it on the list.",
            Action::Review {
                choice: ReviewChoice::Merge,
                ..
            } => "Opens chezmoi's merge tool in this terminal.",
        }
    }

    /// What to reload once the action is done.
    pub fn refresh(&self) -> Vec<JobKind> {
        match self {
            Action::Review { .. } => vec![JobKind::Check],
            Action::Install { .. }
            | Action::Uninstall { .. }
            | Action::Move { .. }
            | Action::Drop { .. }
            | Action::CaptureAdd { .. } => vec![JobKind::List, JobKind::CaptureList],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn command_lines_quote_only_when_needed() {
        let a = Action::Review {
            file: "~/My Notes.conf".into(),
            choice: ReviewChoice::Keep,
        };
        assert_eq!(
            a.command_line(false),
            "setup review --file '~/My Notes.conf' --action keep"
        );
        let a = Action::Drop {
            ids: ids(&["brew:pkg-one"]),
        };
        assert_eq!(
            a.command_line(true),
            "setup --dry-run drop brew:pkg-one --commit"
        );
    }

    #[test]
    fn titles() {
        assert_eq!(
            Action::Uninstall {
                ids: ids(&["a:1", "b:2"]),
                drop: true
            }
            .title(),
            "Uninstall and drop 2 items"
        );
    }
}
