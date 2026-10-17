//! App state and keys, without a terminal.

mod common;

use macsetup::app::{Row, Tab};
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
    key(&mut app, KeyCode::Char('3'));
    assert_eq!(app.tab, Tab::Dotfiles);
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.tab, Tab::Overview);
    key(&mut app, KeyCode::BackTab);
    assert_eq!(app.tab, Tab::Dotfiles);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.dot_sel, 1);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.dot_sel, 1, "stops at the last row");
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
    assert_eq!(app.queue.len(), 2);
    let (first, kind) = app.take_next_job().unwrap();
    assert_eq!(kind, JobKind::List);
    assert!(app.take_next_job().is_none(), "list is still running");
    key(&mut app, KeyCode::Char('r')); // list is running: not queued again
    assert_eq!(app.queue.len(), 1);

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
    assert_eq!(app.take_next_job().map(|(_, k)| k), Some(JobKind::Check));
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
