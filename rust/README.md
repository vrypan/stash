# stash

Current status:
- this is the active implementation of `stash`
- reads and writes the current `~/.stash` layout:
  - `data/<ulid>`
  - `attr/<ulid>`
- supports the current command set:
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
- `completion`
- stores flat attributes in `attr/<ulid>` and keeps the ordered list cache in
  `cache/`

Notable behavior:
- `ls` supports `-l`, `-n`, `-r`, `-p`, `--date`, `--size`, `--name`,
  `-a/--attr`, `--chars`, and `--id`
- `log` supports `-n`, `-r`, `--json`, `--format`, `--date`,
  `-a/--attr`, `--chars`, and `--id`
- `attr` supports `--json`, `-p/--preview`, `--separator`, and
  `set` / `unset`
- `path` supports `-a/--attr` for the attribute file path and `-d/--dir` for
  the containing directory
- `tee` supports `--partial`

Run it with:

```bash
cargo run --manifest-path rust/Cargo.toml -- ls
```
