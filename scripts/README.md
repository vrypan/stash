# Scripts

This directory contains helper scripts around `stash`.

## `stash-copy`

Copies stash entries from a remote machine into the local stash repository over
SSH using `rsync`.

### Usage

```bash
stash-copy user@host
stash-copy user@host:/remote/stash
```

### Behavior

- local stash root uses `STASH_DIR` if set, otherwise `~/.stash`
- remote stash root uses the explicit path if provided
- otherwise the remote stash root uses remote `STASH_DIR` if set, otherwise
  `~/.stash`
- only `entries/` are copied
- local `tmp/` is created if needed
- remote `tmp/` is not copied
- local entries are not deleted if they do not exist on the remote side

### Requirements

- `ssh`
- `rsync`

### Example

```bash
stash-copy vrypan@srv2.local
```

Copy from a non-default remote stash path:

```bash
stash-copy vrypan@srv2.local:/srv/stash
```

## `sstash.zsh`

Adds a zsh helper function named `sstash` that captures the full interactive
command line into `command` when you pipe output into `stash`.

### Setup

Source it from your `~/.zshrc`:

```zsh
source /path/to/sstash.zsh
```

It installs a zsh `preexec` hook using `add-zsh-hook` and defines `sstash()`.
The hook only records command lines that contain `sstash`.

### Usage

```bash
du -sh * | sstash
find . -type f | sort | sstash
```

Pass additional `stash` flags as usual:

```bash
du -sh * | sstash -a label=ci
find . -type f | sort | sstash -a source=find -a stage=raw
```

### Behavior

- stores the full interactive command line in `command`
- keeps any extra `stash` flags you pass to `sstash`
- if no matching command line was captured, falls back to plain `stash`

### Example

```bash
du -sh * | sstash -a label=nightly
stash attr @1 command
```

## `stash-push-type`

Wraps the local `stash` binary and records a `type` metadata field using the
system `file` command after the entry is created.

### Usage

```bash
scripts/stash-push-type path/to/file
cat output.txt | scripts/stash-push-type
```

### Behavior

- runs `stash push`
- resolves the new entry path with `stash path`
- runs `file -b` on the stored `data` file
- stores the result in `type` via `stash attr set type=...`
- prints the new entry id to stdout

### Requirements

- `stash` available in `PATH`
- `/usr/bin/file`
