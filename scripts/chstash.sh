#!/usr/bin/env sh
# chstash.sh — switch between named stashes
#
# Source this file in your shell config:
#   source /path/to/chstash.sh
#
# Named stashes live under STASH_BASE (default: ~/.stashes).
# A bare name like "work" resolves to $STASH_BASE/work.
# An absolute or home-relative path is used as-is.

chstash() {
    local base="${STASH_BASE:-$HOME/.stashes}"

    case "$1" in
        "")
            if [ -n "$STASH_DIR" ]; then
                echo "$STASH_DIR"
            else
                echo "(default: $HOME/.stash)"
            fi
            return 0
            ;;
        -)
            unset STASH_DIR
            echo "stash: using default ($HOME/.stash)"
            return 0
            ;;
        -l|--list)
            if [ -d "$base" ]; then
                ls "$base"
            else
                echo "(no named stashes in $base)"
            fi
            return 0
            ;;
        -h|--help)
            cat <<'EOF'
usage: chstash [name|path|-|--list]

  (no args)    show active stash
  name         switch to $STASH_BASE/name (created if needed)
  /path        switch to absolute path (created if needed)
  -            reset to default (~/.stash)
  -l, --list   list named stashes in $STASH_BASE
  -h, --help   show this help
EOF
            return 0
            ;;
    esac

    local dir
    case "$1" in
        /*|~*) dir="$1" ;;
        *)     dir="$base/$1" ;;
    esac

    mkdir -p "$dir" || return 1
    export STASH_DIR="$dir"
    echo "stash: $dir"
}

# Completion — zsh
if [ -n "$ZSH_VERSION" ]; then
    _chstash() {
        local base="${STASH_BASE:-$HOME/.stashes}"
        local -a names
        [[ -d "$base" ]] && names=("${(@f)$(ls "$base" 2>/dev/null)}")
        compadd -a names
    }
    compdef _chstash chstash

# Completion — bash
elif [ -n "$BASH_VERSION" ]; then
    _chstash_complete() {
        local base="${STASH_BASE:-$HOME/.stashes}"
        local cur="${COMP_WORDS[COMP_CWORD]}"
        local names
        names=$(ls "$base" 2>/dev/null)
        COMPREPLY=($(compgen -W "$names" -- "$cur"))
    }
    complete -F _chstash_complete chstash
fi
