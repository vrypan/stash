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
