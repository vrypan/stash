#!/usr/bin/env zsh
# Tests for stash zsh completion.
# Usage: zsh tests/completion_zsh.zsh

BINARY="${1:-./zig-out/bin/stash-completion}"

typeset -gi PASS=0 FAIL=0
typeset -ga _test_completions
typeset -g state PREFIX

# Stub compdef before sourcing (it's called during source in non-interactive shell).
compdef() { : }

# Load the completion, then override the utilities it calls with mocks.
source <("$BINARY" zsh)

# Mock _describe: captures items from the named array that match PREFIX.
_describe() {
    local msg=$1 arr_name=$2
    local _ref="${arr_name}[@]"
    local -a arr
    arr=("${(P)_ref}")
    local item word
    for item in "${arr[@]}"; do
        word="${item%%:*}"
        [[ -z $PREFIX || $word == ${PREFIX}* ]] && _test_completions+=("$word")
    done
}

# Mock _arguments: extracts flag names from spec strings and captures matches.
# Handles '(-x --flag)'{-x,--flag}'[desc]:val:' after brace expansion, and
# sets state=command when a ->command transition spec is present.
_arguments() {
    local spec flag
    for spec in "$@"; do
        [[ $spec == '-C' ]] && continue
        if [[ $spec == *'->command' ]]; then
            state=command
            continue
        fi
        flag="$spec"
        # Strip exclusion list '(-x --flag)' prefix
        [[ $flag == \(*\)* ]] && flag="${flag#*\)}"
        # Strip description '[...]' and value spec ':...'
        flag="${flag%%\[*}"
        flag="${flag%%:*}"
        if [[ $flag == --* || $flag == -[a-zA-Z] ]]; then
            [[ -z $PREFIX || $flag == ${PREFIX}* ]] && _test_completions+=("$flag")
        fi
    done
}

_files() { _test_completions+=('<files>') }

# Simulate completion for a given command line string.
# A trailing space means completing a new empty word.
complete_stash() {
    local input=$1
    _test_completions=()
    state=''
    if [[ $input == *' ' ]]; then
        local trimmed="${input%% }"
        words=("${=trimmed}" '')
    else
        words=("${=input}")
    fi
    CURRENT=${#words}
    PREFIX="${words[$CURRENT]}"
    _stash
}

assert_contains() {
    local desc=$1 word=$2
    if (( ${_test_completions[(I)$word]} > 0 )); then
        print "PASS: $desc — '$word' offered"
        (( PASS++ )) || true
    else
        print "FAIL: $desc — '$word' not in: ${(j:, :)_test_completions}"
        (( FAIL++ )) || true
    fi
}

assert_excludes() {
    local desc=$1 word=$2
    if (( ${_test_completions[(I)$word]} == 0 )); then
        print "PASS: $desc — '$word' not offered"
        (( PASS++ )) || true
    else
        print "FAIL: $desc — '$word' unexpectedly in: ${(j:, :)_test_completions}"
        (( FAIL++ )) || true
    fi
}

# stash <tab> → subcommands
complete_stash "stash "
assert_contains "root: subcommands"      "ls"
assert_contains "root: subcommands"      "push"
assert_contains "root: subcommands"      "rm"

# stash --<tab> → root flags only
complete_stash "stash --"
assert_contains "root: flags"            "--attr"
assert_contains "root: flags"            "--version"
assert_excludes "root: no ls flags"      "--number"

# stash ls <tab> → ls flags
complete_stash "stash ls "
assert_contains "ls: flags"              "--number"
assert_contains "ls: flags"              "--reverse"
assert_contains "ls: flags"              "--json"

# stash ls -<tab> → ls flags
complete_stash "stash ls -"
assert_contains "ls -: flags"            "--number"
assert_contains "ls -: short flags"      "-n"
assert_contains "ls -: short flags"      "-r"

# stash ls --n<tab> → --name, --number only
complete_stash "stash ls --n"
assert_contains "ls --n: --number"       "--number"
assert_contains "ls --n: --name"         "--name"
assert_excludes "ls --n: no --json"      "--json"

# stash push <tab> → push flags, not ls flags
complete_stash "stash push "
assert_contains "push: flags"            "--attr"
assert_excludes "push: no ls flags"      "--number"

# stash rm <tab> → rm flags
complete_stash "stash rm "
assert_contains "rm: flags"              "--force"
assert_contains "rm: flags"             "--attr"
assert_excludes "rm: no ls flags"        "--number"

print ""
print "Results: $PASS passed, $FAIL failed"
(( FAIL == 0 ))
