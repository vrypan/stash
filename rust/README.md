# stash-rs

This is a parallel Rust implementation of `stash`.

Current status:
- reads and writes the same `~/.stash` layout
- supports the current core command set:
  - `push`
  - `tee`
  - `cat`
  - `ls`
  - `log`
  - `attr`
  - `path`
  - `rm`
  - `pop`
  - `version`
- matches the current metadata and preview layout
- includes the ordered list cache used by the Go implementation

Notable compatibility points:
- `ls` supports `-l`, `-n`, `-r`, `-p`, `--date`, `--size`, `--name`,
  `-a/--attr`, `--chars`, and `--id`
- `log` supports `-n`, `-r`, `--json`, `--format`, `--date`,
  `-a/--attr`, `--chars`, and `--id`
- `attr` supports `--json`, `-p/--preview`, `--separator`, and
  `set` / `unset` with the same writable-key rules as Go
- `tee` supports `--partial`

Run it with:

```bash
cargo run --manifest-path rust/Cargo.toml -- ls
```
