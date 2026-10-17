# macsetup

A terminal dashboard for a macOS setup engine. It shows what is installed, installs and
removes single items, moves them between groups, and resolves dotfiles that differ, by
driving an engine that speaks the [macsetup protocol](docs/contract.md).

The dashboard holds no package lists or settings of its own. It reads them from the engine
repo on your Mac at run time, and it makes no network calls.

> Status: v0.0.x stub. It finds the engine and checks its API version; the dashboard itself
> is in progress.

## Install

Apple Silicon Macs only.

```sh
brew install ryankschultze/tap/macsetup
```

## Finding the engine

`macsetup` looks for a repo with an executable `setup` at its root, in this order:

1. `$MACSETUP_REPO`
2. `repo = "…"` in `~/.config/macsetup/config.toml` (or `$XDG_CONFIG_HOME/macsetup/config.toml`)
3. the folder `chezmoi source-path` reports, or its parent

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
bash scripts/check-personal.sh   # fails on personal paths or names in tracked files
```

Test fixtures in `tests/fixtures/` are written by hand with made-up names. Never commit output
recorded from a real machine.

Releases: push a `v*` tag. CI builds the binary, publishes a GitHub Release, and updates the
formula in `ryankschultze/homebrew-tap`.

## License

MIT
