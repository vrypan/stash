# Zsh Completion

> [!IMPORTANT]
> If you installed `stash` with Homebrew, zsh completion is
> already enabled!

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

# `fzf` ref selection

If you use `fzf`, you can add interactive ref selection for commands like
`stash cat`, `stash attr`, `stash path`, and `stash rm`.

If you already use the generated `stash` zsh completion, source the helper
after that completion is loaded:

```zsh
source /path/to/stash/scripts/stash-fzf.zsh
```

or if you installed using brew
```zsh
source "$(brew --prefix)/share/stash/scripts/stash-fzf.zsh"
```

Then `stash cat <TAB>` will open an `fzf` picker built from
`stash ls --id=full --name --preview`.

# `sstash`

`sstash` is a zsh helper that captures the full command line and stores it in
a `command` attribute.

This is useful when stashing pipeline output and wanting to remember where it
came from:

```zsh
du -sh * | sstash
stash attr @1 command
```

If you installed `stash` with Homebrew, source the bundled helper from your
`.zshrc`:
```zsh
source "$(brew --prefix)/share/stash/scripts/sstash.zsh"
```

If you installed `stash` manually, source `scripts/sstash.zsh` from your clone
or installation directory:

```zsh
source /path/to/stash/scripts/sstash.zsh
```
