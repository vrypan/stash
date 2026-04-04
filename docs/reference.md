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

This prints core fields such as `id`, `ts`, and `size`, plus any user-defined
attributes.

Read a single field:

```bash
stash attr @1 source
```

Update attributes:

```bash
stash attr @1 set source=usgs stage=raw
stash attr @1 unset stage
```

Core fields such as `id`, `ts`, and `size` are read-only. Other attributes are
writable with `attr set` and `attr unset`.

Use `--json` to print the full attribute object shape:

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

Show or filter log metadata:

```bash
stash log -a @
stash log -a job
stash log -a job -a owner
```

`-a @` shows attributes where available without filtering.
`-a name` matches entries that contain the attribute with any value.
Multiple `-a/--attr` flags are combined with OR.

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

Each entry stores attributes in `~/.stash/entries/<ULID>/attr`.

Current fields include:
- `id`
- `ts`
- `size`
- `preview`
- `meta`

`attr` contains user-supplied `--attr key=value` pairs, `filename` when the
entry was created from a file path, and other user-managed attributes.

Entries live under:

```text
~/.stash/
  entries/<ULID>/data
  entries/<ULID>/attr
```

When `STASH_DIR` is set, that directory becomes the stash root instead.

Data is stored exactly as received.
