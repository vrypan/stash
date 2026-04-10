<img width="1280" height="640" alt="social-banner" src="https://github.com/user-attachments/assets/c98cfa58-2976-4b8d-9e5b-d9e5314348e6" />

# ~{@ stash

`stash` is a small local store for pipeline output and ad hoc file snapshots.

It stores each entry as raw bytes under `~/.stash/data`, stores attributes
under `~/.stash/attr`, assigns it a stable ULID, and lets you retrieve entries
by recency or ID later. Everything is flat files and directories.

Without a subcommand, `stash` uses smart mode:
- in the middle of a pipeline, it behaves like `stash tee`
- otherwise, it behaves like `stash push`

> [!TIP]
> stash fits nicely in any workflow that would involve temporary files, or
> expensive output that needs to be processed in more than one ways.

> [!NOTE]
> What is `~{@`??? An ASCII art acorn.

## Quick Start

Save output and reuse it:

```bash
curl -s https://api.example.com/data | stash
stash cat | jq .
stash cat | jq '.items[]'
```

Keep the pipeline flowing while saving the same bytes:

```bash
curl -s https://api.example.com/data | stash tee | jq .
curl -s https://api.example.com/data | stash | jq .
```

By default, `stash`, `stash push`, and `stash tee` do not print the generated
entry ID. Use `--print=stdout`, `--print=stderr`, or `--print=null` to control
where the ID is emitted.

Use with diff:

```bash
find . -type f | sort | stash -a label=before
# ... later ...
find . -type f | sort | stash -a label=after

diff -u <(stash cat @2) <(stash cat @1)
```

File-oriented view:

```bash
stash ls
stash log
stash attrs --count
stash attr @1
```

## Installation

### From Source

Build the Rust binary and copy it to a location in your `PATH`:

```bash
cargo build --release
cp target/release/stash /usr/local/bin/stash
```

### With Cargo

```bash
cargo install stash-cli
```

### Pre-built binaries

Available under [releases](https://github.com/vrypan/stash/releases).

### Homebrew

```bash
brew install vrypan/tap/stash
```

### Shell support

For auto-completion and other shell-specific features, check:
- [docs/bash.md](docs/bash.md)
- [docs/fish.md](docs/fish.md)
- [docs/zsh.md](docs/zsh.md)

## Stash Repository Location

By default, `stash` stores data under `~/.stash`.

You can override the stash root with `STASH_DIR`:

```bash
STASH_DIR=/tmp/job-a stash log
STASH_DIR=/tmp/job-a stash Makefile
STASH_DIR=/tmp/job-b stash log
```

This is useful when you want separate independent stashes for different jobs,
projects, or CI runs.

## Documentation

- Usage guide: [docs/usage.md](docs/usage.md)
- Command/reference guide: [docs/reference.md](docs/reference.md)
- Example workflows: [docs/examples.md](docs/examples.md)
- Shell completion setup: [docs/bash.md](docs/bash.md), [docs/fish.md](docs/fish.md), [docs/zsh.md](docs/zsh.md)
- Demos: [demos/](demos/)
- Helper scripts: [scripts/](scripts/)
