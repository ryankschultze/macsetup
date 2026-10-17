//! Finding the setup engine: `$MACSETUP_REPO`, then `repo = "…"` in
//! `$XDG_CONFIG_HOME/macsetup/config.toml` (default `~/.config`), then the
//! parent of `chezmoi source-path`.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

/// Where a repo path came from, for error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Env,
    ConfigFile,
    Chezmoi,
}

impl Source {
    pub fn describe(self) -> &'static str {
        match self {
            Source::Env => "$MACSETUP_REPO",
            Source::ConfigFile => "config.toml",
            Source::Chezmoi => "chezmoi source-path",
        }
    }
}

#[derive(Deserialize)]
struct ConfigFile {
    repo: Option<String>,
}

/// Inputs to repo discovery, gathered from the environment by [`Inputs::from_env`].
pub struct Inputs {
    pub env_repo: Option<String>,
    pub config_file: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub chezmoi_source: Option<String>,
}

impl Inputs {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".config")));
        Inputs {
            env_repo: std::env::var("MACSETUP_REPO").ok(),
            config_file: config_dir.map(|d| d.join("macsetup").join("config.toml")),
            home,
            chezmoi_source: chezmoi_source_path(),
        }
    }
}

fn chezmoi_source_path() -> Option<String> {
    let out = Command::new("chezmoi").arg("source-path").output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn expand_tilde(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

/// The engine repo is valid when it holds an executable-looking `setup` file.
fn has_setup(repo: &Path) -> bool {
    repo.join("setup").is_file()
}

/// Find the engine repo. Each source is tried in order; a source that is set
/// but points somewhere without a `setup` file is an error rather than a
/// silent fall-through, so a typo in `$MACSETUP_REPO` is reported.
pub fn find_repo(inputs: &Inputs) -> Result<(PathBuf, Source), String> {
    let home = inputs.home.as_deref();

    if let Some(repo) = inputs.env_repo.as_deref().filter(|s| !s.is_empty()) {
        return checked(expand_tilde(repo, home), Source::Env);
    }

    if let Some(file) = inputs.config_file.as_ref().filter(|f| f.is_file()) {
        let text = std::fs::read_to_string(file)
            .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
        let parsed: ConfigFile =
            toml::from_str(&text).map_err(|e| format!("cannot parse {}: {e}", file.display()))?;
        if let Some(repo) = parsed.repo {
            return checked(expand_tilde(&repo, home), Source::ConfigFile);
        }
    }

    if let Some(source) = &inputs.chezmoi_source {
        // The chezmoi source dir may be a subfolder of the repo (`.chezmoiroot`),
        // so take the first of it and its parent that holds `setup`.
        let source = PathBuf::from(source);
        for candidate in [Some(source.as_path()), source.parent()]
            .into_iter()
            .flatten()
        {
            if has_setup(candidate) {
                return Ok((candidate.to_path_buf(), Source::Chezmoi));
            }
        }
    }

    Err(
        "no setup engine found. Set MACSETUP_REPO, add `repo = \"…\"` to \
         ~/.config/macsetup/config.toml, or initialise chezmoi from the engine repo."
            .to_string(),
    )
}

fn checked(repo: PathBuf, source: Source) -> Result<(PathBuf, Source), String> {
    if has_setup(&repo) {
        Ok((repo, source))
    } else {
        Err(format!(
            "{} points to {}, which has no `setup` file",
            source.describe(),
            repo.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    /// A fresh temp dir, removed when dropped.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "macsetup-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn engine_in(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("setup"), "#!/bin/sh\n").unwrap();
        dir.to_path_buf()
    }

    fn none() -> Inputs {
        Inputs {
            env_repo: None,
            config_file: None,
            home: None,
            chezmoi_source: None,
        }
    }

    #[test]
    fn env_wins() {
        let t = TempDir::new();
        let repo = engine_in(&t.0.join("engine"));
        let other = engine_in(&t.0.join("other"));
        let inputs = Inputs {
            env_repo: Some(repo.display().to_string()),
            chezmoi_source: Some(other.display().to_string()),
            ..none()
        };
        assert_eq!(find_repo(&inputs).unwrap(), (repo, Source::Env));
    }

    #[test]
    fn env_without_setup_is_an_error() {
        let t = TempDir::new();
        let inputs = Inputs {
            env_repo: Some(t.0.display().to_string()),
            ..none()
        };
        let err = find_repo(&inputs).unwrap_err();
        assert!(
            err.contains("$MACSETUP_REPO") && err.contains("no `setup` file"),
            "{err}"
        );
    }

    #[test]
    fn config_file_with_tilde() {
        let t = TempDir::new();
        let home = t.0.join("home");
        let repo = engine_in(&home.join("src/engine"));
        let cfg = t.0.join("config.toml");
        std::fs::write(&cfg, "repo = \"~/src/engine\"\n").unwrap();
        let inputs = Inputs {
            config_file: Some(cfg),
            home: Some(home),
            ..none()
        };
        assert_eq!(find_repo(&inputs).unwrap(), (repo, Source::ConfigFile));
    }

    #[test]
    fn config_file_without_repo_falls_through() {
        let t = TempDir::new();
        let repo = engine_in(&t.0.join("engine"));
        let cfg = t.0.join("config.toml");
        std::fs::write(&cfg, "# nothing yet\n").unwrap();
        let inputs = Inputs {
            config_file: Some(cfg),
            chezmoi_source: Some(repo.display().to_string()),
            ..none()
        };
        assert_eq!(find_repo(&inputs).unwrap(), (repo, Source::Chezmoi));
    }

    #[test]
    fn chezmoi_root_subfolder_uses_parent() {
        let t = TempDir::new();
        let repo = engine_in(&t.0.join("engine"));
        let source = repo.join("home");
        std::fs::create_dir_all(&source).unwrap();
        let inputs = Inputs {
            chezmoi_source: Some(source.display().to_string()),
            ..none()
        };
        assert_eq!(find_repo(&inputs).unwrap(), (repo, Source::Chezmoi));
    }

    #[test]
    fn nothing_found() {
        let err = find_repo(&none()).unwrap_err();
        assert!(err.contains("MACSETUP_REPO"), "{err}");
    }
}
