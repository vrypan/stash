# Using `stash` in Agent Workflows

`stash` works well as a scratchpad for agent-style command-line work.

Instead of writing intermediate results to ad hoc files under `/tmp`, an agent
can store command output in `stash`, tag it, and come back to it later by stack
ref or attribute.

This is useful when:
- a command is expensive and you want to inspect the output more than once
- you want to compare the current result with an earlier run
- you want to label outputs with context such as `step`, `kind`, or `branch`

`stash` is not a replacement for real files when another tool requires a file
path. In those cases, use a normal file. But for reusable command output,
`stash` is often a better fit.

## Store command output for later inspection

```bash
cargo test -- --nocapture | stash -a kind=test-output -a step=pre-refactor
stash cat | less
stash attr @1
```

This gives you a reusable snapshot of the full test output without inventing a
temporary file name.

## Compare two `cargo test` runs

```bash
cargo test -- --nocapture | stash -a kind=test-output -a phase=before
# ... make changes ...
cargo test -- --nocapture | stash -a kind=test-output -a phase=after

diff -u <(stash cat @2) <(stash cat @1)
```

If you want to find the right snapshots first:

```bash
stash attrs --count
stash ls -a phase -a +phase
stash ls --attrs=list
```

## Keep benchmark runs by default

`cargo bench` is a particularly good fit for `stash`.

Benchmark output is often useful later, even when nobody planned ahead for a
comparison. If an agent stores each benchmark run with a few stable
attributes, it becomes easy to go back and compare current results with older
ones.

A good default is:

```bash
cargo bench | stash -a kind=cargo-bench -a commit="$(git rev-parse HEAD)"
```

That gives each run:
- `kind=cargo-bench`
- `commit=<git sha>`
- the built-in stash timestamp

Those three pieces are often enough to recover the right baseline later.

Useful follow-up commands:

```bash
stash attrs --count
stash ls -a ++kind -a ++commit
stash ls --attrs=list
diff -u <(stash cat @2) <(stash cat @1)
```

You can still add narrower labels when they help:

```bash
cargo bench --bench cli | stash -a kind=cargo-bench -a commit="$(git rev-parse HEAD)" -a suite=cli
```

This is a good habit for agent workflows because it preserves benchmark history
even when the comparison was not planned in advance.

## Keep the pipeline flowing with `stash tee`

If you still want to stream output to the terminal while saving it:

```bash
cargo test -- --nocapture | stash tee -a kind=test-output | less
cargo bench | stash tee -a kind=bench
```

This is useful in interactive debugging sessions where you want to watch the
command and still keep the result.

## Suggested attribute conventions

Small, predictable attributes make stash output easier to reuse:

- `kind=test-output`
- `kind=cargo-bench`
- `step=pre-refactor`
- `step=post-refactor`
- `phase=before`
- `phase=after`
- `branch=main`
- `commit=<git sha>`
- `suite=cli`

With a few consistent keys, you can quickly find related runs:

```bash
stash ls -a ++kind -a ++phase
stash ls --attrs=list
stash attrs --count
```

## Recall by content

When you don't remember which entry holds something, search the data with
`stash grep`. Attribute filters narrow the search first, so tagged entries
stay cheap to scan:

```bash
stash grep -i "connection timeout" -a kind=test-output
stash grep -l "TODO" | while read id; do stash attr "$id"; done
```

`stash grep` does literal substring matching and exits non-zero when there
are no matches, so it composes cleanly in scripts.

## Supersede a labeled entry

When you keep a single "current" entry under a stable label and want to
update it, `stash push --replace` writes the new content and drops the old
entry in one step, carrying the label forward:

```bash
# first write
echo "prefers tabs" | stash -a name=user-prefs -a kind=memory

# later update: same labels, new content, old entry removed
echo "prefers tabs; 4-wide" | stash push --replace @1
```

The replacement inherits the referenced entry's attributes, so the label
survives without repeating `-a name=user-prefs`. Any `-a` you pass overrides
the inherited value. This keeps "latest wins per label" from accumulating
stale entries that attribute filters can't tell apart.

## When to use a real file instead

Prefer a normal file when:
- another program needs a filename
- you need to edit the result in place
- the output is part of a file-based interface or handoff

Prefer `stash` when:
- the output is mainly for inspection, comparison, or reuse
- stack refs like `@1` and `@2` are more useful than file paths
- the result benefits from lightweight labels
