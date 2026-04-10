# Migrating From Shell TPM

`tpm-rs` keeps compatibility with existing tmux plugin repositories, but the
operating model changes from shell-driven bootstrap to an explicit CLI plus a
declarative `tpm.yaml`.

This guide covers:

- automatic migration with `tpm migrate`
- what the migration command reads and writes
- supported `@plugin` syntaxes
- follow-up manual changes in `tmux.conf`

## Automatic Migration

Use `tpm migrate` to create `tpm.yaml` from an existing tmux config:

```bash
tpm migrate
```

You can also point it at a specific file:

```bash
tpm migrate --tmux-conf ~/.tmux.conf
```

The command writes a new `tpm.yaml` to the resolved TPM config path:

- `--config PATH`, if provided
- `TPM_CONFIG_FILE`, if set
- `TPM_CONFIG_DIR/tpm.yaml`, if set
- otherwise `${XDG_CONFIG_HOME:-$HOME/.config}/tpm/tpm.yaml`

If `tpm.yaml` already exists, migration stops and refuses to overwrite it.

## How `tpm migrate` Finds `tmux.conf`

If `--tmux-conf PATH` is not provided, `tpm migrate` checks these locations in
order and uses the first existing regular file:

1. `~/.tmux.conf`
2. `~/.tmux`
3. `~/.tmux/tmux.conf`
4. `${XDG_CONFIG_HOME:-$HOME/.config}/tmux/tmux.conf`

If none of those files exist, the command fails and prints the checked paths.

## What The Command Does

`tpm migrate`:

- reads exactly one tmux config file
- extracts plugin declarations from `set -g @plugin ...` and
  `set-option -g @plugin ...` lines
- preserves plugin order from that file
- drops the legacy manager plugin `tmux-plugins/tpm`
- converts local plugin paths to absolute paths before writing `tpm.yaml`
- writes a brand new deterministic `tpm.yaml`
- prints a short summary at the end, including the absolute path of the written
  `tpm.yaml`

`tpm migrate` does not:

- modify `tmux.conf`
- overwrite an existing `tpm.yaml`
- follow or merge `source-file` includes
- parse inline tmux command sequences that use `;` or `\;`; those fail loudly so plugin declarations are not skipped silently

When it sees `source-file`, it skips that line and warns at the end that
multi-file tmux configs are not supported by the migration command. The summary
also prints the skipped line number and resolved path for each skipped
`source-file` directive so you can merge those plugin declarations manually.

## Supported `@plugin` Syntax

The migration command understands these plugin source forms:

- GitHub shorthand: `owner/repo`
- GitHub shorthand with a suffix: `owner/repo#branch`
- GitHub shorthand with a version-like suffix that starts with `v` and then a digit: `owner/repo#v0.3.6`
- full Git URLs
- SSH Git URLs
- scp-style Git URLs such as `git@github.com:user/plugin`
- local relative paths
- local absolute paths
- `~/...` local paths

Examples:

```tmux
set -g @plugin 'tmux-plugins/tmux-sensible'
set -g @plugin 'catppuccin/tmux#v2.1.3'
set -g @plugin 'tmux-plugins/tmux-resurrect#stable'
set -g @plugin 'git@github.com:user/plugin'
set -g @plugin 'git@bitbucket.com:user/plugin'
set -g @plugin 'https://github.com/user/plugin.git'
set -g @plugin './plugins/local-plugin'
```

How suffixes map into `tpm.yaml`:

- `#branch-name` becomes `branch: branch-name`
- `#v<digit>...` becomes `ref: v...`
- a 7-40 character hexadecimal suffix becomes `ref: <sha>`

Example conversion:

```tmux
set -g @plugin 'tmux-plugins/tmux-resurrect#stable'
set -g @plugin 'catppuccin/tmux#v2.1.3'
set -g @plugin './plugins/tmux-local'
```

becomes:

```yaml
version: 1
plugins:
  - source: tmux-plugins/tmux-resurrect
    branch: stable
  - source: catppuccin/tmux
    ref: v2.1.3
  - source: /absolute/path/to/plugins/tmux-local
```

## Covered Cases

`tpm migrate` works well when your root tmux config directly contains plugin
declarations such as:

- `set -g @plugin 'owner/repo'`
- `set -g @plugin 'owner/repo#branch'`
- `set-option -g @plugin 'git@github.com:user/plugin'`
- `set -g @plugin './local/plugin'`

It also handles:

- duplicate declarations that resolve to the same install directory by failing
  through normal config validation
- the old `tmux-plugins/tpm` manager entry by skipping it
- relative local paths by rewriting them as absolute paths

## Limitations And Caveats

The migration command is intentionally conservative.

- It reads only one file. `source-file` is skipped and only reported in the
  final summary.
- It expects plugin declarations and `source-file` directives to appear one per
  tmux command. Inline command sequences that use `;` or `\;` are rejected so
  migration does not silently miss later commands on the same line.
- Plugins declared in sourced files will not appear in the generated
  `tpm.yaml`.
- Conditional config, shell-generated config, or dynamically built plugin lists
  are not evaluated.
- `tmux.conf` is left unchanged, so you still need to update the bootstrap line
  yourself.
- Runtime tmux state is not used as the source of truth; migration is file
  based.

## Example Command Output

Successful migration prints a summary like:

```text
Detected and parsed 3 plugin(s) correctly
Skipped 1 source-file directive(s); multi-file tmux configs are not supported
Skipped source-file on line 6: /Users/alice/.config/tmux/extra.conf
Wrote tpm.yaml to /Users/alice/.config/tpm/tpm.yaml
Did not modify /Users/alice/.tmux.conf
You may still need to replace the legacy TPM bootstrap with `run-shell "tpm load"` at the end of the file
```

If a config already exists, the command refuses to overwrite it:

```text
error: config already exists at `/Users/alice/.config/tpm/tpm.yaml`; refusing to overwrite
```

## Manual Follow-Up

After `tpm migrate` succeeds:

1. Review the generated `tpm.yaml`.
2. Remove the legacy shell TPM bootstrap from `tmux.conf`:

```tmux
set -g @plugin 'tmux-plugins/tpm'
run '~/.tmux/plugins/tpm/tpm'
```

3. Replace it with:

```tmux
run-shell "tpm load"
```

Keep `run-shell "tpm load"` at the end of `tmux.conf`, the same way classic
TPM expects its bootstrap to be placed last.

4. Install plugins:

```bash
tpm install
```

## Behavioral Differences After Migration

- `tpm load` is strictly offline. It never installs or updates plugins during
  tmux startup.
- Plugins without `branch` or `ref` track the remote default branch.
- `branch` tracks a named remote branch, while `ref` pins a tag or commit SHA.
- `tpm cleanup` preserves an undeclared `plugins_dir/tpm` checkout so a staged
  migration does not delete a legacy TPM clone unexpectedly.
- YAML comments and original formatting are not preserved when commands rewrite
  `tpm.yaml`.

## Common Follow-Up Commands

```bash
tpm install
tpm update
tpm list
tpm paths
tpm doctor
```
