//! Shared test helpers: load hand-written fixtures into an `App`.
#![allow(dead_code)]

use std::path::PathBuf;

use macsetup::app::{App, Screen};
use macsetup::events::parse_line;
use macsetup::model::{JobKind, Model};
use macsetup::theme::Theme;

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn fake_engine(dir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(dir)
}

pub fn load(model: &mut Model, kind: JobKind, name: &str) {
    model.begin(kind);
    let text = std::fs::read_to_string(fixture_path(name)).unwrap();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        model.apply(kind, parse_line(line).unwrap());
    }
}

/// A connected dashboard with no data yet (nothing queued).
pub fn connected() -> App {
    let mut app = App::new(
        Screen::Main,
        Theme::plain(),
        Some(PathBuf::from("/home/someone")),
    );
    app.repo = Some(PathBuf::from("/home/someone/src/engine"));
    app.api = Some(1);
    app
}

/// A dashboard showing the `list` and `check` fixtures.
pub fn loaded() -> App {
    let mut app = connected();
    load(&mut app.model, JobKind::List, "list.ndjson");
    load(&mut app.model, JobKind::Check, "check.ndjson");
    app
}
