# Shell Completion

> [!IMPORTANT]
> If you installed `stash` with Homebrew, shell completion is
> already enabled.

## Bash

```bash
mkdir -p ~/.local/share/bash-completion/completions
stash-completion bash > ~/.local/share/bash-completion/completions/stash
```

Make sure your shell loads `bash-completion`. On many systems this already
happens automatically for interactive shells.

## Zsh

```bash
mkdir -p ~/.zsh/completions
stash-completion zsh > ~/.zsh/completions/_stash
```

Then add this to your `.zshrc` before `compinit`:

```zsh
fpath=(~/.zsh/completions $fpath)
autoload -Uz compinit
compinit
```

## Fish

```bash
mkdir -p ~/.config/fish/completions
stash-completion fish > ~/.config/fish/completions/stash.fish
```

Fish loads completions from that directory automatically.
