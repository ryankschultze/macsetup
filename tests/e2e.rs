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
    let item = m.items.iter().find(|i| i.id == "brew:cowsay").expect("listed");
    assert_eq!((item.group.as_str(), item.state), ("cli-extras", ItemState::Installed));

    let gone = run(Action::Uninstall {
        ids: vec!["brew:cowsay".into()],
        drop: true,
    });
    assert!(ok(&gone, "brew:cowsay") && ok(&gone, "commit"), "{gone:?}");

    engine::run_blocking(&repo(), JobKind::List, &mut m).unwrap();
    assert!(m.items.iter().all(|i| i.id != "brew:cowsay"), "dropped from the repo");
}
