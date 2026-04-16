# Scripts

Helper scripts around `stash`.

You can use these as-is, but they are also meant to serve as examples and
inspiration for integrating `stash` into your own workflows and shell setup.

## Starship

If you use [Starship](https://starship.rs/), you can add a custom prompt module
that shows the number of items currently stored in your stash.

Add this to `~/.config/starship.toml`:

```toml
[custom.stash_count]
description = "Show stash item count"
command = '''
count=$(find "${STASH_DIR:-$HOME/.stash}/attr" -type f 2>/dev/null | wc -l | tr -d ' ')
if [ -n "$STASH_DIR" ] && [ "$STASH_DIR" != "$HOME/.stash" ]; then
  printf '%s~{@ %s' "$(basename "$STASH_DIR")" "$count" 
else
  printf '~{@ %s' "$count"
fi
'''
when = 'test -d "${STASH_DIR:-$HOME/.stash}/attr"'
shell = ["bash", "--noprofile", "--norc"]
style = "bold cyan"
format = "[$output]($style)"
```

Shows `~{@ 42` when using the default stash, and `~{@ 12 (work)` when a
named stash is active.

Then add `${custom.stash_count}` to your main Starship `format`.

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
## `stash-rg`

Searches both attribute output and entry contents across the whole stash using
`rg`.

> [!NOTE]
> brew installs it in `$(brew --prefix)/share/stash/scripts/stash-rg`

```bash
stash-rg TODO
stash-rg 'error|warning'
stash-rg '^filename'
```

Output format:

```text
attr 37733x4x 1: command du -sh * | sstash\nstash attr @1 command
data 37733x4x 2: stash attr @1 command
```

It prints:
- whether the match came from `attr` or `data`
- the short stash ID
- the `rg` line number
- the matched line with color preserved

## `rstash`

Stores stdin or a local file into a remote stash over SSH using `stash push`.

`stash` must be available on the remote, but is not required on the local
host.

> [!NOTE]
> brew installs it in `$(brew --prefix)/share/stash/scripts/rstash`

```bash
rstash user@host README.md
printf 'hello\n' | rstash user@host --attr source=local
rstash user@host --print=stdout --attr label=docs README.md
```

Behavior:
- first argument is the SSH target
- forwards arguments to remote `stash push`
- when the last argument is a local file, it is streamed over SSH
- adds `filename=<basename>` automatically for local files unless you already
  passed a `filename=...` attribute explicitly

## `chstash.sh`

Switches between named stashes by setting `STASH_DIR`. Works with both
bash and zsh, and activates tab completion automatically based on the
running shell.

Source it from your `~/.bashrc` or `~/.zshrc`:

```sh
source /path/to/scripts/chstash.sh
# or, if you installed stash using brew:
# source "$(brew --prefix)/share/stash/scripts/chstash.sh"
```

Usage:

```sh
chstash           # show the active stash (or default)
chstash work      # switch to ~/.stashes/work (created if needed)
chstash /tmp/foo  # switch to an absolute path (created if needed)
chstash -         # reset to default (~/.stash)
chstash --list    # list named stashes in $STASH_BASE
```

Named stashes live under `~/.stashes` by default. Override with
`STASH_BASE`:

```sh
export STASH_BASE=~/projects
chstash myapp     # switches to ~/projects/myapp
```

Tab completion lists existing named stashes in `$STASH_BASE`.

## zsh-specific

### `sstash.zsh`

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

### `stash-fzf.zsh`

Adds `fzf`-powered ref completion for selected `stash cat/attr/path/rm` commands in zsh.

Just type `stash cat <tab>` and you'll get a picker to select the entry id.

To enable, source this helper from your
`~/.zshrc`:

```zsh
source /path/to/stash/scripts/stash-fzf.zsh
# or, if you installed stash using brew:
# source "$(brew --prefix)/share/stash/scripts/stash-fzf.zsh"
```
