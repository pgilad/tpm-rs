# tpm

<p align="center"><strong>A small tmux plugin manager with explicit installs, predictable startup, and an XDG-first layout.</strong></p>

<p align="center"><code>tpm-rs</code> keeps the public command as <code>tpm</code>.</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="License: MIT" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.95.0-orange.svg" alt="Rust 1.95.0" /></a>
  <a href="https://github.com/tmux/tmux"><img src="https://img.shields.io/badge/tmux-3.2%2B-1BB91F.svg" alt="tmux 3.2+" /></a>
  <img src="https://img.shields.io/badge/layout-XDG--first-0A7EA4.svg" alt="XDG-first layout" />
</p>

`tpm` is a tmux plugin manager for people who want to try plugins without turning tmux startup into an installer.

TPM usually refers to the original shell-based tmux plugin manager. This project is a Rust replacement that keeps the public command as `tpm`, but moves plugin state into a declarative `tpm.yaml`.

It works with existing tmux plugin repositories: plugins are cloned as Git checkouts, and `tpm load` runs their executable root `*.tmux` entrypoints. Loading is predictable: enabled plugins run in `tpm.yaml` order, and each plugin's root entrypoints run in sorted filename order.

Migrating from legacy shell TPM? See [Migrating From Legacy TPM](./docs/MIGRATING_FROM_TPM.md).

## Why use tpm

- Declarative plugin management: `tpm.yaml` is the source of truth instead of a long list of `set -g @plugin` lines in `tmux.conf`.
- Safer tmux startup: `tpm load` is offline-only and never installs or updates plugins during startup.
- XDG-first filesystem layout: config, data, state, and cache all resolve through XDG locations by default.
- Automation-friendly output: `paths`, `list`, and `doctor` support `--json`, and `install` and `update` keep stable line-oriented stdout.
- Compatible plugin model: existing tmux plugin repos still work as Git checkouts with executable root `*.tmux` entrypoints.
- Better failure reporting: per-plugin failures are aggregated and surfaced clearly in stderr and tmux messages.

## Quick Start

New to tmux plugins? The steps below install two common plugins and load them from `tmux.conf`. If you are migrating from the original shell TPM, use [Migrating From Legacy TPM](./docs/MIGRATING_FROM_TPM.md).

### 1. Install `tpm`

Recommended install:

```bash
curl -fsSL https://github.com/pgilad/tpm-rs/releases/latest/download/install.sh | sh
```

The install script downloads the correct release archive for your platform, installs `tpm` into `~/.local/bin`, marks it executable, and tells you if `~/.local/bin` is not on your `PATH`.

Downloading `install.sh` from a specific release tag keeps that tag pinned by default. `TPM_INSTALL_VERSION` and `--version` still override it for manual installs.

GitHub releases publish `.tar.gz` archives for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, and `aarch64-apple-darwin`. On Apple Silicon, the installer prefers the native `arm64` archive even when the shell is running under Rosetta. musl-based Linux distributions such as Alpine are not covered by the published release assets; install from source there instead. Each archive contains `tpm`, `README.md`, `CHANGELOG.md`, and `LICENSE`.

You can also install from source:

```bash
cargo install --git https://github.com/pgilad/tpm-rs --bin tpm
```

Or build locally:

```bash
cargo build --release
```

Runtime requirements:

- `git` 2.25.0 or newer
- `tmux` 3.2 or newer

`tpm doctor` verifies both.

### 2. Create `tpm.yaml`

Create `${XDG_CONFIG_HOME:-$HOME/.config}/tpm/tpm.yaml`:

```yaml
version: 1
plugins:
  - source: tmux-plugins/tmux-sensible
  - source: tmux-plugins/tmux-resurrect
```

`tmux-sensible` applies widely used tmux defaults. `tmux-resurrect` can save and restore tmux sessions.

Or let `tpm` create it and install plugins for you:

```bash
tpm add tmux-plugins/tmux-sensible
tpm add tmux-plugins/tmux-resurrect
tpm add catppuccin/tmux --ref v2.1.3 --skip-install
```

### 3. Load plugins from `tmux.conf`

Add this line at the end of `tmux.conf`:

```tmux
run-shell "tpm load"
```

Plugin-specific tmux options still belong in `tmux.conf`. `tpm.yaml` only declares which plugins to install and load.

### 4. Install and inspect plugins

```bash
tpm install
tpm list
tpm paths
tpm doctor
```

