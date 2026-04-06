# Changelog

All notable changes to `stash` are documented in this file.

## 0.5.6

- Make `--print` require an explicit value to avoid ambiguity with
  `stash push [file]`.
- Save captured input on `Ctrl-C` in both `stash tee` and stdin-driven
  `stash` when `--save-on-error=true` applies.
- Align interruption handling more closely with `tee`:
  - `SIGINT` returns `130`
  - `SIGTERM` returns `143`
  - non-signal partial saves return `1`
- Treat downstream broken pipes as successful exits while still keeping any
  captured output.
- Keep `partial=true` only for input interruption/failure, not downstream
  output closure.

## 0.5.5

- Replace the earlier quiet flag with `--print=stdout|stderr|null` for
  `stash`, `stash push`, and `stash tee`, with numeric aliases `1`, `2`,
  and `0`.
- Rename `--partial` to `--save-on-error` and default it to `true`.

## 0.5.4

- Add smart default behavior for bare `stash`:
  - in the middle of a pipeline, it behaves like `stash tee`
  - otherwise, it behaves like `stash push`
- Simplify `stash attr` syntax:
  - `stash attr <ref>` lists all attributes
  - `stash attr <ref> key...` reads selected attributes
  - `stash attr <ref> key=value...` writes attributes
  - `stash attr <ref> --unset key...` removes attributes
- Add `stash rm -a` for attribute-based entry removal with confirmation.
- Allow `stash rm` to remove multiple refs in one command.
- Refine `ls` and `log` attribute flags:
  - `-a/--attr` selects attribute filters
  - `-A/--attrs` shows all attributes
- Add `cargo install stash-cli` to the installation docs.
- Use `stash-cli-*` release asset names for the custom Homebrew formula updater.

## 0.5.3 - 2026-04-05

- Rename the published crate to `stash-cli` while keeping the installed binary
  name as `stash`.
- General repository cleanup.

## 0.5.2 - 2026-04-05

- Remove the dedicated `stash version` subcommand in favor of clap's built-in
  `--version`.
- Harden the Homebrew upgrade guard for the breaking `0.5.0` boundary.

## 0.5.1 - 2026-04-05

- Bump the release to `0.5.1`.
- Add protection against unsupported Homebrew upgrades from versions older than
  `0.5.0`.
- Update the custom Homebrew formula generator for `cargo-dist` release assets.

## 0.5.0 - 2026-04-05

- Replace the Go implementation with Rust as the active codebase.
- Move the Rust crate to the repository root.
- Adopt `cargo-dist` for release packaging.
- Split GitHub release packaging from the custom Homebrew formula workflow.
- Switch stash storage to split `data/` and `attr/` directories with lower-case
  ULID filenames.
- Replace `meta.json` with flat `attr` files.
- Store previews directly in entry attributes and simplify preview generation.
- Add and use an ordered list cache for faster listing and history commands.
- Remove indexes, hashing, and MIME/type auto-detection.
- Add the `path` command.
- Expand Rust CLI support:
  - clap-based help and completions
  - colorized `ls` and `log`
  - terminal-width-aware preview sizing
  - attribute-aware `log --format`
- Add Rust command benchmarks and benchmark helpers.
- Update documentation to describe only the Rust implementation.

## 0.4.4 - 2026-04-02

- Simplify writable attribute handling in `stash attr`.

## 0.4.3 - 2026-04-02

- Refine metadata display in `ls` and `log`.

## 0.4.2 - 2026-04-02

- Document shell support.
- Add and install shell completions.
- Improve Homebrew publishing workflow handling.

## 0.4.1 - 2026-04-02

- Split the Homebrew formula update into its own workflow.
- Add manual tag input for release runs.
- Add scripted Homebrew formula publishing.
- Update demo videos.

## 0.4.0 - 2026-04-02

- Introduce `stash attr` as the main metadata interface.
- Remove `inspect` and the old metadata subcommand.
- Refine `ls` output flags and metadata display.
- Add VHS demos.

## 0.3.1 - 2026-04-01

- Show previews in `inspect`.
- Fix README links.

## 0.3.0 - 2026-04-01

- Expand the examples workflow and docs.
- Add `stash tee`.
- Add summary and entry indexes for faster scans and listings.
- Add tests and store benchmarks.

## 0.2.2 - 2026-04-01

- Add `rm --before` and remove `clear`.
- Add `inspect`.
- Make `log` the detailed history view.

## 0.2.1 - 2026-04-01

- Add file-oriented `ls`.
- Improve compact `log` rendering.
- Merge `peek` into `cat`.
- Add the `stash-copy` helper script.

## 0.2.0 - 2026-04-01

- Replace `--full` with `log` ID modes.
- Add stack refs and log metadata filters.
- Update the README for the newer history model.

## 0.1.4 - 2026-04-01

- Clear quarantine bits in the Homebrew cask install flow.

## 0.1.3 - 2026-04-01

- Switch GoReleaser Homebrew publishing back to casks.

## 0.1.2 - 2026-04-01

- Use the supported GoReleaser formula key.

## 0.1.1 - 2026-04-01

- Update GoReleaser publishing configuration.

## 0.1.0 - 2026-04-01

- Initial public release of `stash`.
- Add history views, previews, metadata editing, templates, colors, and
  `STASH_DIR` support.
