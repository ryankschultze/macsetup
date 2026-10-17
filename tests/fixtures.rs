//! Every fixture line is a JSON object with a `type`, starting with `hello`
//! and ending with `done`.

use std::fs;

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
        let types: Vec<String> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let v: serde_json::Value = serde_json::from_str(l)
                    .unwrap_or_else(|e| panic!("{}: {e}: {l}", path.display()));
                v["type"]
                    .as_str()
                    .expect("every event has a type")
                    .to_string()
            })
            .collect();
        assert_eq!(
            types.first().map(String::as_str),
            Some("hello"),
            "{}",
            path.display()
        );
        assert_eq!(
            types.last().map(String::as_str),
            Some("done"),
            "{}",
            path.display()
        );
    }
    assert!(seen > 0, "no fixtures found");
}
