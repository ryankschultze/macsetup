//! Every fixture is a well-formed event stream: each line parses into a known
//! event, the first is `hello` and the last is `done`.

use std::fs;

use macsetup::events::{Event, parse_line};

#[test]
fn fixtures_are_well_formed_event_streams() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let mut seen = 0;
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("ndjson") {
            continue;
        }
        seen += 1;
        let text = fs::read_to_string(&path).unwrap();
        let events: Vec<Event> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| parse_line(l).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
            .collect();
        assert!(
            matches!(events.first(), Some(Event::Hello(_))),
            "{} must start with hello",
            path.display()
        );
        assert!(
            matches!(events.last(), Some(Event::Done(_))),
            "{} must end with done",
            path.display()
        );
        assert!(
            !events.contains(&Event::Other),
            "{} has an event type this dashboard doesn't know",
            path.display()
        );
    }
    assert!(seen >= 3, "fixtures missing");
}

#[test]
fn list_fixture_covers_every_kind_and_state() {
    let text = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/list.ndjson"
    ))
    .unwrap();
    let items: Vec<_> = text
        .lines()
        .filter_map(|l| match parse_line(l).unwrap() {
            Event::Item(i) => Some(i),
            _ => None,
        })
        .collect();
    for kind in [
        "brew", "cask", "mas", "npm", "uv", "cargo", "go", "bun", "vendor",
    ] {
        assert!(items.iter().any(|i| i.kind == kind), "no {kind} item");
    }
    for state in macsetup::events::ItemState::ALL {
        assert!(items.iter().any(|i| i.state == state), "no {state:?} item");
    }
}
