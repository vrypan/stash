# fzf-powered stash ref completion for zsh.
#
# Load the standard `_stash` completion first, then source this file:
#   source /path/to/stash/scripts/stash-fzf.zsh
#
# If `fzf` is unavailable, this file leaves the normal completion unchanged.

if (( ! $+commands[fzf] )); then
  return 0
fi

typeset -gi _stash_fzf_has_base=0

if (( $+functions[_stash] )); then
  functions -c _stash _stash_completion_base
  _stash_fzf_has_base=1
fi

_stash_fzf_needs_ref() {
  local subcmd="${words[2]-}"
  local current_word="${words[CURRENT]-}"
  local i

  case "$subcmd" in
    cat|attr|path)
      [[ $CURRENT -eq 3 && "$current_word" != -* ]]
      return
      ;;
    rm)
      for (( i = 3; i < CURRENT; i++ )); do
        case "${words[i]}" in
          -a|--attr|--before)
            return 1
            ;;
        esac
      done
      [[ "$current_word" != -* ]]
      return
      ;;
    *)
      return 1
      ;;
  esac
}

_stash_fzf_pick_ref() {
  local query="${PREFIX:-}"

  stash ls --id=short --attrs=flag --preview --color=false 2>/dev/null \
    | fzf \
        --height=40% \
        --reverse \
        --border \
        --ansi \
        --query="$query" \
        --preview='stash attr {1} 2>/dev/null; printf "\n"; stash cat {1} 2>/dev/null | head -100' \
    | awk '{print $1}'
}

_stash_fzf_redraw() {
  zle reset-prompt 2>/dev/null || zle redisplay 2>/dev/null || true
}

_stash_fzf_complete() {
  if _stash_fzf_needs_ref; then
    local selected fzf_status
    zle -I 2>/dev/null || true
    selected="$(_stash_fzf_pick_ref)"
    fzf_status=$?
    _stash_fzf_redraw
    (( fzf_status == 0 )) || return 0
    [[ -n "$selected" ]] || return 0
    compstate[insert]=1
    compstate[list]=''
    compadd -Q -U -S ' ' -- "$selected"
    return
  fi

  if (( _stash_fzf_has_base )); then
    _stash_completion_base "$@"
    return
  fi

  return 1
}

compdef _stash_fzf_complete stash
