# The macsetup engine protocol (API 1)

`macsetup` never installs anything itself. It runs an engine, an executable called `setup`
at the root of a repo, and reads the events it prints. This file is the contract between the
two. The engine's own test suite checks its side of it.

## Output modes

| Flag | stdout | Human-readable text | Prompts |
|---|---|---|---|
| (none) | styled text | terminal | interactive |
| `--json` | NDJSON events only | the engine's log file only | none: each prompt becomes a `needs` event and takes the safe default |
| `--events PATH` | styled text | terminal | interactive |

`--events` exists for terminal handoffs. The dashboard leaves its screen, runs `setup` on the
terminal (for `sudo`, interactive merges, vendor install scripts), then reads `PATH`.

## Events

Every line is one JSON object with a `type`. The first line is always `hello` and the last is
always `done`, even after a failure. Unknown fields must be ignored by readers.

| type | Fields |
|---|---|
| `hello` | `api` (1), `cmd`, `dry_run`, `repo`, `log` |
| `stage` | `id`, `title`, `n`, `total` |
| `result` | `status` (ok, skip, warn, manual, fail), `stage`, `id`, `item`, `detail` |
| `group` | `id`, `title`, `desc`, `selected` |
| `profile` | `id`, `groups` (array) |
| `item` | `id`, `kind`, `name`, `group`, `state` (installed, missing, outside, mismatch, unknown), `desc`, `meta` (object of strings: url, app, bundle, …; `{}` when none) |
| `dotfile` | `id`, `path`, `state` (pending, differs) |
| `untracked` | `id`, `kind`, `name`, `hint` |
| `needs` | `id`, `kind` (confirm, input, choose, choose_many, pause), `question`, `default`, `answer_with` (the flag that answers it, or empty) |
| `log` | `level` (info, warn, error), `text` |
| `done` | `ok`, `skip`, `warn`, `manual`, `fail`, `exit`, `log` |

### IDs

IDs are `<kind>:<name>`, for example `brew:pkg-one`, `cask:app-one`, `mas:123456789`,
`npm:pkg-two`, `uv:pkg-three`, `cargo:pkg-four`, `bun:tool-one`, `vendor:app_two`,
`dotfile:~/.example-rc`, `repo:repo-one`, `group:group-a`.

### `setup list`

Prints `hello`, every `profile` and `group` (in file order), then every `item`, then `done`.
No stages and no results; it changes nothing.

- `item.kind`: brew, cask, mas, npm, uv, cargo, go, bun, vendor. Taps are not items.
- `item.state`: `installed`; `missing`; `outside` (a cask whose app, font or binary exists
  outside Homebrew, so the engine skips it); `mismatch` (an installed app with an unexpected
  bundle ID); `unknown` (the tool that would tell isn't installed).
- `item.desc`: a human description, possibly empty.
- `group.selected`: whether the group is in the saved selection.

### `dotfile` events

`setup check` emits one `dotfile` event per file that is `pending` (the engine would write it)
or `differs` (yours differs from the repo), right after that file's `result`.

### Readers

Readers ignore unknown fields and unknown event types, and treat an unknown `state` or
`status` value as unknown rather than failing. A run that ends without `done` failed.

## Commands

| Command | Notes |
|---|---|
| `setup api` | prints `{"api":1}` and exits; no `hello` |
| `setup list [--groups …]` | `group`, `profile` and `item` events |
| `setup check` | `stage` and `result` events, reported per group |
| `setup install --items brew:pkg-one,cask:app-one` | installs single items; never upgrades |
| `setup uninstall brew:pkg-one [--drop] [--commit]` | refuses when other installed packages depend on it, or when a cask's app was installed outside Homebrew; `--drop` also removes the entry from its group |
| `setup move cask:app-one --to group-b [--commit]` | moves an entry between groups, keeping its metadata |
| `setup capture --list` | `untracked` events |
| `setup capture --add brew:pkg-one --to group-a [--commit]` | adds an installed item to a group |
| `setup review --file ~/.example-rc --action keep\|take\|later\|merge` | one dotfile decision; `merge` needs a terminal (`--events`) |
| `setup groups --set group-a,group-b` | saves the selected groups |
| `setup drop brew:pkg-one [--commit]` | removes an entry from its group; nothing is uninstalled |

`--dry-run` works on every command that changes something.

The dashboard runs actions with `--json` in the background. Installs and uninstalls that
include `cask:`, `mas:` or `vendor:` items, and `review --action merge`, run attached to the
terminal with `--events FILE` instead; the dashboard reads `FILE` afterwards. Group edits
(`uninstall --drop`, `move`, `drop`, `capture --add`) always pass `--commit`.

Exit codes: 0 when no `result` has status `fail`, 1 when any does, 2 for bad arguments.

## `--commit`

1. Before editing, the engine notes whether each file it will touch has uncommitted changes.
2. It makes the edit.
3. If every touched file was clean, it makes a local commit of only those files. Otherwise it
   emits a `warn` result naming the file and leaves the edit uncommitted.
4. It never pushes.

## Versioning

`api` changes only on breaking changes. The dashboard runs `setup api` at start and refuses
to run against a different version, saying which side to update.
