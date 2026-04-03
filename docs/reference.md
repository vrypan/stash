# Reference

## Commands

```text
stash [file]
stash push [file]
stash tee
stash log
stash attr <id|n|@n>
stash cat [id|n|@n]
stash ls
stash pop
stash rm <id>
stash rm --before <id|@n>
stash version
```

`stash list` is an alias for `stash log`.

## Attr

Show all stored entry fields:

```bash
stash attr wpc43xd3
stash attr 01kn2ahqhr738w84t3wpc43xd3
```

This prints core fields such as `id`, `ts`, and `size`, plus nested
user metadata as flattened `meta.*` keys.

Read a single field:

```bash
stash attr @1 meta.source
```

Update user metadata:

```bash
stash attr @1 set source=usgs stage=raw
stash attr @1 unset stage
```

Writable keys are stored under `meta.*`, but `attr set` and `attr unset` accept
bare keys. Core fields such as `id`, `ts`, and `size` are read-only.

Use `--json` to print the full `meta.json` object shape:

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
stash ls -m @
stash ls --date
stash ls --size
stash ls --name
stash ls --preview
stash ls --size=bytes --name
stash ls -m source -m stage
```

`--long` is shorthand for `--date --size --name`:

```bash
stash ls -l
```

Notes:
- `--date` defaults to `absolute` if no value is given
- `--size` defaults to `human` if no value is given
- `--date` accepts `absolute`, `relative`, or `ls`
- `--size` accepts `human` or `bytes`
- `-m @` or `--meta @` shows metadata where available without filtering
- `-m tag` filters to entries where `tag` is set
- multiple `-m/--meta` flags use OR semantics
- `stash ls` renders one column per requested tag when explicit tags are used
- `--id=short|full|pos` controls the first column in all modes

## Log Output

`stash log` shows one detailed block per entry:

```bash
stash log
stash log -n 10
stash log -r
stash log --id=short
stash log --id=pos
stash log -m @
```

`stash log` defaults to full IDs and absolute dates.
Use `--id=short`, `--id=full`, or `--id=pos` to override the display mode.

Show or filter log metadata:

```bash
stash log -m @
stash log -m job
stash log -m job -m owner
```

`-m @` shows metadata where available without filtering.
`-m tag` matches entries that contain the tag with any value.
Multiple `-m/--meta` flags are combined with OR.

Notes:
- `stash log` shows size, date, metadata, and a preview.
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

`stash log --format` renders each entry through a Go template:

```bash
stash log --format '{{.ShortID}} {{.Date}} {{.SizeHuman}}'
stash log --format '{{.ShortID}} {{index .Meta "filename"}}'
```

Available template fields:
- `ID`
- `ShortID`
- `StackRef`
- `TS`
- `Date`
- `Size`
- `SizeHuman`
- `Meta`
- `Preview`

## Storage

Each entry stores metadata in `~/.stash/entries/<ULID>/meta.json`.

Current fields include:
- `id`
- `ts`
- `size`
- `preview`
- `meta`

`meta` contains user-supplied `--meta key=value` pairs, `filename` when the
entry was created from a file path, and other user-managed metadata.

Entries live under:

```text
~/.stash/
  entries/<ULID>/data
  entries/<ULID>/meta.json
```

When `STASH_DIR` is set, that directory becomes the stash root instead.

Data is stored exactly as received.
