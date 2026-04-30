# Changelog

All notable changes to `stash` are documented in this file.

## 0.10.0

This is a **major** update: `stash` has been ported to Zig.

- Replace external CLI metadata dependencies with the built-in Zig parser.
- Generate command help option rows from the same parameter descriptions used
  by the parser.
- Remove `stash pop`; use `stash cat @1` followed by `stash rm @1` when the
  newest entry should be printed and removed.
- Remove the obsolete `stash-completion` packaging/docs; the Zig port does not
  ship the separate completion helper.
- Update release automation to publish Zig-targeted archives and run the
  Homebrew formula update after release assets are created.
- Restore smart bare `stash` behavior:
  - in a pipeline, `stash` forwards input like `stash tee`
  - otherwise, `stash` stores input like `stash push`
- Restore file stashing for `stash <file>` and `stash push <file>`.
- Restore automatic `filename` attributes when stashing files directly.
- Restore partial-save handling for interrupted stdin input:
  - captured bytes are saved when `--save-on-error=true`
  - partial entries are marked with `partial=true`
  - interrupted saves exit non-zero after writing the entry
- Parse `--save-on-error=true|false` for `stash tee` and stdin-driven
  push paths.
- Make `stash ls --size` and `stash ls --bytes` independent columns so they
  can be shown separately or together.
- Keep `stash tee <file>` invalid so explicit tee remains stdin-only.
- Fix a list-cache temporary-file race when multiple commands rebuild the
  cache concurrently.

## 0.9.0

- Color attribute keys in `stash attr` output (cyan when stdout is a
  terminal; disable with `--color false`).
- Add attribute value listing, so attribute-oriented workflows can inspect
  the distinct values stored for a given key.
- Performance improvements in listing hot paths:
  - Replace sort + reverse with a single descending sort in
    `list_entry_ids`.
  - Eliminate intermediate `Vec` allocation when building the inline
    attribute string in `ls -A`.
  - `write_list_cache` now takes ownership of items to avoid cloning
    the full entry list on cache writes.
  - `all_attr_keys` computes the key index from already-loaded items
    instead of re-reading the cache file.
- Add in-process Criterion benchmarks (`BenchmarkLsLib1000`,
  `BenchmarkLsLibAll1000`, `BenchmarkLsJsonLib1000`) for reliable
  regression detection without process-spawn variance.
- Improve `stash rm` confirmation output:
  - show entry previews before deletion
  - reuse the long-listing rendering for a more informative confirmation
- Expand stash-switching workflows:
  - add `chstash.sh` helper for switching between named stashes
  - add pocket-based stash selection (STASH_POCKET)
  - update `chstash` to use pockets in the selector flow

## 0.8.1

- Extend `stash cat` selection:
  - allow multiple refs in one call
  - add `-a/--attr` filtering with `name` and `name=value`
  - add `-r/--reverse` to reverse the output order
- Extend `stash ls -a/--attr` filtering:
  - allow `-a name=value` for exact-value filtering
  - allow `-a ++name=value` as shorthand for `-a name=value -a +name`
- Update the `ls` help text and docs so `++name` and `++name=value` are
  described as shorthand aliases.

## 0.8.0

Breaking UI change: The meaning of `stash ls -a key` and `stash ls -a +key`
has been swapped. Now `-a key` means "filter where key exists" and
`-a +key` means "show key (value) where set".

- Remove the in-process `stash completion` subcommand in favor of the
  standalone `stash-completion` helper binary.
- Refine `stash ls -a/--attr` selection:
  - `-a name` filters on the attribute
  - `-a +name` shows the attribute column
  - `-a ++name` filters and shows in one argument
- Update the README, reference docs, and demos to match the current `ls`
  attribute behavior.

## 0.7.1

- Reduce binary size by about 40%
- Improve `stash cat` throughput by using a 64KB `BufReader`.

## 0.7.0

`stash log` has been removed in favor of `stash ls`.

- `stash log` removed.
- `stash ls` is now the single listing command.
- Expand `stash ls` output modes:
  - add `--json` with rich per-entry output
  - add `--attrs=list|count|flag`
  - add `--headers` for labeled tabular output
  - make `-l` expand to `--date --size --attrs=flag --preview`
  - color explicit `-a` columns and inline `-A` output in cyan
- Improve listing and store hot paths:
  - reduce allocations in `ls`
  - reuse output buffers in tabular rendering
  - tighten cache validation and id resolution
  - add and tune Criterion benchmarks for `ls`, `attr`, and `push`
- Add `stash rm --after` to remove entries newer than a referenced entry.
- Migrate the internal list cache serialization from `bincode` to `rkyv`.
- Update `signal-hook` to `0.4.4`.
- Keep empty `-A/--attrs=list` cells aligned across `stash ls` rows.
- Refine `stash ls -a/--attr` selection:
  - `-a name` shows the attribute column
  - `-a +name` filters on the attribute
  - `-a ++name` filters and shows in one argument
- Improve the `stash-fzf.zsh` helper:
  - use a richer multiline ref picker
  - fix picker cancel and insertion behavior
  - add `fzf`-powered attribute completion from `stash attrs --count`
  - support `-a`, `-a +`, and `-a ++` attr completion prefixes


## 0.6.0

This is a major code refactor with small perf improvements and significant
code quality improvements.

- Add `stash attrs` for listing attribute keys across the stash, with optional
  per-key counts.
- Improve `stash-rg`, the helper script for searching through stashed
  attributes and data.
- Refactor the Rust codebase into smaller modules:
  - split the old `cli.rs` monolith into per-command modules under `cmd/`
  - split `store.rs` into a `store/` module
  - extract shared display/rendering code into a `display` module
- Simplify and tighten the store/cache implementation:
  - store `Meta` directly in the list cache
  - move `MetaSelection` into store-related code
  - keep `newest()` on the cache-based path
- Improve performance in several hot paths:
  - avoid extra item passes in `ls --preview`
  - fix N+1 cache reads in `stash attrs`
  - reduce allocations in preview and ANSI-trimming helpers
  - speed up attr parsing and ULID encoding
- General internal cleanup and deduplication:
  - deduplicate the push/tee read loop
  - consolidate UTC date conversion helpers
  - simplify path handling and attribute-selection internals

## 0.5.7

- Escape control characters like `\n` in plain-text attribute output for
  `stash attr`, `stash ls`, and `stash log`.
- Add `stash-rg`, a helper script for searching across stashed attributes and
  data with `rg`.
- Add `rstash`, a helper script for pushing stdin or files to a remote stash
  host over `ssh`.

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
