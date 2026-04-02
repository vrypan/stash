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
- local `tmp/` and `lock` are created if needed
- remote `tmp/` and `lock` are not copied
- local entries are not deleted if they do not exist on the remote side
- runs `stash index update` locally after syncing when `stash` is available in
  `PATH`

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
command line into `meta.command` when you pipe output into `stash`.

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
du -sh * | sstash -m label=ci
find . -type f | sort | sstash -m source=find -m stage=raw
```

### Behavior

- stores the full interactive command line in `meta.command`
- keeps any extra `stash` flags you pass to `sstash`
- if no matching command line was captured, falls back to plain `stash`

### Example

```bash
du -sh * | sstash -m label=nightly
stash attr @1 meta.command
```
