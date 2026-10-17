//! App state and keys, without a terminal.

mod common;

use macsetup::action::{Action, ReviewChoice};
use macsetup::activity::{Line, Outcome};
use macsetup::app::{Effect, PickFor, Row, Tab};
use macsetup::engine::{Envelope, Msg};
use macsetup::events::{Event, ItemState, parse_line};
use macsetup::model::JobKind;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(app: &mut macsetup::app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn typed(app: &mut macsetup::app::App, s: &str) {
    for c in s.chars() {
        key(app, KeyCode::Char(c));
    }
}

#[test]
fn groups_start_closed_and_open_with_enter() {
    let mut app = common::loaded();
    app.tab = Tab::Packages;
    assert_eq!(app.package_rows().len(), 3); // three closed groups
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.package_rows().len(), 3 + 6);
    key(&mut app, KeyCode::Down);
    assert!(matches!(app.package_rows()[app.pkg_sel], Row::Item(i) if i.name == "pkg-one"));
    // ← on an item closes its group and puts the cursor back on it
    key(&mut app, KeyCode::Left);
    assert_eq!(app.pkg_sel, 0);
    assert_eq!(app.package_rows().len(), 3);
}

#[test]
fn state_filter_cycles_and_shows_only_matches() {
    let mut app = common::loaded();
    app.tab = Tab::Packages;
    key(&mut app, KeyCode::Char('f'));
    assert_eq!(app.state_filter, Some(ItemState::Missing));
    let names: Vec<&str> = app
        .package_rows()
        .iter()
        .filter_map(|r| match r {
            Row::Item(i) => Some(i.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["pkg-two", "vendor_one", "tool-uv", "tool-bun"]);
    for _ in 0..5 {
        key(&mut app, KeyCode::Char('f'));
    }
    assert_eq!(app.state_filter, None, "the cycle comes back to all");
}

#[test]
fn text_filter_edits_then_esc_clears_before_quitting() {
    let mut app = common::loaded();
    app.tab = Tab::Packages;
    key(&mut app, KeyCode::Char('/'));
    typed(&mut app, "TOOL-c");
    key(&mut app, KeyCode::Enter);
    assert!(!app.editing_filter);
    assert_eq!(app.text_filter, "TOOL-c");
    assert_eq!(app.package_rows().len(), 2); // Group B + tool-cargo
    key(&mut app, KeyCode::Esc);
    assert!(app.text_filter.is_empty() && !app.quit);
    key(&mut app, KeyCode::Esc);
    assert!(app.quit);
}

#[test]
fn letters_typed_into_the_filter_are_not_commands() {
    let mut app = common::loaded();
    app.tab = Tab::Packages;
    key(&mut app, KeyCode::Char('/'));
    typed(&mut app, "q");
    assert!(!app.quit);
    assert_eq!(app.text_filter, "q");
}

#[test]
fn tabs_switch_by_number_and_tab_key() {
    let mut app = common::loaded();
    key(&mut app, KeyCode::Char('4'));
    assert_eq!(app.tab, Tab::Dotfiles);
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.tab, Tab::Activity);
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.tab, Tab::Overview);
    key(&mut app, KeyCode::BackTab);
    key(&mut app, KeyCode::BackTab);
    assert_eq!(app.tab, Tab::Dotfiles);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.dot_sel, 1);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.dot_sel, 1, "stops at the last row");
    key(&mut app, KeyCode::Up);
    assert_eq!(app.dot_sel, 0);
    key(&mut app, KeyCode::Char('3'));
    assert_eq!(app.tab, Tab::Capture);
}

#[test]
fn help_closes_on_any_key_without_acting() {
    let mut app = common::loaded();
    key(&mut app, KeyCode::Char('?'));
    assert!(app.show_help);
    key(&mut app, KeyCode::Char('q'));
    assert!(!app.show_help && !app.quit);
}

