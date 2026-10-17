//! The engine client against stand-in engines in tests/fake-engine*.

mod common;

use std::sync::mpsc;

use macsetup::engine::{self, Msg};
use macsetup::model::{JobError, JobKind, Model};

#[test]
fn api_check() {
    let (_, info) = engine::api(&common::fake_engine("fake-engine")).unwrap();
    assert_eq!(info.api, engine::API);
}

#[test]
fn list_and_check_stream_into_the_model() {
    let repo = common::fake_engine("fake-engine");
    let mut m = Model::default();
    engine::run_blocking(&repo, JobKind::List, &mut m).unwrap();
    assert_eq!(m.groups.len(), 3);
    assert_eq!(m.items.len(), 11);
    assert_eq!(m.profiles.len(), 2);
    assert!(m.list.done.is_some() && m.list.error.is_none());

    engine::run_blocking(&repo, JobKind::Check, &mut m).unwrap();
    assert_eq!(m.dotfiles.len(), 2);
    assert_eq!(m.needs.len(), 1);
    assert_eq!(m.check.done.as_ref().unwrap().fail, 1);
    // the check didn't wipe what list loaded
    assert_eq!(m.items.len(), 11);
}

#[test]
fn a_run_without_done_is_an_error_with_the_engines_stderr() {
    let repo = common::fake_engine("fake-engine-abort");
    let mut m = Model::default();
    let err = engine::run_blocking(&repo, JobKind::Check, &mut m).unwrap_err();
    assert_eq!(
        err,
        JobError::NoDone {
            exit: Some(1),
            stderr: "engine blew up\n".into()
        }
    );
    assert!(
        m.check.hello.is_some(),
        "events before the failure are kept"
    );
    assert_eq!(m.check.bad_lines, vec!["this is not json".to_string()]);
    assert!(m.check.error.as_deref().unwrap().contains("engine blew up"));
}

#[test]
fn a_missing_engine_is_a_spawn_error() {
    let (tx, rx) = mpsc::channel();
    engine::spawn(
        std::path::Path::new("/nonexistent/engine"),
        JobKind::List,
        7,
        tx,
    );
    let env = rx.recv().unwrap();
    assert_eq!(env.job, 7);
    assert!(matches!(env.msg, Msg::Finished(Err(JobError::Spawn(_)))));
}

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn an_action_streams_its_events() {
    let repo = common::fake_engine("fake-engine");
    let (tx, rx) = mpsc::channel();
    let pid = engine::spawn_args(
        &repo,
        &args(&["uninstall", "brew:pkg-one", "--drop", "--commit"]),
        JobKind::Action,
        3,
        tx,
    );
    assert!(pid.is_some());
    let msgs: Vec<Msg> = rx.iter().map(|e| e.msg).collect();
    let results = msgs
        .iter()
        .filter(|m| matches!(m, Msg::Event(macsetup::events::Event::Result(_))))
        .count();
    assert_eq!(results, 3);
    assert!(matches!(msgs.last(), Some(Msg::Finished(Ok(())))));
}

#[test]
fn an_attached_run_writes_its_events_to_the_file() {
    let repo = common::fake_engine("fake-engine");
    let dir = std::env::temp_dir().join(format!("macsetup-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("events.ndjson");
    let status = engine::run_attached(
        &repo,
        &args(&["review", "--file", "~/.example.conf", "--action", "merge"]),
        &file,
    )
    .unwrap();
    assert!(status.success());
    let (events, bad) = engine::read_events(&file);
    assert!(bad.is_empty());
    assert!(matches!(
        events.first(),
        Some(macsetup::events::Event::Hello(_))
    ));
    assert!(matches!(
        events.last(),
        Some(macsetup::events::Event::Done(_))
    ));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn cancel_stops_the_whole_job_quickly() {
    let repo = common::fake_engine("fake-engine");
    let (tx, rx) = mpsc::channel();
    let pid = engine::spawn_args(&repo, &args(&["slow"]), JobKind::Action, 9, tx).unwrap();
    // wait for the first event so the engine is really running
    let first = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert!(matches!(first.msg, Msg::Event(_)));
    let started = std::time::Instant::now();
    engine::cancel(pid);
    let end = rx
        .iter()
        .find(|e| matches!(e.msg, Msg::Finished(_)))
        .unwrap();
    assert!(matches!(
        end.msg,
        Msg::Finished(Err(JobError::NoDone { .. }))
    ));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the sleep was killed too"
    );
}
