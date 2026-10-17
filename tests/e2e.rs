//! End to end against a real engine: the dashboard's own command builder and
//! engine client, run manually against a throwaway copy of an engine repo.
//!
//! It installs nothing itself: `cowsay` must already be installed and absent
//! from every group. It adds it to a group, then uninstalls and drops it.
//!
//! ```sh
//! MACSETUP_E2E_REPO=/path/to/throwaway/engine cargo test --test e2e -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::mpsc;

use macsetup::action::Action;
use macsetup::app::Job;
use macsetup::engine::{self, Msg};
use macsetup::events::{Event, ItemState, Status};
use macsetup::model::{JobKind, Model};

fn repo() -> PathBuf {
    PathBuf::from(std::env::var("MACSETUP_E2E_REPO").expect("set MACSETUP_E2E_REPO"))
}

/// Run an action the way the dashboard does and return its results.
fn run(action: Action) -> Vec<(Status, String, String)> {
    let job = Job {
        kind: JobKind::Action,
        action: Some(action),
        dry_run: false,
    };
    assert!(!job.attached(), "this test only runs background actions");
    println!("$ {}", job.command());
    let (tx, rx) = mpsc::channel();
    engine::spawn_args(&repo(), &job.args(), JobKind::Action, 1, tx).expect("spawned");
    let mut results = Vec::new();
    for env in rx {
        match env.msg {
            Msg::Event(Event::Result(r)) => {
                println!("  {:?} {} — {}", r.status, r.item, r.detail);
                results.push((r.status, r.id, r.detail));
            }
            Msg::Finished(r) => {
                r.expect("the engine finished with a done event");
                break;
            }
            _ => {}
        }
    }
    results
}

fn ok(results: &[(Status, String, String)], id: &str) -> bool {
    results.iter().any(|(s, i, _)| *s == Status::Ok && i == id)
}

#[test]
#[ignore = "changes this Mac: run by hand against a throwaway engine copy"]
fn add_then_uninstall_and_drop_cowsay() {
    let add = run(Action::CaptureAdd {
        id: "brew:cowsay".into(),
        to: "cli-extras".into(),
    });
    assert!(ok(&add, "brew:cowsay") && ok(&add, "commit"), "{add:?}");

    let mut m = Model::default();
    engine::run_blocking(&repo(), JobKind::List, &mut m).unwrap();
    let item = m
        .items
        .iter()
        .find(|i| i.id == "brew:cowsay")
        .expect("listed");
    assert_eq!(
        (item.group.as_str(), item.state),
        ("cli-extras", ItemState::Installed)
    );

    let gone = run(Action::Uninstall {
        ids: vec!["brew:cowsay".into()],
        drop: true,
    });
    assert!(ok(&gone, "brew:cowsay") && ok(&gone, "commit"), "{gone:?}");

    engine::run_blocking(&repo(), JobKind::List, &mut m).unwrap();
    assert!(
        m.items.iter().all(|i| i.id != "brew:cowsay"),
        "dropped from the repo"
    );
}

/// Saving groups from the Profiles tab writes the same `groups=` line the
/// wizard writes for the same selection. Uses a scratch state folder, never
/// the real one:
///
/// ```sh
/// MACSETUP_E2E_REPO=/path/to/throwaway/engine MACSETUP_E2E_STATE=/empty/scratch/dir \
///   cargo test --test e2e saving_groups -- --ignored --nocapture
/// ```
#[test]
#[ignore = "runs the real engine: run by hand against a throwaway engine copy"]
fn saving_groups_matches_the_wizard() {
    use macsetup::app::{App, Screen};
    use macsetup::theme::Theme;
    use std::collections::BTreeSet;

    let state = PathBuf::from(std::env::var("MACSETUP_E2E_STATE").expect("set MACSETUP_E2E_STATE"));
    let (dash, wiz) = (state.join("dashboard"), state.join("wizard"));
    // SAFETY: run by name and alone; nothing else in this process reads the
    // environment while it changes. The engine children inherit it.
    unsafe { std::env::set_var("XDG_STATE_HOME", &dash) };

    // The groups as the Profiles tab would save them: picked out of order.
    let mut app = App::new(Screen::Main, Theme::plain(), None);
    engine::run_blocking(&repo(), JobKind::List, &mut app.model).unwrap();
    let picked: BTreeSet<String> = ["ai", "cli-extras", "core"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    app.pending_groups = Some(picked.clone());
    let groups = app.in_group_order(&picked);
    let saved = run(Action::SetGroups { groups });
    assert!(ok(&saved, "groups"), "{saved:?}");

    // The wizard, same selection given out of order, running no stage at all.
    let status = std::process::Command::new(repo().join("setup"))
        .args(["--yes", "--groups", "ai,cli-extras", "--only", "none"])
        .env("XDG_STATE_HOME", &wiz)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());

    let line = |dir: &PathBuf| {
        std::fs::read_to_string(dir.join("macsetup/state"))
            .unwrap()
            .lines()
            .find(|l| l.starts_with("groups="))
            .map(str::to_string)
            .expect("a groups= line")
    };
    println!("dashboard: {}\nwizard:    {}", line(&dash), line(&wiz));
    assert_eq!(line(&dash), line(&wiz));
}
