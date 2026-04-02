# Reference

## Commands

```text
stash [file]
stash push [file]
stash tee
stash log
stash inspect <id|n|@n>
stash attr <id|n|@n>
stash cat [id|n|@n]
stash ls
stash pop
stash rm <id>
stash rm --before <id|@n>
stash version
```

`stash list` is an alias for `stash log`.

## Index

`stash` keeps internal indexes to speed up stack refs, listing, and history
commands.

Rebuild them manually with:

```bash
stash index update
```

This is mainly useful after external changes to the stash directory, such as
copying entries from another machine. Normal `stash` commands maintain or
rebuild indexes automatically when needed.

## Attr

Show all stored entry fields:

```bash
stash attr wpc43xd3
stash attr 01kn2ahqhr738w84t3wpc43xd3
```

This prints core fields such as `id`, `ts`, `hash`, `size`, `type`, and
`mime`, plus nested user metadata as flattened `meta.*` keys.

Read a single field:

```bash
stash attr @1 hash
stash attr @1 meta.source
```

Update user metadata:

```bash
stash attr @1 set meta.source=usgs meta.stage=raw
stash attr @1 unset meta.stage
```

Writable keys are limited to `meta.*`. Core fields such as `id`, `ts`, `hash`,
`size`, `type`, and `mime` are read-only.

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
stash ls --date
stash ls --size
stash ls --name
stash ls --mime
stash ls --preview
stash ls --size=bytes --name
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
- `--id=short|full|pos` controls the first column in all modes

## Log Output

`stash log` shows one detailed block per entry:

```bash
stash log
stash log -n 10
stash log --reverse
stash log --id=short
stash log --id=pos
```

`stash log` defaults to full IDs and absolute dates.
Use `--id=short`, `--id=full`, or `--id=pos` to override the display mode.

Filter log output by metadata:

```bash
stash log --meta job
stash log --meta job=nightly
stash log --meta job --meta owner=ci
```

`--meta key` matches entries that contain the key with any value.
`--meta key=value` matches entries with an exact value.
Multiple `--meta` flags are combined with AND.

Notes:
- `stash log` shows the base MIME type, size, date, hash, metadata, and a
  preview only for text-like entries.
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
- `hash`
- `size`
- `size_human`
- `type`
- `mime`
- `meta`
- `preview`

## Custom Formatting

`stash log --format` renders each entry through a Go template:

```bash
stash log --format '{{.ShortID}} {{.Date}} {{.SizeHuman}} {{.MIME}}'
stash log --format '{{.ShortID}} {{index .Meta "filename"}}'
stash log --format '{{.ID}} {{.Hash}}'
```

Available template fields:
- `ID`
- `ShortID`
- `StackRef`
- `TS`
- `Date`
- `Hash`
- `Size`
- `SizeHuman`
- `Type`
- `MIME`
- `Meta`
- `Preview`

`MIME` is exposed in display form, so parameters such as `; charset=utf-8` are
stripped.

## Storage

Each entry stores metadata in `~/.stash/entries/<ULID>/meta.json`.

Current fields include:
- `id`
- `ts`
- `hash`
- `size`
- `type`
- `mime`
- `meta`

`meta` contains user-supplied `--meta key=value` pairs and `filename` when the
entry was created from a file path.

Entries live under:

```text
~/.stash/
  entries/<ULID>/data
  entries/<ULID>/meta.json
```

When `STASH_DIR` is set, that directory becomes the stash root instead.

Data is stored exactly as received.
