# Reference

## Commands

```text
stash [file]
stash push [file]
stash tee
stash path [id|n|@n]
stash log
stash attr <id|n|@n>
stash cat [id|n|@n]
stash ls
stash pop
stash rm <id>
stash rm --before <id|@n>
stash completion <bash|zsh|fish>
```

`stash list` is an alias for `stash log`.

## Attr

Show all stored entry fields:

```bash
stash attr wpc43xd3
stash attr 01kn2ahqhr738w84t3wpc43xd3
```

This prints reserved fields such as `id`, `ts`, and `size`, plus any
user-defined attributes.

Read a single field:

```bash
stash attr @1 source
```

Update attributes:

```bash
stash attr @1 set source=usgs stage=raw
stash attr @1 unset stage
```

Reserved fields such as `id`, `ts`, and `size` are read-only. Other
attributes are writable with `attr set` and `attr unset`.

Use `--json` to print the full flat attribute object:

```bash
stash attr @1 --json
```

Use `--separator` to change the delimiter in the default text output:

```bash
stash attr @1 --separator='='
```

## ls

`stash ls` prints entry identifiers only:

```bash
stash ls
stash ls --id=full
stash ls --id=pos
```

Add columns explicitly:

```bash
stash ls -a @
stash ls --date
stash ls --size
stash ls --name
stash ls --preview
stash ls --size=bytes --name
stash ls -a source -a stage
```

`--long` is shorthand for `--date --size --name`:

```bash
stash ls -l
```

Notes:
- `--date` defaults to `ls` if no value is given
- `--size` defaults to `human` if no value is given
- `--date` accepts `iso`, `ago`, or `ls`
- `--size` accepts `human` or `bytes`
- `-a @` or `--attr @` shows attributes where available without filtering
- `-a name` filters to entries where the attribute is set
- multiple `-a/--attr` flags use OR semantics
- `stash ls` renders one column per requested tag when explicit tags are used
- `stash ls -a @` shows attribute values inline
- `--id=short|full|pos` controls the first column in all modes

## Log Output

`stash log` shows one detailed block per entry:

```bash
stash log
stash log -n 10
stash log -r
stash log --id=short
stash log --id=pos
stash log -a @
```

`stash log` defaults to full IDs and ISO dates.
Use `--id=short`, `--id=full`, or `--id=pos` to override the display mode.

Show or filter entry attributes:

```bash
stash log -a @
stash log -a job
stash log -a job -a owner
```

`-a @` shows attributes where available without filtering.
`-a name` matches entries that contain the attribute with any value.
Multiple `-a/--attr` flags are combined with OR.

Notes:
- `stash log` shows size, date, selected attributes, and a preview.
- Use `stash ls` for one-line ID views and `stash ls -l` for file-oriented detail.

## Structured Output

JSON output mirrors the long view:

```bash
stash log --json
stash log --json -n 1
```

Each JSON entry includes:
- `id`
- `short_id`
- `stack_ref`
- `ts`
- `date`
- `size`
- `size_human`
- `meta`
- `preview`

## Custom Formatting

`stash log --format` renders each entry through a lightweight placeholder
format:

```bash
stash log --format '{short_id} {date} {size_human}'
stash log --format '{short_id} {attr:filename}'
```

Available placeholders:
- `{id}`
- `{short_id}`
- `{stack_ref}`
- `{ts}`
- `{date}`
- `{size}`
- `{size_human}`
- `{preview}`
- `{attr:key}`

Use shell quoting such as `$'...'` if you want to include tabs or newlines in
the format string.

## Path

Use `stash path` to print data or attribute file paths:

```bash
stash path @1
stash path -a @1
stash path -d @1
stash ls | stash path
```

Notes:
- default with a ref: data file path
- `-a/--attr` with a ref: attribute file path
- `-d/--dir` with a ref: containing directory
- with no ref:
  - default: `STASH_DIR/data`
  - `-a`: `STASH_DIR/attr`
  - `-d`: `STASH_DIR`

## Storage

Each entry stores raw data in `~/.stash/data/<ulid>` and attributes in
`~/.stash/attr/<ulid>`.

Current fields include:
- `id`
- `ts`
- `size`
- `preview`
- user-defined attributes such as `filename`, `source`, or `label`

Attribute files contain flat `key=value` lines. Reserved keys are read-only;
other keys come from `-a/--attr key=value`, `filename` when the entry was
created from a file path, and other user-managed attributes.

Entries live under:

```text
~/.stash/
  data/<ulid>
  attr/<ulid>
  cache/
  tmp/
```

When `STASH_DIR` is set, that directory becomes the stash root instead.

Data is stored exactly as received.
