# Scripts

Helper scripts around `stash`.

## `stash-copy`

Copies stash data and attribute files from a remote machine into the local stash repository over
SSH using `rsync`.

```bash
stash-copy user@host
stash-copy user@host:/remote/path/to/stash
```

- local stash root uses `STASH_DIR` if set, otherwise `~/.stash`
- remote stash root uses the explicit path if provided
- otherwise the remote stash root uses remote `STASH_DIR` if set, otherwise
  `~/.stash`
- `data/` and `attr/` are copied
- local `tmp/` is created if needed
- remote `tmp/` is not copied
- local entries are not deleted if they do not exist on the remote side

> [!NOTE]
> brew installs it in `$(brew --prefix)/share/stash/scripts/stash-copy`

## `stash-push-type`

Wraps the local `stash` binary and records a `type` attribute using the system
`file` command after the entry is created.

> [!NOTE]
> brew installs it in `$(brew --prefix)/share/stash/scripts/stash-push-type.zsh`

After you add it to your path:

```bash
$ ./scripts/stash-push-type demos/words.gif
01knj60kt6mxyjvehm647y0yb8

$ stash attr @1
id      01knj60kt6mxyjvehm647y0yb8
ts      2026-04-06T19:58:46.600732000Z
size    299145
filename        words.gif
type    image/gif
```

## `sstash.zsh`

Adds a zsh helper function named `sstash` that captures the full interactive
command line into `command` when you pipe output into `stash`.

Source it from your `~/.zshrc`:

```zsh
source /path/to/sstash.zsh
# or, if you installed stash using brew:
# source "$(brew --prefix)/share/stash/scripts/sstash.zsh"
```

This installs a zsh `preexec` hook using `add-zsh-hook` and defines `sstash()`.
The hook only records command lines that contain `sstash`.

```zsh
$ du -sh * | sstash --attr label=test

$ stash attr @1
id      01knj5c3zd0tnsdn5ac5fekbtx
ts      2026-04-06T19:47:35.252875000Z
size    220
command du -sh * | sstash --attr label=test
label   test
```

## `stash-fzf.zsh`

Adds `fzf`-powered ref completion for selected `stash cat/attr/path/rm` commands in zsh.

Just type `stash cat <tab>` and you'll get a picker to select the entry id.

To enable, source this helper from your
`~/.zshrc`:

```zsh
source /path/to/stash/scripts/stash-fzf.zsh
# or, if you installed stash using brew:
# source "$(brew --prefix)/share/stash/scripts/stash-fzf.zsh"
```
