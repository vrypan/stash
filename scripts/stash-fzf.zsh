
# fzf-powered stash ref completion for zsh.
#
# Load the standard `_stash` completion first, then source this file:
#   source /path/to/stash/scripts/stash-fzf.zsh
#
# If `fzf` is unavailable, this file leaves the normal completion unchanged.

if (( ! $+commands[fzf] )); then
    echo "fzf is not available; stash fzf-assisted completion was not enabled." >&2
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

  [[ "$current_word" == -* ]] && return 1

  case "$subcmd" in
    cat|attr|path)
      (( CURRENT == 3 ))
      ;;
    rm)
      local i
      for (( i = 3; i < CURRENT; i++ )); do
        [[ "${words[i]}" == (-a|--attr) ]] && return 1
      done
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

_stash_fzf_needs_attr() {
  local subcmd="${words[2]-}"
  local prev="${words[CURRENT-1]-}"

  case "$subcmd" in
    ls|rm)
      [[ ("$prev" == "-a" || "$prev" == "--attr") && "${words[CURRENT]-}" != -* ]]
      ;;
    *)
      return 1
      ;;
  esac
}

_stash_fzf_pick_ref() {
  local query="${PREFIX:-}"

  if (( $+commands[jq] )); then
    stash ls --json --chars=120 2>/dev/null \
      | jq -j '
          .[]
          | (
              [
                ("\u001b[1;33m" + .short_id + "\u001b[0m " + .date + " \u001b[36m" + .size_human + "\u001b[0m"),
                (
                  to_entries
                  | map(
                    select(
                        .key as $k
                        | (["id", "short_id", "stack_ref", "ts", "date", "size", "size_human", "preview"] | index($k))
                        | not
                      )
                    )
                  | .[]
                  | "\u001b[36m" + .key + ":\u001b[0m " + (.value | tostring)
                ),
                (
                  (.preview // [])
                  | .[]
                  | "... " + .
                ),
                ""
              ]
              | flatten
              | join("\n")
            ) + "\u0000"
        ' \
      | fzf \
          --read0 \
          --height=60% \
          --reverse \
          --border \
          --ansi \
          --query="$query" \
      | sed 's/\x1b\[[0-9;]*m//g' \
      | awk 'NR == 1 { print $1 }'
  else
    stash ls --id=short --date --size --preview --color=false 2>/dev/null \
      | fzf \
          --height=40% \
          --reverse \
          --border \
          --ansi \
          --query="$query" \
      | awk '{ print $1 }'
  fi
}

_stash_fzf_insert_match() {
  local selected="$1"
  [[ -n "$selected" ]] || return 1
  compstate[insert]=1
  compstate[list]=''
  compadd -Q -U -S ' ' -- "$selected"
}

_stash_fzf_complete_attr() {
  local current_word="${PREFIX:-${words[CURRENT]-}}"
  local attr_prefix=""
  local query="$current_word"
  local -a items
  local key count

  if [[ "$current_word" == ++* ]]; then
    attr_prefix="++"
    query="${current_word#++}"
  elif [[ "$current_word" == +* ]]; then
    attr_prefix="+"
    query="${current_word#+}"
  fi

  while IFS=$'\t' read -r key count; do
    [[ -n "$key" ]] && items+=("$key [$count]")
  done < <(stash attrs --count 2>/dev/null)
  (( ${#items} )) || return 0

  zle -I 2>/dev/null || true
  local selected
  selected="$(
    printf '%s\n' "${items[@]}" \
      | fzf \
          --height=40% \
          --reverse \
          --border \
          --query="$query"
  )" || { _stash_fzf_redraw; return 0; }
  _stash_fzf_redraw
  _stash_fzf_insert_match "${attr_prefix}${selected%% \[*}"
}

_stash_fzf_redraw() {
  zle reset-prompt 2>/dev/null || zle redisplay 2>/dev/null || true
}

_stash_fzf_complete() {
  if _stash_fzf_needs_attr; then
    _stash_fzf_complete_attr
    return
  fi

  if _stash_fzf_needs_ref; then
    zle -I 2>/dev/null || true
    local selected
    selected="$(_stash_fzf_pick_ref)" || { _stash_fzf_redraw; return 0; }
    _stash_fzf_redraw
    _stash_fzf_insert_match "$selected"
    return
  fi

  if (( _stash_fzf_has_base )); then
    _stash_completion_base "$@"
    return
  fi

  return 1
}

compdef _stash_fzf_complete stash
