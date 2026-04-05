Use `rustup` with `rust-toolchain.toml` for Rust install & usage.
Treat CI parity as required: `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-targets`.
This is a CLI crate; user-facing behavior lives in `src/commands/*.rs`, and the public command/output contract is `tpm`.
When behavior changes, update or add integration coverage in `tests/*_cli.rs` before considering the task done.
Preserve the project’s XDG-first layout, `tpm.yaml` workflow, and stable machine-readable output (`--json` and line-oriented command output).
