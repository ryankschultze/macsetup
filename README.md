# macsetup

A terminal dashboard for a macOS setup engine. It shows what is installed, installs and
removes single items, moves them between groups, and resolves dotfiles that differ, by
driving an engine that speaks the [macsetup protocol](docs/contract.md).

The dashboard holds no package lists or settings of its own. It reads them from the engine
repo on your Mac at run time, and it makes no network calls.

> Status: v0.2. Packages, untracked installs and differing dotfiles, with actions on each.
> Editing groups and profiles arrives in a later version.

## Install

Apple Silicon Macs only.

```sh
brew install ryankschultze/tap/macsetup
```

## Using it

Run `macsetup`. It loads the package list and the installs no Brewfile has, then runs a
check in the background.

| Key | Does |
|---|---|
| `1`–`5`, Tab | Overview, Packages, Capture, Dotfiles, Activity |
| ↑ ↓, `j` `k` | move (arrows only on Dotfiles, where `k` means keep) |
| Enter, Space, → ← | open or close a group (Packages) |
| `f` | filter by state: missing, outside Homebrew, wrong app, unknown, installed |
| `/` | filter by text; Enter keeps it, Esc clears it |
| `r` | reload the package list and untracked installs |
| `c` | run a check |
| `?` | help and the state legend |
| `q`, Esc | quit (Esc first clears a filter) |

Actions. Each one opens a dialog showing the exact `setup` command it will run; press Enter
to run it, `D` to see a dry run first, or Esc to cancel.

| Where | Key | Runs |
|---|---|---|
| Packages | `i` | `setup install --items ID` (on a group: everything missing in it) |
| Packages | `u` / `U` | `setup uninstall ID`, or with `--drop --commit` to also remove it from its group |
| Packages | `m` | `setup move ID --to GROUP --commit` (pick the group) |
| Packages | `d` | `setup drop ID --commit` (nothing is uninstalled) |
| Capture | `a` | `setup capture --add ID --to GROUP --commit` |
| Dotfiles | `k` `t` `l` `m` | `setup review --file PATH --action keep\|take\|later\|merge` |
| Overview | `l` | open the latest check log |
| Activity | `x` / `y` | cancel the running job / copy its log path |

Edits to group package lists always pass `--commit`: the engine commits the files it changed
(never pushes) and leaves an edit uncommitted if that file already had changes.

Most actions run in the background and report on the Activity tab and the status line.
Installs and uninstalls that include a cask, App Store app or vendor app, and dotfile merges,
run in the terminal itself so you can answer password prompts or use the merge tool; press any
key afterwards to return to the dashboard.

Every state has a symbol as well as a colour (`✓` installed, `✗` missing, `◆` outside
Homebrew, `!` wrong app, `?` unknown). Set `NO_COLOR=1` to turn colour off.

## Finding the engine

`macsetup` looks for a repo with an executable `setup` at its root, in this order:

1. `--repo PATH`
2. `$MACSETUP_REPO`
3. `repo = "…"` in `~/.config/macsetup/config.toml` (or `$XDG_CONFIG_HOME/macsetup/config.toml`)
4. the folder `chezmoi source-path` reports, or its parent

When none of these finds an engine, `macsetup` asks for the path and saves it to
`config.toml` (it never replaces an existing one).

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
bash scripts/check-personal.sh   # fails on personal paths or names in tracked files
```

Test fixtures in `tests/fixtures/` are written by hand with made-up names. Never commit output
recorded from a real machine. `tests/fake-engine*/setup` are stand-in engines that print those
fixtures. Rendered frames are snapshot-tested with insta: `cargo insta review` after a UI change.

`macsetup --snapshot DIR [--size 100x30]` runs `list`, `capture --list` and `check` against the real engine and
writes a text render of each tab to `DIR` instead of starting the UI. It's for checking a real
setup by eye; keep its output out of the repo.

Releases: push a `v*` tag. CI builds the binary, publishes a GitHub Release, and updates the
formula in `ryankschultze/homebrew-tap`.

## License

MIT
