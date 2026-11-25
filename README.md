# bizkit

Self-hosted business hub for small software companies: analytics, forms, webhooks and mailing

## Installation

Download a binary from the [releases page](https://github.com/hay-kot/bizkit/releases), or build from source:

```sh
cargo install --locked --git https://github.com/hay-kot/bizkit
```

## Usage

```sh
bizkit --help
bizkit serve          # API, MCP endpoint and web app on 127.0.0.1:8484
bizkit migrate        # run database migrations and exit
bizkit openapi        # print the OpenAPI document
```

Sign-in uses emailed magic links. Configure who may have an account with email policies (below). Without `server.mail.smtp_url`, mail goes to the log, so the first sign-in link shows up in the server output.

Agents reach every workspace operation as an MCP tool at `/mcp` with an API key: `Authorization: Bearer bk_...`.

Shell completions:

```sh
bizkit completions zsh > ~/.zfunc/_bizkit
```

### Configuration

bizkit reads `~/.config/bizkit/config.yaml` (or `$XDG_CONFIG_HOME/bizkit/config.yaml`) when it exists. Pass `--config <path>` to use a different file. Flags take precedence over the file.

```yaml
log_level: info
server:
  listen: 127.0.0.1:8484            # --listen / BIZKIT_LISTEN
  base_url: https://hub.example.com # --base-url / BIZKIT_BASE_URL; http only on loopback
  data_dir: ~/.local/share/bizkit   # --data-dir / BIZKIT_DATA_DIR
  trusted_proxies: 0                # proxies in front that append to X-Forwarded-For
  mail:
    from: "bizkit <noreply@example.com>"
    smtp_url: smtps://user@mail.example.com:465  # --smtp-url / BIZKIT_SMTP_URL
    smtp_password_env: BIZKIT_SMTP_PASSWORD
  auth:
    session_ttl_days: 30
    policies:                       # first match wins; with none, only invited emails can sign up
      - name: team
        matches: ["*@example.com"]  # case-insensitive globs
        sign_up: true
        join:                       # once, when the account is created; existing workspaces only
          - { workspace: acme, role: member }
```

## Development

Tools come from [mise](https://mise.jdx.dev), and Rust comes from [rustup](https://rustup.rs) at the version in `rust-toolchain.toml`.

```sh
mise install       # install the dev tools
mise run check     # everything CI runs
mise tasks         # list all tasks
```

`mise run check` runs these checks, and CI runs the same task:

| Task | Tool | What it checks |
|---|---|---|
| `fmt:check` | rustfmt | Formatting, per `rustfmt.toml` |
| `lint` | clippy | Lints from `[workspace.lints]` in `Cargo.toml`, with warnings as errors, with and without `test-hooks` |
| `test` | cargo test | Unit, integration and CLI tests in every crate |
| `docs` | rustdoc | Doc comments build without warnings |
| `deny` | [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) | Security advisories, licenses and crate sources, per `deny.toml` |
| `tidy` | [cargo-shear](https://github.com/Boshen/cargo-shear) | Dependencies in `Cargo.toml` that the code never uses |
| `typos` | [typos](https://github.com/crate-ci/typos) | Spelling |
| `db:check` | sqlx-cli | The committed `.sqlx/` query data matches the queries |
| `gen:check` | bizkit, hey-api | `openapi.json` and `web/src/api/gen` are current |
| `web:check` | eslint, stylelint, vue-tsc, vitest | The web app |

`mise run test:e2e` runs the Playwright suite against a fresh server and Mailpit. `svx up` starts a dev stack (Mailpit, API, Vite) on this checkout's own ports; see `AGENTS.md`.

`mise run coverage` writes an `lcov.info` coverage report with [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov).

### Tests

- Unit tests live next to the code in `#[cfg(test)]` modules. Commands write to a `&mut impl Write`, so a test passes a `Vec<u8>` and checks what was written.
- CLI tests in `crates/bizkit/tests/cli.rs` run the built binary with [assert_cmd](https://docs.rs/assert_cmd).
- API tests in `crates/bizkit-testkit/tests/` run a real app with fakes and check every response against the OpenAPI document.
- [insta](https://insta.rs) is included so snapshot tests are easy to add later. Today it backs only the small inline snapshots in the command tests. It earns its place on large outputs such as help text, JSON, or rendered files. When output changes on purpose, run `cargo insta review` to accept the new snapshot instead of editing the expected text by hand.

### Dependencies

[Renovate](https://docs.renovatebot.com) keeps dependencies current (install the Renovate GitHub app on the repository to turn it on). `renovate.json` sets these rules:

- Crates: `[workspace.dependencies]` in `Cargo.toml` names versions at least 7 days old, and pins with `=` where a newer release is too fresh. One weekly PR groups them, and a weekly lockfile refresh picks up transitive updates.
- The Rust toolchain in `rust-toolchain.toml`, the dev tools in `mise.toml`, and GitHub Actions each get grouped non-major PRs.
- GitHub Actions are pinned to commit SHAs with the release tag as a comment.
- Updates wait 7 days after a release. Security fixes from OSV advisories (which include RustSec) skip the wait and the schedule.

### Adding a command

```sh
scaffold new command
```

This generates `crates/bizkit/src/commands/<name>.rs` and registers the command in `cli.rs`, `commands.rs` and `main.rs` next to it.

### Releases

Run the **Build and Publish** workflow from the Actions tab and pick a semver bump. It tags the release and runs GoReleaser, which cross-compiles with [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild). Releases build on Linux. The macOS binaries link against zig's bundled macOS stubs, which covers pure-Rust code; a dependency that links Apple frameworks needs a macOS SDK (set `SDKROOT`).

`mise run build` builds a release binary for this machine into `dist/`. GoReleaser runs `mise run web:build` first, because the binary embeds `web/dist`.
