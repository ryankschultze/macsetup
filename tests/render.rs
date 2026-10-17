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
