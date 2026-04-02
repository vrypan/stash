# Source this file from ~/.zshrc to add `sstash`, a zsh helper that captures
# the full interactive command line into `meta.command`.
#
# Example:
#   source /path/to/stash/scripts/sstash.zsh
#   du -sh * | sstash

autoload -Uz add-zsh-hook

typeset -g STASH_SSTASH_LAST_CMD=""

_stash_sstash_preexec() {
  case "$1" in
    *sstash*)
      STASH_SSTASH_LAST_CMD=$1
      ;;
    *)
      STASH_SSTASH_LAST_CMD=""
      ;;
  esac
}

# Avoid duplicate hook registration if the file is sourced more than once.
add-zsh-hook -d preexec _stash_sstash_preexec 2>/dev/null || true
add-zsh-hook preexec _stash_sstash_preexec

sstash() {
  local cmd="$STASH_SSTASH_LAST_CMD"
  STASH_SSTASH_LAST_CMD=""

  if [[ -n "$cmd" ]]; then
    command stash -m command="$cmd" "$@"
  else
    command stash "$@"
  fi
}
