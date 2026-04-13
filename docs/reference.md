# Reference

## Commands

```text
stash [file]
stash push [file]
stash tee
stash path [id|n|@n]
stash attr <id|n|@n>
stash attrs [--count]
stash cat [id|n|@n]
stash ls
stash pop
stash rm <id|n|@n>...
stash rm --before <id|@n>
stash rm -a <name|name=value>
stash-completion <bash|zsh|fish>
```

Without a subcommand, `stash` uses smart mode:
- in the middle of a pipeline, it behaves like `stash tee`
- otherwise, it behaves like `stash push`
## Push And Tee

`stash push` always stores input and returns the new entry ID only if asked:

```bash
stash push file.txt
stash push --print=stdout file.txt
stash push --print=stderr file.txt
```

`stash tee` always stores input and forwards it to stdout:

```bash
some-command | stash tee | next-command
some-command | stash tee --print=stderr | next-command
some-command | stash tee --save-on-error=false | next-command
```

`--print` controls where the generated entry ID is emitted:
- `--print=stdout` or `--print=1`
- `--print=stderr` or `--print=2`
- `--print=null` or `--print=0`

Notes:
- default is `--print=null`
- bare `stash` uses the same `--print` flag and passes it through to the
  implicit `push` or `tee` mode it selects
- `stash tee` defaults to `--save-on-error=true`
- `--save-on-error=false` disables saving interrupted input such as `Ctrl-C`
- downstream broken pipes are treated as normal exits
- when a broken pipe happens after input was captured, `stash tee` keeps the
  saved entry

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

## Attrs

List attribute keys seen across the stash:

```bash
stash attrs
stash attrs --count
```

Notes:
- `stash attrs` prints one attribute key per line
- `stash attrs --count` prints `key<TAB>count`
- this command lists user-defined attribute keys stored in entry attrs
- use `stash ls -a +key` to see matching entries
- use `stash attr <ref>` to inspect the attributes of one specific entry

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
stash ls --headers --date --size
stash ls --json
stash ls --size=bytes --name
stash ls -a source -a stage
stash ls -a source -a +source
```

`--long` is shorthand for `--date --size --attrs=flag --preview`:

```bash
stash ls -l
```

Notes:
- `--date` defaults to `ls` if no value is given
- `--size` defaults to `human` if no value is given
- `--date` accepts `iso`, `ago`, or `ls`
- `--size` accepts `human` or `bytes`
- `--headers` prints a header row for tabular output
- `-A` and `--attrs=list` show attribute values inline
- `--attrs=count` shows a per-entry count of user-defined attrs
- `--attrs=flag` shows `*` when an entry has one or more user attrs
- `-a name` selects an attribute for display
- `-a +name` filters to entries where the attribute is set
- `-a ++name` both shows that attribute and filters to entries where it is set
- `--id=short|full|pos` controls the first column in all modes

## Structured Output

JSON output mirrors the rich listing view:

```bash
stash ls --json
stash ls --json -n 1
stash ls --json -a +kind
```

Each JSON entry includes:
- `id`
- `short_id`
- `stack_ref`
- `ts`
- `date`
- `size`
- `size_human`
- flattened attributes
- `preview`

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
stash rm --after @10
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
- `--before <ref>` removes entries older than the referenced entry
- `--after <ref>` removes entries newer than the referenced entry
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
