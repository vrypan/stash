# stash

`stash` is a small local store for pipeline output and ad hoc file snapshots.

It stores each entry as raw bytes under `~/.stash`, assigns it a stable ULID, and
lets you retrieve entries by recency or ID later.

## Build

```bash
make build
```

## Stash Location

By default, `stash` stores data under `~/.stash`.

You can override the stash root with `STASH_DIR`:

```bash
STASH_DIR=/tmp/job-a stash log
STASH_DIR=/tmp/job-a stash Makefile
STASH_DIR=/tmp/job-b stash log
```

This is useful when you want separate independent stashes for different jobs,
projects, or CI runs.

## Basic Usage

Stash stdin:

```bash
git diff | stash
printf 'hello\n' | stash
```

Stash a file directly:

```bash
stash Makefile
stash push path/to/file.txt
```

When stashing a file path, `stash` stores the basename in entry metadata as
`meta.filename`.

Retrieve data:

```bash
stash peek
stash pop
stash cat 01kn2ahqhr738w84t3wpc43xd3
stash cat wpc43xd3
```

Remove data:

```bash
stash rm wpc43xd3
stash clear
```

## Commands

```text
stash [file]
stash push [file]
stash log
stash metadata <id>
stash peek
stash pop
stash cat <id>
stash rm <id>
stash clear
stash version
```

`stash list` is an alias for `stash log`.

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

Default log output is compact and optimized for scanning:

```bash
stash log
stash log -n 10
stash log --reverse
stash log --full
```

Long output shows one block per entry:

```bash
stash log -l
stash log -l --date absolute
```

Notes:
- Compact output shows text previews for text-like entries.
- Compact output only shows a type label for non-text entries.
- Long output shows the base MIME type, size, date, hash, metadata, and a
  preview only for text-like entries.

## Structured Output

JSON output mirrors the long view:

```bash
stash log --json
stash log --json -n 1
```

Each JSON entry includes:
- `id`
- `short_id`
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

## Metadata

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

## Storage

Entries live under:

```text
~/.stash/
  entries/<ULID>/data
  entries/<ULID>/meta.json
```

When `STASH_DIR` is set, that directory becomes the stash root instead.

Data is stored exactly as received.

## Examples

Preview recent text entries:

```bash
stash log
```

Show verbose history:

```bash
stash log -l
```

Query JSON with `jq`:

```bash
stash log --json | jq '.[].meta.filename'
```

Show entry metadata:

```bash
stash meta wpc43xd3
```

Print a custom table:

```bash
stash log --format '{{printf "%-10s %-8s %s" .ShortID .SizeHuman (index .Meta "filename")}}'
```
