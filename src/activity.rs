//! The Activity tab's history: every job this session, with the events worth
//! reading (stages, results, logs, skipped questions) and how it ended.
//! Bulk data (items, groups, untracked installs, dotfiles) is only counted.

use crate::events::{Event, Status};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Stage(String),
    Result {
        status: Status,
        item: String,
        detail: String,
    },
    Log {
        level: String,
        text: String,
    },
    /// A prompt the engine couldn't ask with --json, and what it did instead.
    Skipped {
        question: String,
        took: String,
    },
    /// Output that wasn't a valid event.
    Raw(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The engine finished; `fail` results make it a failure.
    Done {
        ok: u32,
        skip: u32,
        warn: u32,
        manual: u32,
        fail: u32,
    },
    Error(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct JobLog {
    pub id: u64,
    pub title: String,
    /// The command as it was run, e.g. `setup uninstall brew:x --json`.
    pub command: String,
    pub dry_run: bool,
    pub lines: Vec<Line>,
    /// Items, groups and other bulk events, counted instead of listed.
    pub data_events: usize,
    pub log: String,
    pub outcome: Option<Outcome>,
}

impl JobLog {
    pub fn running(&self) -> bool {
        self.outcome.is_none()
    }

    /// Results that need attention, then the rest, for a one-line summary.
    pub fn results(&self) -> impl Iterator<Item = (&Status, &String, &String)> {
        self.lines.iter().filter_map(|l| match l {
            Line::Result {
                status,
                item,
                detail,
            } => Some((status, item, detail)),
            _ => None,
        })
    }

    /// e.g. "✓ 1 ok · ✗ 1 failed: brew:x — needed by: y".
    pub fn summary(&self) -> String {
        match &self.outcome {
            None => "running…".into(),
            Some(Outcome::Cancelled) => "cancelled".into(),
            Some(Outcome::Error(e)) => e.clone(),
            Some(Outcome::Done {
                ok,
                skip,
                warn,
                manual,
                fail,
            }) => {
                let mut parts = Vec::new();
                for (n, label) in [
                    (*ok, "ok"),
                    (*skip, "skipped"),
                    (*warn, "warnings"),
                    (*manual, "manual"),
                    (*fail, "failed"),
                ] {
                    if n > 0 {
                        parts.push(format!("{n} {label}"));
                    }
                }
                if parts.is_empty() {
                    parts.push("nothing to do".into());
                }
                let mut s = parts.join(" · ");
                let first_problem = self
                    .results()
                    .find(|(st, ..)| matches!(st, Status::Fail | Status::Manual | Status::Warn));
                if let Some((_, item, detail)) = first_problem {
                    s.push_str(&format!(
                        " — {item}: {}",
                        detail.lines().next().unwrap_or("")
                    ));
                }
                s
            }
        }
    }

    /// The worst status, for colouring the summary.
    pub fn status(&self) -> Status {
        match &self.outcome {
            None => Status::Unknown,
            Some(Outcome::Cancelled) => Status::Skip,
            Some(Outcome::Error(_)) => Status::Fail,
            Some(Outcome::Done {
                fail, warn, manual, ..
            }) => {
                if *fail > 0 {
                    Status::Fail
                } else if *warn > 0 || *manual > 0 {
                    Status::Warn
                } else {
                    Status::Ok
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct Activity {
    /// Oldest first.
    pub jobs: Vec<JobLog>,
}

impl Activity {
    pub fn start(&mut self, id: u64, title: String, command: String, dry_run: bool) {
        self.jobs.push(JobLog {
            id,
            title,
            command,
            dry_run,
            lines: Vec::new(),
            data_events: 0,
            log: String::new(),
            outcome: None,
        });
    }

    pub fn get(&self, id: u64) -> Option<&JobLog> {
        self.jobs.iter().find(|j| j.id == id)
    }

    fn job(&mut self, id: u64) -> Option<&mut JobLog> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    pub fn event(&mut self, id: u64, event: &Event) {
        let Some(job) = self.job(id) else { return };
        match event {
            Event::Hello(h) => job.log = h.log.clone(),
            Event::Stage(s) => job.lines.push(Line::Stage(s.title.clone())),
            Event::Result(r) => job.lines.push(Line::Result {
                status: r.status,
                item: r.item.clone(),
                detail: r.detail.clone(),
            }),
            Event::Log(l) => job.lines.push(Line::Log {
                level: l.level.clone(),
                text: l.text.clone(),
            }),
            Event::Needs(n) => job.lines.push(Line::Skipped {
                question: n.question.clone(),
                took: match n.kind.as_str() {
                    "confirm" => "answered no".into(),
                    "pause" => "didn't wait".into(),
                    _ if n.default.is_empty() => "left blank".into(),
                    _ => format!("took the default, {}", n.default),
                },
            }),
            Event::Done(d) => {
                if !d.log.is_empty() {
                    job.log = d.log.clone();
                }
                job.outcome = Some(Outcome::Done {
                    ok: d.ok,
                    skip: d.skip,
                    warn: d.warn,
                    manual: d.manual,
                    fail: d.fail,
                });
            }
            Event::Item(_)
            | Event::Group(_)
            | Event::Profile(_)
            | Event::Dotfile(_)
            | Event::Untracked(_) => job.data_events += 1,
            Event::Other => {}
        }
    }

    pub fn raw(&mut self, id: u64, line: String) {
        if let Some(job) = self.job(id) {
            job.lines.push(Line::Raw(line));
        }
    }

    /// The job ended. An error replaces a `done`-based outcome; cancellation
    /// wins over the error the killed process produces.
    pub fn finish(&mut self, id: u64, error: Option<String>) {
        let Some(job) = self.job(id) else { return };
        if matches!(job.outcome, Some(Outcome::Cancelled)) {
            return;
        }
        if let Some(e) = error {
            job.outcome = Some(Outcome::Error(e));
        } else if job.outcome.is_none() {
            job.outcome = Some(Outcome::Error("ended without a summary".into()));
        }
    }

    pub fn cancelled(&mut self, id: u64) {
        if let Some(job) = self.job(id) {
            job.outcome = Some(Outcome::Cancelled);
        }
    }
}
