//! Talking to the setup engine.

use std::path::Path;
use std::process::Command;

use serde::Deserialize;

/// The engine API version this dashboard speaks.
pub const API: u32 = 1;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct ApiInfo {
    pub api: u32,
}

/// Parse the single JSON line `setup api` prints.
pub fn parse_api(stdout: &str) -> Result<ApiInfo, String> {
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    serde_json::from_str(line).map_err(|e| format!("unexpected `setup api` output {line:?}: {e}"))
}

/// Run `<repo>/setup api` and return the raw JSON and its parsed form.
pub fn api(repo: &Path) -> Result<(String, ApiInfo), String> {
    let setup = repo.join("setup");
    let out = Command::new(&setup)
        .arg("api")
        .output()
        .map_err(|e| format!("cannot run {}: {e}", setup.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let first = stderr.lines().next().unwrap_or("").trim();
        return Err(format!(
            "`setup api` failed (exit {}){}. This engine may predate the dashboard protocol; update it.",
            out.status
                .code()
                .map_or("signal".to_string(), |c| c.to_string()),
            if first.is_empty() {
                String::new()
            } else {
                format!(": {first}")
            }
        ));
    }
    let info = parse_api(&stdout)?;
    Ok((stdout.trim().to_string(), info))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_line() {
        assert_eq!(parse_api("{\"api\":1}\n").unwrap(), ApiInfo { api: 1 });
    }

    #[test]
    fn ignores_blank_lines_and_extra_fields() {
        assert_eq!(
            parse_api("\n {\"api\":2,\"x\":true}\n").unwrap(),
            ApiInfo { api: 2 }
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_api("setup: unknown argument 'api'").is_err());
        assert!(parse_api("").is_err());
    }
}
