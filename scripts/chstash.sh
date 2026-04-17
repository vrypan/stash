#!/usr/bin/env sh
# chstash.sh — switch between stash pockets
#
# Source this file in your shell config:
#   source /path/to/chstash.sh

_chstash_list_pockets() {
    if command -v stash >/dev/null 2>&1; then
        STASH_POCKET= stash attrs pocket 2>/dev/null
    fi
}

chstash() {
    case "$1" in
        "")
            if [ -n "${STASH_POCKET:-}" ]; then
                echo "$STASH_POCKET"
            else
                echo "(all pockets)"
            fi
            return 0
            ;;
        -)
            unset STASH_POCKET
            echo "stash: all pockets"
            return 0
            ;;
        -l|--list)
            local pockets
            pockets="$(_chstash_list_pockets)"
            if [ -n "$pockets" ]; then
                printf '%s\n' "$pockets"
            else
                echo "(no pockets)"
            fi
            return 0
            ;;
        -h|--help)
            cat <<'EOF'
usage: chstash [pocket|-|--list]

  (no args)    show active pocket
  pocket       set STASH_POCKET=pocket
  -            clear STASH_POCKET
  -l, --list   list known pocket values via `stash attrs pocket`
  -h, --help   show this help
EOF
            return 0
            ;;
    esac

    export STASH_POCKET="$1"
    echo "stash pocket: $STASH_POCKET"
}

# Completion — zsh
if [ -n "$ZSH_VERSION" ]; then
    _chstash() {
        local -a names
        names=("${(@f)$(_chstash_list_pockets)}")
        compadd -a names
    }
    compdef _chstash chstash

# Completion — bash
elif [ -n "$BASH_VERSION" ]; then
    _chstash_complete() {
        local cur="${COMP_WORDS[COMP_CWORD]}"
        local names
        names="$(_chstash_list_pockets)"
        COMPREPLY=($(compgen -W "$names" -- "$cur"))
    }
    complete -F _chstash_complete chstash
fi