Then reload your tmux config, or restart tmux:

```bash
tmux source-file ~/.tmux.conf
```

Common follow-up workflows:

```bash
tpm add tmux-plugins/tmux-yank
tpm update
tpm sync
tpm self-update
tpm remove tmux-plugins/tmux-yank
tpm cleanup
```

Optional tmux key bindings for a TPM-like workflow:

```tmux
bind I run-shell "tpm install"
bind U run-shell "tpm update"
bind M-u run-shell "tpm cleanup"
```

## Commands

| Command | Purpose |
|---|---|
| `load` | Run enabled installed plugin entrypoints inside tmux |
| `install` | Install configured plugins from `tpm.yaml` |
| `update [plugin...]` | Update installed plugins |
| `sync` | Remove stale managed plugin directories, install missing plugins, and update existing ones efficiently |
| `self-update` | Update the installed `tpm` binary from the latest release |
| `cleanup` | Remove stale managed plugin directories that are no longer declared |
| `list [--json]` | List configured plugins and installation state |
| `doctor [--json]` | Validate config, tool availability, and resolved paths |
| `add SOURCE [--branch BRANCH] [--ref REF] [--skip-install]` | Add a plugin to `tpm.yaml`, creating it if needed, and install it by default |
| `migrate [--tmux-conf PATH]` | Create `tpm.yaml` from an existing tmux config without modifying `tmux.conf` |
| `remove NAME` | Remove a plugin from `tpm.yaml` |
| `paths [--json]` | Print resolved config, data, state, cache, and plugins paths |

Global flags:

- `--config PATH` overrides the config file path.
- `--plugins-dir PATH` overrides the resolved plugin checkout directory.

## Configuration

Configuration is YAML only. If `--config`, `TPM_CONFIG_FILE`, and `TPM_CONFIG_DIR` are all omitted, `tpm` reads `$XDG_CONFIG_HOME/tpm/tpm.yaml`, or `~/.config/tpm/tpm.yaml` when `XDG_CONFIG_HOME` is unset.

Start from this shape:

```yaml
version: 1
paths:
  # Optional: overrides the default plugins directory.
  plugins: ~/.local/share/tpm/plugins
plugins:
  - source: tmux-plugins/tmux-sensible
  - source: tmux-plugins/tmux-resurrect
    branch: stable
  - source: tmux-plugins/tmux-yank
  - source: catppuccin/tmux
    ref: v2.1.3
  - source: ../local-plugin
    enabled: false
```

Branch and ref behavior:

- Omitting both `branch` and `ref` tracks the remote default branch.
- `branch` tracks a named remote branch and fast-forwards it on `tpm update`.
- `ref` pins a tag or commit SHA and keeps the checkout fixed to that exact object.
- `branch` and `ref` are mutually exclusive.

Accepted `source` formats:

- `owner/repo` GitHub shorthand
- full Git URL
- SSH Git URL
- absolute local path
- relative local path

For local paths that could be mistaken for `owner/repo`, use `./` or `../` so they stay unambiguous.

`tmux-plugins/tpm` is intentionally rejected in `tpm.yaml`; use the `tpm` CLI and
`run-shell "tpm load"` instead of managing the legacy shell TPM plugin.

Path behavior:

- `paths.plugins` can override the plugin checkout root.
- Relative `paths.plugins` values are resolved relative to the directory that contains `tpm.yaml`.
- Relative local plugin sources in `tpm.yaml` are resolved relative to the directory that contains `tpm.yaml`.
- Git-hosted plugin sources keep their namespace in the checkout path, so `tmux-plugins/tmux-sensible` installs into `${plugins_dir}/tmux-plugins/tmux-sensible`.

Default resolved locations:

- Config file: `$XDG_CONFIG_HOME/tpm/tpm.yaml`
- Data dir: `$XDG_DATA_HOME/tpm`
- State dir: `$XDG_STATE_HOME/tpm`
- Cache dir: `$XDG_CACHE_HOME/tpm`
- Plugins dir: `$XDG_DATA_HOME/tpm/plugins`

If XDG variables are unset, the usual `~/.config`, `~/.local/share`, `~/.local/state`, and `~/.cache` fallbacks apply.

Additional environment overrides:

- `TPM_CONFIG_FILE`
- `TPM_CONFIG_DIR`
- `TPM_DATA_DIR`
- `TPM_STATE_DIR`
- `TPM_CACHE_DIR`
- `TPM_PLUGINS_DIR`

