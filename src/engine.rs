//! Talking to the setup engine: `setup api` synchronously, long-running
//! commands (`setup list --json`, `setup check --json`, actions) on a reader
//! thread that sends each event back over a channel, and actions that need the
//! terminal run attached to it with `--events FILE`.

use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

use serde::Deserialize;

use crate::events::{Event, parse_line};
use crate::model::{JobError, JobKind, Model};

/// The engine API version this dashboard speaks.
pub const API: u32 = 1;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct ApiInfo {
    pub api: u32,
}

/// Parse the single JSON line `setup api` prints.
pub fn parse_api(stdout: &str) -> Result<ApiInfo, String> {
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    serde_json::from_str(line).map_err(|e| format!("unexpected `setup api` output {line:?}: {e}"))
}

/// Run `<repo>/setup api` and return the raw JSON and its parsed form.
pub fn api(repo: &Path) -> Result<(String, ApiInfo), String> {
    let setup = repo.join("setup");
    let out = Command::new(&setup)
        .arg("api")
        .output()
        .map_err(|e| format!("cannot run {}: {e}", setup.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let first = stderr.lines().next().unwrap_or("").trim();
        return Err(format!(
            "`setup api` failed (exit {}){}. This engine may predate the dashboard protocol; update it.",
            out.status
                .code()
                .map_or("signal".to_string(), |c| c.to_string()),
            if first.is_empty() {
                String::new()
            } else {
                format!(": {first}")
            }
        ));
    }
    let info = parse_api(&stdout)?;
    Ok((stdout.trim().to_string(), info))
}

/// What a running job reports.
#[derive(Debug)]
pub enum Msg {
    Event(Event),
    /// A stdout line that wasn't a valid event.
    BadLine(String),
    /// The job ended: `Ok` when the engine sent its final `done` event.
    Finished(Result<(), JobError>),
}

/// A [`Msg`] tagged with the job it belongs to, so late messages from a
/// replaced job can be ignored.
#[derive(Debug)]
pub struct Envelope {
    pub job: u64,
    pub kind: JobKind,
    pub msg: Msg,
}

/// Start `<repo>/setup <kind's args> --json`; see [`spawn_args`].
pub fn spawn(repo: &Path, kind: JobKind, job: u64, tx: Sender<Envelope>) -> Option<u32> {
    let args: Vec<String> = kind.args().iter().map(|s| s.to_string()).collect();
    spawn_args(repo, &args, kind, job, tx)
}

/// Start `<repo>/setup <args> --json` and stream its events to `tx`. Never
/// blocks: the process is read on its own thread. It runs in its own process
/// group, so [`cancel`] stops everything it started. Returns its pid.
pub fn spawn_args(
    repo: &Path,
    args: &[String],
    kind: JobKind,
    job: u64,
    tx: Sender<Envelope>,
) -> Option<u32> {
    let setup = repo.join("setup");
    let child = Command::new(&setup)
        .args(args)
        .arg("--json")
        .current_dir(repo)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn();
    match child {
        Ok(child) => {
            let pid = child.id();
            thread::spawn(move || read_job(child, kind, job, tx));
            Some(pid)
        }
        Err(e) => {
            let _ = tx.send(Envelope {
                job,
                kind,
                msg: Msg::Finished(Err(JobError::Spawn(format!("{}: {e}", setup.display())))),
            });
            None
        }
    }
}

/// Stop a job started by [`spawn_args`]: SIGTERM to its whole process group.
pub fn cancel(pid: u32) {
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(format!("-{pid}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Run `<repo>/setup <args> --events FILE` attached to this terminal (for
/// password prompts and merge tools) and wait for it.
pub fn run_attached(repo: &Path, args: &[String], events: &Path) -> std::io::Result<ExitStatus> {
    Command::new(repo.join("setup"))
        .args(args)
        .arg("--events")
        .arg(events)
        .current_dir(repo)
        .status()
}

/// The events a `--events FILE` run wrote, plus lines that weren't events.
pub fn read_events(path: &Path) -> (Vec<Event>, Vec<String>) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut events = Vec::new();
    let mut bad = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match parse_line(line) {
            Ok(e) => events.push(e),
            Err(_) => bad.push(line.to_string()),
        }
    }
    (events, bad)
}

fn read_job(mut child: Child, kind: JobKind, job: u64, tx: Sender<Envelope>) {
    let send = |msg| {
        let _ = tx.send(Envelope { job, kind, msg });
    };
    // stderr is drained on its own thread so a chatty engine can't fill the pipe and stall.
    let stderr = child.stderr.take().map(|mut err| {
        thread::spawn(move || {
            let mut s = String::new();
            let _ = err.read_to_string(&mut s);
            s
        })
    });
    let mut saw_done = false;
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match parse_line(&line) {
                Ok(event) => {
                    saw_done |= matches!(event, Event::Done(_));
                    send(Msg::Event(event));
                }
                Err(_) => send(Msg::BadLine(line)),
            }
        }
    }
    let status = child.wait().ok();
    let stderr = stderr.and_then(|t| t.join().ok()).unwrap_or_default();
    send(Msg::Finished(if saw_done {
        Ok(())
    } else {
        Err(JobError::NoDone {
            exit: status.and_then(|s| s.code()),
            stderr,
        })
    }));
}

/// Run one job to completion, applying its events to `model`. For tests and
/// the `--snapshot` dev flag; the dashboard itself never blocks on a job.
pub fn run_blocking(repo: &Path, kind: JobKind, model: &mut Model) -> Result<(), JobError> {
    let (tx, rx) = std::sync::mpsc::channel();
    model.begin(kind);
    spawn(repo, kind, 0, tx);
    for env in rx {
        match env.msg {
            Msg::Event(e) => model.apply(kind, e),
            Msg::BadLine(l) => model.bad_line(kind, l),
            Msg::Finished(r) => {
                if let Err(e) = &r {
                    model.fail(kind, e);
                }
                return r;
            }
        }
    }
    Err(JobError::NoDone {
        exit: None,
        stderr: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_line() {
        assert_eq!(parse_api("{\"api\":1}\n").unwrap(), ApiInfo { api: 1 });
    }

    #[test]
    fn ignores_blank_lines_and_extra_fields() {
        assert_eq!(
            parse_api("\n {\"api\":2,\"x\":true}\n").unwrap(),
            ApiInfo { api: 2 }
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_api("setup: unknown argument 'api'").is_err());
        assert!(parse_api("").is_err());
    }
}
