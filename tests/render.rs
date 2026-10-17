//! Rendered frames, snapshotted with insta (text only; colours aren't compared).
//! Review changes with `cargo insta review`.

mod common;

use std::path::PathBuf;

use macsetup::app::{App, FirstRun, Screen, Tab};
use macsetup::model::{JobError, JobKind};
use macsetup::ui;
use ratatui::{Terminal, backend::TestBackend};

fn render(app: &App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::draw(f, app)).unwrap();
    format!("{}", t.backend())
}

fn tab(mut app: App, tab: Tab) -> App {
    app.tab = tab;
    app
}

#[test]
fn overview_loaded() {
    insta::assert_snapshot!(render(&common::loaded(), 100, 30));
}

#[test]
fn packages_loaded_with_a_group_open() {
    let mut app = tab(common::loaded(), Tab::Packages);
    app.expanded.insert("group-a".into());
    app.pkg_sel = 3; // cask app-one: shows its metadata
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn packages_filtered_to_missing() {
    let mut app = tab(common::loaded(), Tab::Packages);
    app.state_filter = Some(macsetup::events::ItemState::Missing);
    app.text_filter = "tool".into();
    app.pkg_sel = 1;
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn packages_narrow_stacks_the_detail_pane() {
    let mut app = tab(common::loaded(), Tab::Packages);
    app.expanded.insert("group-b".into());
    app.pkg_sel = 2;
    insta::assert_snapshot!(render(&app, 60, 24));
}

#[test]
fn dotfiles_loaded() {
    insta::assert_snapshot!(render(&tab(common::loaded(), Tab::Dotfiles), 100, 30));
}

#[test]
fn overview_while_loading() {
    let mut app = common::connected();
    app.enqueue(JobKind::List);
    app.enqueue(JobKind::Check);
    app.take_next_job();
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn packages_empty() {
    let mut app = tab(common::connected(), Tab::Packages);
    common::load(&mut app.model, JobKind::List, "list-empty.ndjson");
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn packages_error() {
    let mut app = tab(common::connected(), Tab::Packages);
    let err = JobError::NoDone {
        exit: Some(2),
        stderr: "setup: unknown argument 'list'".into(),
    };
    app.model.fail(JobKind::List, &err);
    app.last_error = Some(format!("list: {err}"));
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn dotfiles_all_in_sync() {
    let mut app = tab(common::connected(), Tab::Dotfiles);
    common::load(&mut app.model, JobKind::List, "list.ndjson");
    app.model.begin(JobKind::Check);
    app.model.check.done = Some(Default::default());
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn dotfiles_not_set_up_says_so_instead_of_all_match() {
    let mut app = tab(common::connected(), Tab::Dotfiles);
    app.model.begin(JobKind::Check);
    for line in [
        r#"{"type":"result","status":"warn","stage":"dotfiles","id":"","item":"dotfiles","detail":"not configured yet (run setup)"}"#,
        r#"{"type":"done","ok":0,"skip":0,"warn":1,"manual":0,"fail":0,"exit":0,"log":""}"#,
    ] {
        app.model
            .apply(JobKind::Check, macsetup::events::parse_line(line).unwrap());
    }
    let frame = render(&app, 100, 30);
    assert!(frame.contains("dotfiles: not configured yet (run setup)"));
    assert!(!frame.contains("All dotfiles match"));
    insta::assert_snapshot!(frame);
}

#[test]
fn dotfiles_check_failed() {
    let mut app = tab(common::connected(), Tab::Dotfiles);
    app.model
        .fail(JobKind::Check, &JobError::Spawn("permission denied".into()));
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn help_overlay() {
    let mut app = common::loaded();
    app.show_help = true;
    insta::assert_snapshot!(render(&app, 100, 30));
}

fn first_run(input: &str, error: Option<&str>) -> App {
    let mut app = common::connected();
    app.repo = None;
    app.api = None;
    app.screen = Screen::FirstRun(FirstRun {
        input: input.into(),
        error: error.map(Into::into),
        config_file: PathBuf::from("/home/someone/.config/macsetup/config.toml"),
    });
    app
}

#[test]
fn first_run_screen() {
    insta::assert_snapshot!(render(&first_run("~/src/eng", None), 100, 30));
}

#[test]
fn first_run_with_a_bad_path() {
    insta::assert_snapshot!(render(
        &first_run(
            "~/nowhere",
            Some("/home/someone/nowhere has no executable `setup` file")
        ),
        100,
        30
    ));
}

#[test]
fn api_mismatch() {
    let mut app = common::connected();
    app.fatal(
        "Engine API mismatch",
        "The engine at ~/src/engine speaks API 2, this dashboard speaks API 1. Update whichever is older: `brew upgrade macsetup` for the dashboard, or pull the engine repo.",
    );
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn too_small() {
    insta::assert_snapshot!(render(&common::loaded(), 50, 12));
}

// --- phase 5: actions ---------------------------------------------------------

use macsetup::action::Action;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// Packages tab, group A open, cursor on pkg-one.
fn on_pkg_one() -> App {
    let mut app = tab(common::loaded(), Tab::Packages);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Down);
    app
}

#[test]
fn confirm_dialog() {
    let mut app = on_pkg_one();
    press(&mut app, KeyCode::Char('U'));
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn confirm_dialog_with_a_dry_run() {
    let mut app = on_pkg_one();
    press(&mut app, KeyCode::Char('U'));
    press(&mut app, KeyCode::Char('D'));
    common::run_next(&mut app, "action-dry.ndjson");
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn confirm_dialog_for_a_cask_runs_in_the_terminal() {
    let mut app = on_pkg_one();
    app.dialog = Some(macsetup::app::Dialog {
        action: Action::Install {
            ids: vec!["cask:app-one".into()],
        },
        preview: None,
    });
    insta::assert_snapshot!(render(&app, 80, 24));
}

#[test]
fn group_picker() {
    let mut app = on_pkg_one();
    press(&mut app, KeyCode::Char('m'));
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn capture_loaded() {
    let mut app = tab(common::loaded(), Tab::Capture);
    app.cap_sel = 1;
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn capture_empty() {
    let mut app = tab(common::loaded(), Tab::Capture);
    app.model.begin(JobKind::CaptureList);
    app.model.capture.done = Some(Default::default());
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn activity_empty() {
    insta::assert_snapshot!(render(&tab(common::loaded(), Tab::Activity), 100, 30));
}

fn after_an_action() -> App {
    let mut app = on_pkg_one();
    press(&mut app, KeyCode::Char('U'));
    press(&mut app, KeyCode::Enter);
    common::run_next(&mut app, "action.ndjson");
    app
}

#[test]
fn activity_after_an_action() {
    let mut app = after_an_action();
    app.tab = Tab::Activity;
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn activity_narrow() {
    let mut app = after_an_action();
    app.tab = Tab::Activity;
    insta::assert_snapshot!(render(&app, 60, 24));
}

#[test]
fn toast_after_an_action() {
    let app = after_an_action();
    let frame = render(&app, 100, 30);
    assert!(frame.contains("✓ Uninstall and drop brew:pkg-one: 3 ok"));
    insta::assert_snapshot!(frame);
}

#[test]
fn profiles_loaded() {
    insta::assert_snapshot!(render(&common::profiles(), 100, 30));
}

#[test]
fn profiles_with_pending_changes() {
    let mut app = common::profiles();
    app.pending_groups = Some(
        ["core", "group-a", "group-c"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    app.grp_sel = 3;
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn profiles_reset_dialog() {
    let mut app = common::profiles();
    app.on_key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Char('R'),
        ratatui::crossterm::event::KeyModifiers::NONE,
    ));
    insta::assert_snapshot!(render(&app, 100, 30));
}

#[test]
fn profiles_narrow() {
    let mut app = common::profiles();
    app.prof_focus = macsetup::app::ProfilesFocus::Profiles;
    insta::assert_snapshot!(render(&app, 60, 24));
}
