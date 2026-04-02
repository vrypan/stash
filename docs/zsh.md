# Zsh Completion

If you installed `stash` with Homebrew, zsh completion is installed
automatically.

For a manual install, generate the completion file:

```bash
mkdir -p ~/.zsh/completions
stash completion zsh > ~/.zsh/completions/_stash
```

Then add this to your `.zshrc` before `compinit`:

```zsh
fpath=(~/.zsh/completions $fpath)
autoload -Uz compinit
compinit
```

# `sstash`

## Why?

`sstash` is a zsh helper that captures the full command line and stores it in
`meta.command`.

This is useful when stashing pipeline output and wanting to remember where it
came from:

```zsh
du -sh * | sstash
stash attr @1 meta.command
```

## Homebrew install

If you installed `stash` with Homebrew, source the bundled helper from your
`.zshrc`:

```zsh
source "$(brew --prefix)/share/stash/scripts/sstash.zsh"
```

## Manual install

If you installed `stash` manually, source `scripts/sstash.zsh` from your clone
or installation directory:

```zsh
source /path/to/stash/scripts/sstash.zsh
```
