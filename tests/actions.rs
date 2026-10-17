//! The exact `setup` arguments each dashboard action runs.

use macsetup::action::{Action, ReviewChoice};
use macsetup::app::Job;
use macsetup::model::JobKind;

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn argv(a: &Action, dry: bool) -> Vec<String> {
    a.argv(dry)
}

#[test]
fn install_and_uninstall() {
    let install = Action::Install {
        ids: ids(&["brew:pkg-two", "cask:app-one"]),
    };
    assert_eq!(
        argv(&install, false),
        ["install", "--items", "brew:pkg-two,cask:app-one"]
    );
    let keep = Action::Uninstall {
        ids: ids(&["brew:pkg-one"]),
        drop: false,
    };
    assert_eq!(argv(&keep, false), ["uninstall", "brew:pkg-one"]);
    let drop = Action::Uninstall {
        ids: ids(&["brew:pkg-one"]),
        drop: true,
    };
    assert_eq!(
        argv(&drop, false),
        ["uninstall", "brew:pkg-one", "--drop", "--commit"]
    );
    assert_eq!(
        argv(&drop, true),
        [
            "--dry-run",
            "uninstall",
            "brew:pkg-one",
            "--drop",
            "--commit"
        ]
    );
}

#[test]
fn brewfile_edits_always_commit() {
    let mv = Action::Move {
        id: "brew:pkg-one".into(),
        to: "group-b".into(),
    };
    assert_eq!(
        argv(&mv, false),
        ["move", "brew:pkg-one", "--to", "group-b", "--commit"]
    );
    let drop = Action::Drop {
        ids: ids(&["brew:pkg-one"]),
    };
    assert_eq!(argv(&drop, false), ["drop", "brew:pkg-one", "--commit"]);
    let add = Action::CaptureAdd {
        id: "cask:extra-app".into(),
        to: "group-a".into(),
    };
    assert_eq!(
        argv(&add, true),
        [
            "--dry-run",
            "capture",
            "--add",
            "cask:extra-app",
            "--to",
            "group-a",
            "--commit"
        ]
    );
}

#[test]
fn review_choices() {
    for (choice, word) in [
        (ReviewChoice::Keep, "keep"),
        (ReviewChoice::Take, "take"),
        (ReviewChoice::Later, "later"),
        (ReviewChoice::Merge, "merge"),
    ] {
        let a = Action::Review {
            file: "~/.example.conf".into(),
            choice,
        };
        assert_eq!(
            argv(&a, false),
            ["review", "--file", "~/.example.conf", "--action", word]
        );
    }
}

#[test]
fn what_runs_in_the_terminal() {
    let install = |v: &[&str]| Action::Install { ids: ids(v) };
    assert!(!install(&["brew:pkg-two", "npm:tool-npm"]).needs_terminal());
    assert!(install(&["brew:pkg-two", "cask:app-one"]).needs_terminal());
    assert!(install(&["mas:Store App"]).needs_terminal());
    assert!(install(&["vendor:vendor_one"]).needs_terminal());
    assert!(
        Action::Uninstall {
            ids: ids(&["cask:app-one"]),
            drop: false
        }
        .needs_terminal()
    );
    assert!(
        !Action::Move {
            id: "cask:app-one".into(),
            to: "group-b".into()
        }
        .needs_terminal()
    );
    let review = |choice| Action::Review {
        file: "~/.example.conf".into(),
        choice,
    };
    assert!(review(ReviewChoice::Merge).needs_terminal());
    assert!(!review(ReviewChoice::Merge).can_dry_run());
    assert!(!review(ReviewChoice::Take).needs_terminal());
    assert!(review(ReviewChoice::Take).can_dry_run());
}

#[test]
fn a_dry_run_never_takes_the_terminal() {
    let action = Action::Install {
        ids: ids(&["cask:app-one"]),
    };
    let real = Job {
        kind: JobKind::Action,
        action: Some(action.clone()),
        dry_run: false,
    };
    let dry = Job {
        dry_run: true,
        ..real.clone()
    };
    assert!(real.attached() && !dry.attached());
    assert_eq!(
        real.command(),
        "setup install --items cask:app-one --events FILE (in the terminal)"
    );
    assert_eq!(
        dry.command(),
        "setup --dry-run install --items cask:app-one --json"
    );
}
