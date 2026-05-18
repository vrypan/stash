#!/usr/bin/env bash
# Tests for stash bash completion.
# Usage: bash tests/completion_bash.sh

set -uo pipefail

BINARY="${1:-./zig-out/bin/stash-completion}"
PASS=0
FAIL=0

eval "$("$BINARY" bash)"

# Simulate tab completion for a given command line.
# The last word in the string is the word being completed.
# A trailing space means completing a new empty word.
complete_stash() {
    local input="$1"
    COMPREPLY=()
    if [[ "$input" == *" " ]]; then
        read -ra COMP_WORDS <<< "$input"
        COMP_WORDS+=("")
    else
        read -ra COMP_WORDS <<< "$input"
    fi
    COMP_CWORD=$(( ${#COMP_WORDS[@]} - 1 ))
    _stash
    echo "${COMPREPLY[@]}"
}

assert_contains() {
    local desc="$1" result="$2" word="$3"
    if [[ " $result " == *" $word "* ]]; then
        echo "PASS: $desc — '$word' offered"
        PASS=$(( PASS + 1 ))
    else
        echo "FAIL: $desc — '$word' not in: $result"
        FAIL=$(( FAIL + 1 ))
    fi
}

assert_excludes() {
    local desc="$1" result="$2" word="$3"
    if [[ " $result " != *" $word "* ]]; then
        echo "PASS: $desc — '$word' not offered"
        PASS=$(( PASS + 1 ))
    else
        echo "FAIL: $desc — '$word' unexpectedly offered in: $result"
        FAIL=$(( FAIL + 1 ))
    fi
}

# stash <tab> → subcommands
result=$(complete_stash "stash ")
assert_contains "root: subcommands"   "$result" "ls"
assert_contains "root: subcommands"   "$result" "push"
assert_contains "root: subcommands"   "$result" "cat"
assert_contains "root: subcommands"   "$result" "rm"

# stash --<tab> → root flags
result=$(complete_stash "stash --")
assert_contains "root: flags"         "$result" "--attr"
assert_contains "root: flags"         "$result" "--version"
assert_excludes "root: flags no cmd"  "$result" "ls"

# stash ls <tab> → ls flags
result=$(complete_stash "stash ls ")
assert_contains "ls: flags"           "$result" "--number"
assert_contains "ls: flags"           "$result" "--reverse"
assert_contains "ls: flags"           "$result" "--json"

# stash ls -<tab> → ls flags
result=$(complete_stash "stash ls -")
assert_contains "ls -: flags"         "$result" "--number"
assert_contains "ls -: short flags"   "$result" "-n"
assert_contains "ls -: short flags"   "$result" "-r"

# stash ls --n<tab> → --name, --number
result=$(complete_stash "stash ls --n")
assert_contains "ls --n: --number"    "$result" "--number"
assert_contains "ls --n: --name"      "$result" "--name"
assert_excludes "ls --n: no --json"   "$result" "--json"

# stash push <tab> → push flags (not ls flags)
result=$(complete_stash "stash push ")
assert_contains "push: flags"         "$result" "--attr"
assert_excludes "push: no ls flags"   "$result" "--number"

# stash rm <tab> → rm flags
result=$(complete_stash "stash rm ")
assert_contains "rm: flags"           "$result" "--force"
assert_contains "rm: flags"           "$result" "--attr"
assert_excludes "rm: no ls flags"     "$result" "--number"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[[ $FAIL -eq 0 ]]
