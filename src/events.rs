//! The engine's JSON events (see `docs/contract.md`). Unknown fields are
//! ignored and an unknown `type` becomes [`Event::Other`], so a newer engine
//! never crashes an older dashboard.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Hello(Hello),
    Stage(Stage),
    Result(StepResult),
    Group(Group),
    Profile(Profile),
    Item(Item),
    Dotfile(Dotfile),
    Untracked(Untracked),
    Needs(Needs),
    Log(Log),
    Done(Done),
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Hello {
    pub api: u32,
    pub cmd: String,
    pub dry_run: bool,
    pub repo: String,
    pub log: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Stage {
    pub id: String,
    pub title: String,
    pub n: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Skip,
    Warn,
    Manual,
    Fail,
    #[default]
    #[serde(other)]
    Unknown,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Skip => "skipped",
            Status::Warn => "warning",
            Status::Manual => "manual",
            Status::Fail => "failed",
            Status::Unknown => "unknown",
        }
    }
}

/// A `result` event: the outcome of one step.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct StepResult {
    pub status: Status,
    pub stage: String,
    pub id: String,
    pub item: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Group {
    pub id: String,
    pub title: String,
    pub desc: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    Installed,
    Missing,
    Outside,
    Mismatch,
    #[default]
    #[serde(other)]
    Unknown,
}

impl ItemState {
    pub const ALL: [ItemState; 5] = [
        ItemState::Installed,
        ItemState::Missing,
        ItemState::Outside,
        ItemState::Mismatch,
        ItemState::Unknown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ItemState::Installed => "installed",
            ItemState::Missing => "missing",
            ItemState::Outside => "outside Homebrew",
            ItemState::Mismatch => "wrong app",
            ItemState::Unknown => "unknown",
        }
    }

    /// One-cell marker, so state never depends on colour alone.
    pub fn symbol(self) -> &'static str {
        match self {
            ItemState::Installed => "✓",
            ItemState::Missing => "✗",
            ItemState::Outside => "◆",
            ItemState::Mismatch => "!",
            ItemState::Unknown => "?",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Item {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub group: String,
    pub state: ItemState,
    pub desc: String,
    pub meta: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DotfileState {
    Pending,
    Differs,
    #[default]
    #[serde(other)]
    Unknown,
}

impl DotfileState {
    pub fn label(self) -> &'static str {
        match self {
            DotfileState::Pending => "pending",
            DotfileState::Differs => "differs",
            DotfileState::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Dotfile {
    pub id: String,
    pub path: String,
    pub state: DotfileState,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Untracked {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub hint: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Needs {
    pub id: String,
    pub kind: String,
    pub question: String,
    pub default: String,
    pub answer_with: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Log {
    pub level: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Done {
    pub ok: u32,
    pub skip: u32,
    pub warn: u32,
    pub manual: u32,
    pub fail: u32,
    pub exit: i32,
    pub log: String,
}

/// Parse one line of engine output.
pub fn parse_line(line: &str) -> Result<Event, String> {
    serde_json::from_str(line).map_err(|e| format!("{e}: {line}"))
}

/// Render a meta value for display (strings without quotes).
pub fn meta_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_types_and_ignores_extra_fields() {
        let e = parse_line(
            r#"{"type":"item","id":"brew:pkg-one","kind":"brew","name":"pkg-one","group":"group-a","state":"missing","desc":"","meta":{},"new_field":1}"#,
        )
        .unwrap();
        let Event::Item(item) = e else {
            panic!("not an item")
        };
        assert_eq!(item.state, ItemState::Missing);
        assert_eq!(item.group, "group-a");
    }

    #[test]
    fn unknown_type_and_values_are_not_fatal() {
        assert_eq!(
            parse_line(r#"{"type":"from_the_future","x":1}"#).unwrap(),
            Event::Other
        );
        let Event::Item(item) = parse_line(r#"{"type":"item","state":"teleported"}"#).unwrap()
        else {
            panic!()
        };
        assert_eq!(item.state, ItemState::Unknown);
        let Event::Result(r) = parse_line(r#"{"type":"result","status":"meh"}"#).unwrap() else {
            panic!()
        };
        assert_eq!(r.status, Status::Unknown);
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(parse_line("not json").is_err());
        assert!(parse_line(r#"{"no_type":1}"#).is_err());
    }
}