Config rewrites are deterministic, but commands that rewrite `tpm.yaml` do not preserve YAML comments or original formatting.

## Managed Plugin Manifest

`tpm` tracks checkouts it installed or validated in `<plugins-dir>/.tpm-rs/managed.yaml`. `tpm sync` and `tpm cleanup` only delete stale directories recorded in that manifest, so manually added directories under the plugins directory are preserved.

`tpm install` and `tpm update` write manifest entries after installing, validating, or updating selected configured checkouts. `tpm sync` and `tpm cleanup` also adopt existing configured checkouts after validating that their Git remote matches `tpm.yaml`. A manually deleted configured checkout is reinstalled by `tpm sync` or `tpm install`; a manually added undeclared directory is left alone unless it is explicitly recorded in the managed manifest.

## Automation-Friendly Output

- `tpm paths --json`, `tpm list --json`, and `tpm doctor --json` emit pretty-printed JSON.
- `tpm install`, `tpm update`, and `tpm sync` emit stable line-oriented stdout that is suitable for scripts when stdout is not a terminal.
- Interactive `tpm install` now shows live per-plugin progress and a final summary on the terminal stream instead of waiting to print everything at the end.
- `tpm add` emits the normal `add` line followed by the `install` line for the added plugin.
- `tpm add --skip-install` only rewrites `tpm.yaml` and does not update the managed plugin manifest.
- `tpm self-update` emits stable line-oriented stdout for update and no-op outcomes.
- `tpm load` stays silent on success.
- When `tpm load` runs inside tmux, it overwrites a per-server log file at `${XDG_STATE_HOME:-$HOME/.local/state}/tpm/load-<sha256(socket-path)>.log` with plugin discovery, load events, and timing.
- `tpm install`, `tpm update`, `tpm sync`, and `tpm load` continue processing later selected plugins after an individual plugin failure, then exit with code `1` after printing a final summary line.

## Color Output

`tpm` only uses ANSI colors for human-oriented terminal output. Machine-oriented output such as `--json` stays uncolored.

Color can be disabled with any of these standard environment settings:

- `NO_COLOR=1` disables color output. This takes precedence over other color settings.
- `CLICOLOR=0` disables color output for terminal rendering.
- `TERM=dumb` disables color output.

You can also force color output with `CLICOLOR_FORCE=1`.

Typical `install` and `update` output:

```text
Installed tmux-plugins/tmux-sensible into /path/to/plugins/tmux-plugins/tmux-sensible
Skipped already installed tmux-plugins/tmux-sensible at /path/to/plugins/tmux-plugins/tmux-sensible
Updated tmux-plugins/tmux-sensible in /path/to/plugins/tmux-plugins/tmux-sensible
Already up to date tmux-plugins/tmux-sensible at /path/to/plugins/tmux-plugins/tmux-sensible
Kept pinned tmux-continuum at ref v1.0.0 in /path/to/plugins/tmux-continuum
Realigned pinned tmux-continuum to ref v1.0.0 in /path/to/plugins/tmux-continuum
Updated tpm from 2026.04.03-12 to 2026.04.04-1 at /path/to/tpm
Already up to date tpm 2026.04.04-1 at /path/to/tpm
```

Typical interactive `install` output:

```text
Installing 3 plugins into ~/.local/share/tpm/plugins
  [1/3] tmux-plugins/tmux-sensible... installed
  [2/3] tmux-plugins/tmux-resurrect... already installed
  [3/3] tmux-plugins/tmux-continuum... failed
         configured branch `stable` is not available as a remote branch
Done in 1.2s. 1 installed, 1 already installed, 1 failed.
```

Typical `load` failure output:

```text
Failed to load tmux-plugins/tmux-sensible: plugin checkout is missing at /path/to/plugins/tmux-plugins/tmux-sensible
Failed to load tmux-fail: entrypoint /path/to/plugins/tmux-fail/fail.tmux exited with status 7: boom
error: load reported 2 failed operations
```

When `tpm load` runs inside tmux, failures are also mirrored through `display-message`. A single failure includes the detail directly; multiple failures collapse to a count and point back to stderr.

## Development

This repository pins Rust in [`rust-toolchain.toml`](./rust-toolchain.toml) and uses [mise](https://mise.jdx.dev/) as a task runner.

```bash
# install rustup first if you do not already have it
mise run check
```

CI-parity Cargo commands:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

## License

MIT. See [LICENSE](./LICENSE).
