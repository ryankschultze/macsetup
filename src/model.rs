//! What the dashboard knows, built only from engine events.

use std::collections::BTreeMap;

use crate::events::{
    Done, Dotfile, Event, Group, Hello, Item, ItemState, Log, Needs, Profile, Status, StepResult,
};

/// The engine commands the dashboard runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    List,
    Check,
}

impl JobKind {
    pub fn args(self) -> &'static [&'static str] {
        match self {
            JobKind::List => &["list"],
            JobKind::Check => &["check"],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            JobKind::List => "list",
            JobKind::Check => "check",
        }
    }
}

/// How a job ended, when it ended badly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobError {
    /// The engine couldn't be started.
    Spawn(String),
    /// The engine exited without its final `done` event.
    NoDone { exit: Option<i32>, stderr: String },
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobError::Spawn(e) => write!(f, "couldn't start the engine: {e}"),
            JobError::NoDone { exit, stderr } => {
                let code = exit.map_or("a signal".to_string(), |c| format!("exit {c}"));
                write!(f, "the engine stopped early ({code})")?;
                if let Some(first) = stderr.lines().map(str::trim).find(|l| !l.is_empty()) {
                    write!(f, ": {first}")?;
                }
                Ok(())
            }
        }
    }
}

/// Per-job outcome kept for display.
#[derive(Debug, Clone, Default)]
pub struct JobRecord {
    pub hello: Option<Hello>,
    pub done: Option<Done>,
    pub error: Option<String>,
    /// Lines that weren't valid events.
    pub bad_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub installed: usize,
    pub missing: usize,
    pub outside: usize,
    pub mismatch: usize,
    pub unknown: usize,
}

impl Counts {
    pub fn add(&mut self, state: ItemState) {
        match state {
            ItemState::Installed => self.installed += 1,
            ItemState::Missing => self.missing += 1,
            ItemState::Outside => self.outside += 1,
            ItemState::Mismatch => self.mismatch += 1,
            ItemState::Unknown => self.unknown += 1,
        }
    }

    pub fn total(&self) -> usize {
        self.installed + self.missing + self.outside + self.mismatch + self.unknown
    }

    pub fn get(&self, state: ItemState) -> usize {
        match state {
            ItemState::Installed => self.installed,
            ItemState::Missing => self.missing,
            ItemState::Outside => self.outside,
            ItemState::Mismatch => self.mismatch,
            ItemState::Unknown => self.unknown,
        }
    }
}

#[derive(Debug, Default)]
pub struct Model {
    pub profiles: Vec<Profile>,
    pub groups: Vec<Group>,
    pub items: Vec<Item>,
    pub results: Vec<StepResult>,
    pub dotfiles: Vec<Dotfile>,
    pub needs: Vec<Needs>,
    pub logs: Vec<Log>,
    pub list: JobRecord,
    pub check: JobRecord,
}

impl Model {
    /// Clear what a job is about to replace.
    pub fn begin(&mut self, kind: JobKind) {
        match kind {
            JobKind::List => {
                self.profiles.clear();
                self.groups.clear();
                self.items.clear();
                self.list = JobRecord::default();
            }
            JobKind::Check => {
                self.results.clear();
                self.dotfiles.clear();
                self.needs.clear();
                self.logs.clear();
                self.check = JobRecord::default();
            }
        }
    }

    fn record(&mut self, kind: JobKind) -> &mut JobRecord {
        match kind {
            JobKind::List => &mut self.list,
            JobKind::Check => &mut self.check,
        }
    }

    pub fn apply(&mut self, kind: JobKind, event: Event) {
        match event {
            Event::Hello(h) => self.record(kind).hello = Some(h),
            Event::Done(d) => self.record(kind).done = Some(d),
            Event::Profile(p) => self.profiles.push(p),
            Event::Group(g) => self.groups.push(g),
            Event::Item(i) => self.items.push(i),
            Event::Result(r) => self.results.push(r),
            Event::Dotfile(d) => self.dotfiles.push(d),
            Event::Needs(n) => self.needs.push(n),
            Event::Log(l) => self.logs.push(l),
            Event::Stage(_) | Event::Untracked(_) | Event::Other => {}
        }
    }

    pub fn bad_line(&mut self, kind: JobKind, line: String) {
        self.record(kind).bad_lines.push(line);
    }

    pub fn fail(&mut self, kind: JobKind, err: &JobError) {
        self.record(kind).error = Some(err.to_string());
    }