#[test]
fn one_job_at_a_time_without_duplicates() {
    let mut app = common::connected();
    app.connected(app.repo.clone().unwrap(), macsetup::config::Source::Env, 1);
    key(&mut app, KeyCode::Char('c')); // check is already queued
    assert_eq!(app.queue.len(), 3); // list, capture --list, check
    let (first, kind) = app.take_next_job().unwrap();
    assert_eq!(kind, JobKind::List);
    assert!(app.take_next_job().is_none(), "list is still running");
    key(&mut app, KeyCode::Char('r')); // list is running, capture --list queued: nothing added
    assert_eq!(app.queue.len(), 2);

    // a message from some other job is ignored
    app.on_engine(Envelope {
        job: first + 99,
        kind: JobKind::List,
        msg: Msg::Event(parse_line(r#"{"type":"group","id":"stale"}"#).unwrap()),
    });
    assert!(app.model.groups.is_empty());

    app.on_engine(Envelope {
        job: first,
        kind: JobKind::List,
        msg: Msg::Event(Event::Other),
    });
    app.on_engine(Envelope {
        job: first,
        kind: JobKind::List,
        msg: Msg::Finished(Ok(())),
    });
    assert_eq!(
        app.take_next_job().map(|(_, k)| k),
        Some(JobKind::CaptureList)
    );
}

#[test]
fn a_failed_job_sets_the_error() {
    let mut app = common::connected();
    app.enqueue(JobKind::Check);
    let (id, _) = app.take_next_job().unwrap();
    app.on_engine(Envelope {
        job: id,
        kind: JobKind::Check,
        msg: Msg::Finished(Err(macsetup::model::JobError::Spawn("nope".into()))),
    });
    assert!(app.running.is_none());
    assert_eq!(
        app.last_error.as_deref(),
        Some("check: couldn't start the engine: nope")
    );
}

#[test]
fn ctrl_c_always_quits_even_while_typing() {
    let mut app = common::loaded();
    app.tab = Tab::Packages;
    key(&mut app, KeyCode::Char('/'));
    app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.quit);
}

// --- actions ----------------------------------------------------------------

/// Packages tab with group A open and the cursor on `pkg-one`.
fn on_pkg_one() -> macsetup::app::App {
    let mut app = common::loaded();
    app.tab = Tab::Packages;
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Down);
    app
}

#[test]
fn an_action_asks_first_and_esc_cancels() {
    let mut app = on_pkg_one();
    key(&mut app, KeyCode::Char('U'));
    let d = app.dialog.as_ref().expect("a confirm dialog");
    assert_eq!(
        d.action,
        Action::Uninstall {
            ids: vec!["brew:pkg-one".into()],
            drop: true
        }
    );
    key(&mut app, KeyCode::Char('q')); // q closes the dialog, it doesn't quit
    assert!(app.dialog.is_none() && !app.quit && app.queue.is_empty());
}

#[test]
fn confirming_runs_the_action_then_refreshes_and_reports() {
    let mut app = on_pkg_one();
    key(&mut app, KeyCode::Char('u'));
    key(&mut app, KeyCode::Enter);
    assert!(app.dialog.is_none());
    let job = app.queue.front().unwrap().clone();
    assert_eq!(job.args(), ["uninstall", "brew:pkg-one"]);
    assert!(!job.attached(), "a formula runs in the background");

    let id = common::run_next(&mut app, "action.ndjson");
    // the list and the untracked installs reload
    let queued: Vec<JobKind> = app.queue.iter().map(|j| j.kind).collect();
    assert_eq!(queued, [JobKind::List, JobKind::CaptureList]);
    let toast = app.toast.as_ref().unwrap();
    assert!(
        toast.text.starts_with("Uninstall brew:pkg-one: 3 ok"),
        "{}",
        toast.text
    );
    let log = app.activity.get(id).unwrap();
    assert_eq!(
        log.outcome,
        Some(Outcome::Done {
            ok: 3,
            skip: 0,
            warn: 0,
            manual: 0,
            fail: 0
        })
    );
    // the engine's skipped question is shown, not lost
    assert!(log.lines.contains(&Line::Skipped {
        question: "Run it?".into(),
        took: "answered no".into()
    }));
    // an action's results don't replace the last check's
    assert!(app.model.results.iter().all(|r| r.id != "commit"));
}

