# Usage

## Basic Usage

Stash stdin:

```bash
git diff | stash
printf 'hello\n' | stash
```

Without a subcommand, `stash` uses smart mode:
- in the middle of a pipeline, it behaves like `stash tee`
- otherwise, it behaves like `stash push`

When `stash` is reading from stdin, `Ctrl-C` saves captured input before exit.

Stash a file directly:

```bash
stash Makefile
stash push path/to/file.txt
```

When stashing a file path, `stash` stores the basename as the `filename`
attribute.

Pass data through and stash it at the same time:

```bash
some-command | stash tee | next-command
some-command | stash | next-command
some-command | stash tee -a job=nightly | next-command
some-command | stash tee --save-on-error=false | next-command
```

By default, `stash tee` keeps stdout unchanged and does not print the generated
entry ID. Use `--print=stdout`, `--print=stderr`, or `--print=null` if you
want to control where the ID is emitted explicitly. Numeric aliases `1`, `2`,
and `0` are also accepted. With
`--save-on-error=true` (the default), an interrupted input stream is saved if
any bytes were captured, including `Ctrl-C` interruption, and `stash tee`
exits non-zero. Use
`--save-on-error=false` to disable that behavior. Downstream broken pipes still
exit successfully; if any input was already captured, `stash tee` keeps the
saved entry.

Retrieve data:

```bash
stash cat
stash cat 2
stash pop
stash cat @1
stash cat @2
stash cat 01kn2ahqhr738w84t3wpc43xd3
stash cat wpc43xd3
```

Remove data:

```bash
stash rm wpc43xd3
stash rm @1 @3
stash rm --before @10
stash rm --before 01kn2ahqhr738w84t3wpc43xd3 -f
stash rm -a source=usgs
```

`stash rm --before <ref>` removes entries older than the referenced entry.
The referenced entry itself is kept. By default, `stash` asks for
confirmation; use `-f` to skip the prompt.

`stash rm -a <name>` removes entries where the attribute is set.
`stash rm -a <name=value>` removes entries where the attribute matches exactly.
When attribute filters are used, `stash` shows the matching entries and asks
for confirmation unless `-f` is set.

## File-Oriented Use

You can also use `stash` like a small flat file store:

```bash
stash ls
stash ls -l
stash ls --date --size --name
stash ls --headers --date --size -A
stash attrs --count
stash cat @1
stash cat yjvyz3sf
stash rm @2
```

Example output:

```text
$ stash ls
ryacf7sz
a3f11qka

$ stash ls -l
ryacf7sz  384.3K  Apr  1  *  PNG image data, 1024 x 768, 8-bit/color RGBA,...
a3f11qka    493B  Apr  1  *  version: "3"
```

In that model:
- `stash ls` lists entry IDs only
- `stash ls -l` expands that into a richer summary view with date, size,
  attribute flag, and preview
- `stash ls --headers ...` adds column labels to tabular output
- `stash attrs --count` shows which user-defined attributes exist across the stash
- `stash cat` reads an entry by stack ref or ID
- `stash rm` deletes an entry by stack ref or ID, or removes older entries with `--before`

Filenames come from `filename` when available, so stashing files directly
works naturally with `stash ls --name` or `stash ls -l`.

## Stack Refs

Commands that accept an entry ID also accept stack references:

```bash
stash cat @1
stash cat @2
stash attr @1
stash attr @1 filename
stash rm @3
```

Meaning:
- `@1` is the newest entry
- `@2` is the second newest entry
- `@3` is the third newest entry

This works anywhere `stash` normally accepts an `<id>`.

`stash cat` also accepts:
- no argument for the newest entry
- a plain number like `2` for the second newest entry

Examples:

```bash
stash cat
stash cat 2
stash cat @2
```

## Example Workflows

### Save expensive output and reuse

```bash
curl -s https://api.example.com/data | stash
stash cat | jq .
stash cat | jq '.items[]'
stash cat | wc -c
```

A full step-by-step example using the USGS earthquake feed is in
[`docs/examples.md`](/Users/vrypan/Devel/stash/docs/examples.md).

Or keep the pipeline flowing while saving the same bytes:

```bash
curl -s https://api.example.com/data | stash tee | jq .
```

Binary output works too:

```bash
magick input.png -colorspace Gray png:- | stash
stash cat | magick png:- -threshold 60% final60.png
stash cat | magick png:- -threshold 80% final80.png
```

### Use with diff

```bash
find . -type f | sort | stash -a label=before
# ... later ...
find . -type f | sort | stash -a label=after

diff -u <(stash cat @2) <(stash cat @1)
```

And if you want to find the right snapshots first:

```bash
stash attrs --count
stash ls -a label -a +label
stash ls -A
```

### As a rolling scratch stack during shell work

```bash
git diff | stash
ps aux | stash
kubectl get pods -A | stash

# later

stash ls
stash cat | less
stash pop | wc -l
```

### Save intermediate pipeline stages for debugging

Instead of:

```bash
cat data.json | jq '.items' | tee /tmp/items.json | jq 'map(.id)'
```

you can do:

```bash
cat data.json | jq '.items' | stash
stash cat | jq 'map(.id)'
stash cat | jq 'length'
```

### Store outputs from parallel experiments without naming files

```bash
for f in *.json; do
  jq '.important' "$f" | stash -a q="$f"
done

stash ls -a q
stash cat <id> | jq .
```
