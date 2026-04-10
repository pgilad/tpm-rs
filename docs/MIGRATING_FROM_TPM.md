# Migrating From Legacy TPM

Use this guide if your current setup still uses the original shell TPM with
`set -g @plugin` lines in `tmux.conf` and a final
`run '~/.tmux/plugins/tpm/tpm'` bootstrap.

The migration path is:

1. run `tpm migrate` to generate `tpm.yaml`
2. run `tpm install` to clone the migrated plugins
3. update `tmux.conf` to load plugins with `tpm load`

## Before

A typical legacy TPM config looks like this:

```tmux
set -g @plugin 'tmux-plugins/tpm'
set -g @plugin 'tmux-plugins/tmux-sensible'
set -g @plugin 'tmux-plugins/tmux-resurrect'

run '~/.tmux/plugins/tpm/tpm'
```

In `tpm-rs`, plugin declarations move into `tpm.yaml`, and `tmux.conf` only
needs to run `tpm load`.

## 1. Run `tpm migrate`

If your tmux config is in a standard location, run:

```bash
tpm migrate
```

By default, `tpm migrate` checks `~/.tmux.conf`, `~/.tmux`,
`~/.tmux/tmux.conf`, and `${XDG_CONFIG_HOME:-$HOME/.config}/tmux/tmux.conf`.

If you keep it somewhere else, point `tpm` at it directly:

```bash
tpm migrate --tmux-conf /path/to/tmux.conf
```

The command:

- keeps plugin order
- skips the legacy manager entry `tmux-plugins/tpm`
- writes a new `tpm.yaml`
- leaves `tmux.conf` unchanged so you can review it first

Example output:

```text
Detected and parsed 2 plugin(s) correctly
Skipped 0 source-file directive(s); multi-file tmux configs are not supported
Wrote tpm.yaml to /Users/alice/.config/tpm/tpm.yaml
Did not modify /Users/alice/.tmux.conf
You may still need to replace the legacy TPM bootstrap with `run-shell "tpm load"` at the end of the file
```

Generated `tpm.yaml`:

```yaml
version: 1
plugins:
  - source: tmux-plugins/tmux-sensible
  - source: tmux-plugins/tmux-resurrect
```

## 2. Run `tpm install`

Install the migrated plugin set:

```bash
tpm install
```

At this point the plugins exist on disk, but tmux is still using the old
bootstrap until you update `tmux.conf`.

## 3. Update `tmux.conf`

After migration, `tpm.yaml` is the source of truth. Remove the legacy TPM
manager and old `@plugin` lines from `tmux.conf`, then keep only the new
loader at the end of the file.

Before:

```tmux
set -g @plugin 'tmux-plugins/tpm'
set -g @plugin 'tmux-plugins/tmux-sensible'
set -g @plugin 'tmux-plugins/tmux-resurrect'

run '~/.tmux/plugins/tpm/tpm'
```

After:

```tmux
# your normal tmux settings stay here

run-shell "tpm load"
```

Keep `run-shell "tpm load"` at the end of `tmux.conf`, the same way legacy TPM
expected its bootstrap to be last.

Reload tmux or restart the server after the edit:

```bash
tmux source-file /path/to/tmux.conf
```

## What Changes After Migration

- `tpm.yaml` replaces `set -g @plugin ...` lines as the source of truth.
- `tpm load` is offline-only and only loads already-installed plugins.
- `tpm install`, `tpm update`, `tpm cleanup`, `tpm add`, and `tpm remove`
  replace the legacy shell scripts and tmux-driven TPM workflows.
- `tmux-plugins/tpm` is no longer managed as a plugin.

Optional TPM-like tmux bindings:

```tmux
bind I run-shell "tpm install"
bind U run-shell "tpm update"
bind M-u run-shell "tpm cleanup"
```

## If `tpm migrate` Needs Help

- If `tpm.yaml` already exists, `tpm migrate` refuses to overwrite it.
- If your config uses `source-file`, `tpm migrate` reports the skipped files but
  does not merge them automatically.
- If no `@plugin` lines are detected, run
  `tpm migrate --tmux-conf /path/to/tmux.conf` against the file that contains
  the plugin declarations directly.
