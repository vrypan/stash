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
stash rm <id|n|@n>...
stash rm --before <id|@n>
stash rm -a <name|name=value>
stash completion <bash|zsh|fish>
```

Without a subcommand, `stash` uses smart mode:
- in the middle of a pipeline, it behaves like `stash tee`
- otherwise, it behaves like `stash push`

`stash list` is an alias for `stash log`.

## Push And Tee

`stash push` always stores input and returns the new entry ID only if asked:

```bash
stash push file.txt
stash push --print file.txt
stash push --print=stderr file.txt
```

`stash tee` always stores input and forwards it to stdout:

```bash
some-command | stash tee | next-command
some-command | stash tee --print=stderr | next-command
```

`--print` controls where the generated entry ID is emitted:
- `--print` or `--print=stdout` or `--print=1`
- `--print=stderr` or `--print=2`
- `--print=null` or `--print=0`

Notes:
- default is `--print=null`
- bare `stash` uses the same `--print` flag and passes it through to the
  implicit `push` or `tee` mode it selects

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

Set attributes:

```bash
stash attr @1 source=usgs stage=raw
stash attr @1 --unset stage
```

Reserved fields such as `id`, `ts`, and `size` are read-only. Other
attributes are writable directly with `key=value` and removable with
`--unset`.

Read multiple fields:

```bash
stash attr @1 source stage
```

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
stash ls -A
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
- `-A` or `--attrs` shows attributes where available without filtering
- `-a name` filters to entries where the attribute is set
- multiple `-a/--attr` flags use OR semantics
- `stash ls` renders one column per requested tag when explicit tags are used
- `stash ls -A` shows attribute values inline
- `--id=short|full|pos` controls the first column in all modes

## Log Output

`stash log` shows one detailed block per entry:

```bash
stash log
stash log -n 10
stash log -r
stash log --id=short
stash log --id=pos
stash log -A
```

`stash log` defaults to full IDs and ISO dates.
Use `--id=short`, `--id=full`, or `--id=pos` to override the display mode.

Show or filter entry attributes:

```bash
stash log -A
stash log -a job
stash log -a job -a owner
```

`-A` shows attributes where available without filtering.
`-a name` matches entries that contain the attribute with any value.
Multiple `-a/--attr` flags are combined with OR.

Notes:
- `stash log` shows size, date, selected attributes, and a preview.
- `--color=false` disables color output.
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

## Remove

Remove one entry directly:

```bash
stash rm @1
stash rm yjvyz3sf
stash rm @1 @3 yjvyz3sf
```

Remove older entries:

```bash
stash rm --before @10
```

Remove entries by attribute match:

```bash
stash rm -a source
stash rm -a source=usgs
stash rm -a source=usgs -a stage=raw
```

Notes:
- `-a name` matches entries where the attribute is set
- `-a name=value` matches entries where the attribute equals that value
- multiple `-a/--attr` filters use AND semantics
- `stash rm -a ...` shows the matching entries and asks for confirmation unless `-f` is used

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
