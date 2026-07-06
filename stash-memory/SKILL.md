---
name: stash-memory
description: >-
  Persist and recall durable facts across sessions using `stash-mem`, a
  wrapper around the `stash` CLI backed by a dedicated memory store. Use when
  the user shares a preference, decision, convention, or project fact worth
  remembering ("remember that…", "from now on…", "we decided…"); when starting
  work and you need prior context; or when a remembered fact has changed and
  needs updating. Requires `stash` and `stash-mem` on PATH.
---

# Stash as memory

`stash-mem` is `stash` pointed at a dedicated memory store, scoped to the
current project automatically:

- It uses its own store (`STASH_MEMORY_DIR`, default `~/.claude/stash-memory`),
  never the user's `~/.stash`.
- Inside a git repo, every command is scoped to that repo's memory (a stash
  pocket named after the repo). Writes are tagged automatically; `ls`, `grep`,
  `cat`, and refs like `@1` only see this project's facts.
- Cross-project facts live in the `global` pocket:
  `STASH_POCKET=global stash-mem ...`

Everything in this store is memory — no `type=` tag needed.

## When to store

Store a fact when it is durable and not derivable from the repo or git
history: user preferences, team conventions, decisions and their rationale,
project constraints, external references. Do **not** store transient task
state, secrets, or anything already in CLAUDE.md.

## How to store

One fact per entry, with a stable `name`:

    printf 'Prefers tabs, 4-wide, in Go files.\n' \
      | stash-mem push -a name=go-indent

Attribute conventions:

- `name=<kebab>`   — stable identifier for the fact (one per fact)
- `topic=<area>`   — optional grouping (e.g. `git`, `style`, `deploy`)

A fact that applies to all projects goes in the global pocket:

    printf 'Never add attribution to commit messages.\n' \
      | STASH_POCKET=global stash-mem push -a name=commit-attribution

## How to recall

At the start of relevant work, list this project's memory as a cheap index
(metadata only, no entry bodies), then the global facts:

    stash-mem ls --format '%i  %a{name}  %p\n'
    STASH_POCKET=global stash-mem ls --format '%i  %a{name}  %p\n'

Search by content when you don't know the name:

    stash-mem grep -i "indent"
    stash-mem grep "timeout" -a topic=deploy

Read a specific memory in full:

    stash-mem cat <id-or-@n>

## How to update a fact (supersede)

Facts change. Find the entry's id, then replace it — `--replace` carries the
attributes (including the project pocket) forward and removes the old entry,
so "latest wins" without duplicates:

    id=$(stash-mem ls -a name=go-indent --format '%i\n' | head -1)
    printf 'Prefers tabs, 8-wide, in Go files.\n' | stash-mem push --replace "$id"

Pass `-a` to change a label at the same time; anything you don't override is
inherited from the old entry.

## How to prune

    stash-mem ls -l                     # review this project's facts first
    stash-mem rm -a name=go-indent      # remove a specific fact (prompts)

To review the whole store across projects (maintenance only):

    STASH_POCKET= stash-mem ls -a ++pocket

## Rules

- Before storing, `stash-mem grep` for the same fact — update the existing
  entry with `--replace` rather than creating a near-duplicate.
- Never store secrets, tokens, or credentials.
- Prefer a `name` that reads as a fact ("go-indent", not "note-3").
- Reads are safe; `rm` prompts for confirmation.