    /// Item counts per group id.
    pub fn counts(&self) -> BTreeMap<&str, Counts> {
        let mut out: BTreeMap<&str, Counts> = BTreeMap::new();
        for item in &self.items {
            out.entry(item.group.as_str()).or_default().add(item.state);
        }
        out
    }

    pub fn total_counts(&self) -> Counts {
        let mut c = Counts::default();
        for item in &self.items {
            c.add(item.state);
        }
        c
    }

    /// Group ids in display order: known groups first, then any group an item
    /// names that the engine didn't describe.
    pub fn group_order(&self) -> Vec<&str> {
        let mut order: Vec<&str> = self.groups.iter().map(|g| g.id.as_str()).collect();
        for item in &self.items {
            if !order.contains(&item.group.as_str()) {
                order.push(&item.group);
            }
        }
        order
    }

    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// The `check` result for a whole group (id `group:<id>`).
    pub fn group_result(&self, id: &str) -> Option<&StepResult> {
        let key = format!("group:{id}");
        self.results.iter().find(|r| r.id == key)
    }

    /// Results for the overview's status list: repos, secrets and GitHub
    /// identity, plus any failure or manual step that no other view shows.
    pub fn status_results(&self) -> impl Iterator<Item = &StepResult> {
        self.results.iter().filter(|r| {
            let shown_elsewhere = r.id.starts_with("group:") || r.id.starts_with("dotfile:");
            ["identity:", "repo:", "secret:"]
                .iter()
                .any(|p| r.id.starts_with(p))
                || (!shown_elsewhere && matches!(r.status, Status::Fail | Status::Manual))
        })
    }

    /// Warnings, failures and manual steps the check reported for a stage.
    pub fn stage_problems<'a>(&'a self, stage: &'a str) -> impl Iterator<Item = &'a StepResult> {
        self.results.iter().filter(move |r| {
            r.stage == stage && matches!(r.status, Status::Warn | Status::Fail | Status::Manual)
        })
    }

    pub fn worst_status(&self) -> Option<Status> {
        let rank = |s: Status| match s {
            Status::Fail => 4,
            Status::Warn => 3,
            Status::Manual => 2,
            Status::Ok | Status::Skip => 1,
            Status::Unknown => 0,
        };
        self.results
            .iter()
            .map(|r| r.status)
            .max_by_key(|s| rank(*s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::parse_line;

    fn model(lines: &[&str]) -> Model {
        let mut m = Model::default();
        for l in lines {
            m.apply(JobKind::List, parse_line(l).unwrap());
        }
        m
    }

    #[test]
    fn counts_per_group_and_total() {
        let m = model(&[
            r#"{"type":"group","id":"group-a"}"#,
            r#"{"type":"item","id":"brew:a","group":"group-a","state":"installed"}"#,
            r#"{"type":"item","id":"brew:b","group":"group-a","state":"missing"}"#,
            r#"{"type":"item","id":"cask:c","group":"group-a","state":"outside"}"#,
            r#"{"type":"item","id":"cask:d","group":"group-b","state":"mismatch"}"#,
            r#"{"type":"item","id":"mas:e","group":"group-b","state":"weird"}"#,
        ]);
        let counts = m.counts();
        assert_eq!(
            counts["group-a"],
            Counts {
                installed: 1,
                missing: 1,
                outside: 1,
                mismatch: 0,
                unknown: 0
            }
        );
        assert_eq!(counts["group-b"].mismatch, 1);
        assert_eq!(counts["group-b"].unknown, 1);
        assert_eq!(m.total_counts().total(), 5);
        // group-b has items but no group event: it's still listed, after the known ones.
        assert_eq!(m.group_order(), vec!["group-a", "group-b"]);
    }

    #[test]
    fn begin_clears_only_what_the_job_replaces() {
        let mut m = model(&[r#"{"type":"item","id":"brew:a","group":"g","state":"installed"}"#]);
        m.apply(
            JobKind::Check,
            parse_line(r#"{"type":"result","status":"ok","id":"group:g"}"#).unwrap(),
        );
        m.begin(JobKind::Check);
        assert!(m.results.is_empty());
        assert_eq!(m.items.len(), 1);
        m.begin(JobKind::List);
        assert!(m.items.is_empty());
    }

    #[test]
    fn job_error_messages() {
        let e = JobError::NoDone {
            exit: Some(1),
            stderr: "\n boom \nmore".into(),
        };
        assert_eq!(e.to_string(), "the engine stopped early (exit 1): boom");
    }
}
