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
