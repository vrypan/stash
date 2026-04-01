<img width="1280" height="640" alt="social-banner" src="https://github.com/user-attachments/assets/c98cfa58-2976-4b8d-9e5b-d9e5314348e6" />

# ~{@ stash

`stash` is a small local store for pipeline output and ad hoc file snapshots.

It stores each entry as raw bytes under `~/.stash`, assigns it a stable ULID, and
lets you retrieve entries by recency or ID later. Everything is flat files and directories.

> [!TIP]
> stash fits nicely in any workflow that would involve temporary files, or expensive output
> that needs to be processed in more than one ways.



### Save expensive output and reuse

```bash
curl -s https://api.example.com/data | stash
stash peek | jq .
stash peek | jq '.items[]'
stash peek | wc -c
```

stash can handle binary output too

```bash
magick input.png -colorspace Gray png:- | stash
stash peek | magick png:- -threshold 60% final60.png
stash peek | magick png:- -threshold 80% final80.png
```

### As a rolling scratch stack during shell work

```bash
git diff | stash
ps aux | stash
kubectl get pods -A | stash

# later

stash list
stash peek | less
stash pop | wc -l
```
### Save intermediate pipeline stages for debugging

Instead of
```bash
cat data.json | jq '.items' | tee /tmp/items.json | jq 'map(.id)'
```

you can do

```bash
cat data.json | jq '.items' | stash
stash peek | jq 'map(.id)'
stash peek | jq 'length'
```

### Store outputs from parallel experiments without naming files

```bash
for f in *.json; do
  jq '.important' "$f" | stash -m q="$f"
  # -m is used to add custom tags to each entry
done

stash log
stash cat <id> | jq .
```

> [!NOTE]
> What is `~{@`??? An ASCII art acorn.

## Installation

### From Source

Clone the repo, and run `make build`. Copy the generated binary `stash` to a location in your $PATH.

### Pre-built binaries

Available under [releases](/releases).

### Homebrew

```
brew install vrypan/tap/stash
```

## Stash repository location

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
stash peek 2
stash pop
stash cat @1
stash cat @2
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
stash peek [n]
stash pop
stash cat <id>
stash rm <id>
stash clear
stash version
```

`stash list` is an alias for `stash log`.

## Stack Refs

Commands that accept an entry ID also accept stack references:

```bash
stash cat @1
stash cat @2
stash meta @1
stash rm @3
```

Meaning:
- `@1` is the newest entry
- `@2` is the second newest entry
- `@3` is the third newest entry

This works anywhere `stash` normally accepts an `<id>`.

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
stash log --id=full
stash log --id=pos
```

Long output shows one block per entry:

```bash
stash log -l
stash log -l --date absolute
stash log -l --id=short
```

`stash log` defaults to short IDs.
`stash log -l` defaults to full IDs.
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

Peek at the second-most-recent entry:

```bash
stash peek 2
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
