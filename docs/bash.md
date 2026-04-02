# Bash Completion

If you installed `stash` with Homebrew, bash completion is installed
automatically.

For a manual install, generate the completion file:

```bash
mkdir -p ~/.local/share/bash-completion/completions
stash completion bash > ~/.local/share/bash-completion/completions/stash
```

Make sure your shell loads `bash-completion`. On many systems this already
happens automatically for interactive shells.
