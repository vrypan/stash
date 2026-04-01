# Reference

## Commands

```text
stash [file]
stash push [file]
stash tee
stash log
stash inspect <id|n|@n>
stash metadata <id>
stash cat [id|n|@n]
stash peek [id|n|@n]
stash pop
stash rm <id>
stash rm --before <id|@n>
stash version
```

`stash list` is an alias for `stash log`.
`stash peek` is an alias for `stash cat`.

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

## Metadata Commands

Show user metadata for an entry:

```bash
stash meta wpc43xd3
stash metadata 01kn2ahqhr738w84t3wpc43xd3
```

Output is printed as sorted `key=value` lines, similar to
`git config --list`.

Update user metadata:

```bash
stash metadata wpc43xd3 set job=nightly owner=ci
stash metadata wpc43xd3 unset owner
```

These commands update only the user `meta` object in `meta.json`. They do not
modify core fields such as `id`, `ts`, `hash`, `size`, `type`, or `mime`.

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
- Use `stash ls` for one-line, file-oriented views.

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