#[test]
fn a_dry_run_previews_in_the_dialog_without_refreshing() {
    let mut app = on_pkg_one();
    key(&mut app, KeyCode::Char('U'));
    key(&mut app, KeyCode::Char('D'));
    assert!(app.previewing());
    key(&mut app, KeyCode::Enter); // can't confirm while the preview runs
    assert_eq!(app.queue.len(), 1);
    assert!(app.queue[0].dry_run);
    let id = common::run_next(&mut app, "action-dry.ndjson");
    let d = app.dialog.as_ref().expect("the dialog stays open");
    assert_eq!(d.preview, Some(Some(id)));
    assert!(
        app.queue.is_empty(),
        "a dry run changes nothing, so nothing reloads"
    );
    assert!(app.toast.is_none());
    key(&mut app, KeyCode::Enter);
    assert!(!app.queue[0].dry_run);
}

#[test]
fn install_on_a_group_installs_what_is_missing() {
    let mut app = common::loaded();
    app.tab = Tab::Packages;
    key(&mut app, KeyCode::Char('i'));
    assert_eq!(
        app.dialog.unwrap().action,
        Action::Install {
            ids: vec!["brew:pkg-two".into(), "vendor:vendor_one".into()]
        }
    );
}

#[test]
fn move_picks_another_group() {
    let mut app = on_pkg_one();
    key(&mut app, KeyCode::Char('m'));
    let p = app.picker.as_ref().unwrap();
    assert_eq!(
        p.purpose,
        PickFor::Move {
            id: "brew:pkg-one".into()
        }
    );
    assert_eq!(p.options, ["group-b", "group-c"], "not its own group");
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);
    assert!(app.picker.is_none());
    assert_eq!(
        app.dialog.unwrap().action,
        Action::Move {
            id: "brew:pkg-one".into(),
            to: "group-c".into()
        }
    );
}

#[test]
fn capture_adds_to_a_picked_group() {
    let mut app = common::loaded();
    app.tab = Tab::Capture;
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char('a'));
    assert_eq!(app.picker.as_ref().unwrap().options.len(), 3);
    key(&mut app, KeyCode::Enter);
    assert_eq!(
        app.dialog.unwrap().action,
        Action::CaptureAdd {
            id: "cask:extra-app".into(),
            to: "group-a".into()
        }
    );
}

#[test]
fn dotfile_keys_review_the_selected_file() {
    let mut app = common::loaded();
    app.tab = Tab::Dotfiles;
    key(&mut app, KeyCode::Char('k')); // keep, not "move up"
    assert_eq!(
        app.dialog.take().unwrap().action,
        Action::Review {
            file: "~/.example.conf".into(),
            choice: ReviewChoice::Keep
        }
    );
    key(&mut app, KeyCode::Char('m'));
    let merge = app.dialog.take().unwrap().action;
    assert!(merge.needs_terminal() && !merge.can_dry_run());
    // a pending file has nothing to review
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char('t'));
    assert!(app.dialog.is_none() && app.toast.is_some());
}

#[test]
fn cancel_kills_the_running_job_and_marks_it_cancelled() {
    use macsetup::engine::{Envelope, Msg};
    let mut app = on_pkg_one();
    key(&mut app, KeyCode::Char('u'));
    key(&mut app, KeyCode::Enter);
    let (id, kind) = app.take_next_job().unwrap();
    app.started(Some(4242));
    app.tab = Tab::Activity;
    key(&mut app, KeyCode::Char('x'));
    assert_eq!(app.effects, [Effect::Kill(4242)]);
    app.on_engine(Envelope {
        job: id,
        kind,
        msg: Msg::Finished(Err(macsetup::model::JobError::NoDone {
            exit: None,
            stderr: String::new(),
        })),
    });
    assert_eq!(
        app.activity.get(id).unwrap().outcome,
        Some(Outcome::Cancelled)
    );
    assert!(app.last_error.is_none(), "a cancel isn't an error");
}

#[test]
fn log_keys_copy_and_open() {
    let mut app = on_pkg_one();
    key(&mut app, KeyCode::Char('u'));
    key(&mut app, KeyCode::Enter);
    common::run_next(&mut app, "action.ndjson");
    app.tab = Tab::Activity;
    key(&mut app, KeyCode::Char('y'));
    assert_eq!(
        app.effects.pop(),
        Some(Effect::Copy(
            "/tmp/engine-logs/20260101-120003-uninstall.log".into()
        ))
    );
    app.tab = Tab::Overview;
    key(&mut app, KeyCode::Char('l'));
    assert_eq!(
        app.effects.pop(),
        Some(Effect::Open(
            "/tmp/engine-logs/20260101-120001-check.log".into()
        ))
    );
}
