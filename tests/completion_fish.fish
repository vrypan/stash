#!/usr/bin/env fish
# Tests for stash fish completion.
# Usage: fish tests/completion_fish.fish

set BINARY (count $argv >/dev/null; and echo $argv[1]; or echo ./zig-out/bin/stash-completion)
set -g PASS 0
set -g FAIL 0

# Load completions
$BINARY fish | source

# Returns completions for a given command line suffix (after 'stash '),
# stripping tab-separated descriptions.
function complete_stash
    complete --do-complete "stash $argv[1]" | string replace -r '\t.*' ''
end

function assert_contains
    set desc $argv[1]
    set input $argv[2]
    set word $argv[3]
    set completions (complete_stash $input)
    if contains -- $word $completions
        echo "PASS: $desc — '$word' offered"
        set -g PASS (math $PASS + 1)
    else
        echo "FAIL: $desc — '$word' not offered (got: "(string join ', ' $completions)")"
        set -g FAIL (math $FAIL + 1)
    end
end

function assert_excludes
    set desc $argv[1]
    set input $argv[2]
    set word $argv[3]
    set completions (complete_stash $input)
    if not contains -- $word $completions
        echo "PASS: $desc — '$word' not offered"
        set -g PASS (math $PASS + 1)
    else
        echo "FAIL: $desc — '$word' unexpectedly offered"
        set -g FAIL (math $FAIL + 1)
    end
end

# stash <tab> → subcommands
assert_contains "root: subcommands"      ""      "ls"
assert_contains "root: subcommands"      ""      "push"
assert_contains "root: subcommands"      ""      "rm"

# stash --<tab> → root flags only
assert_contains "root: flags"            "--"    "--attr"
assert_contains "root: flags"            "--"    "--version"
assert_excludes "root: no ls flags"      "--"    "--number"

# stash ls -<tab> → ls flags (fish only shows flags when token starts with -)
assert_contains "ls -: flags"           "ls -"  "--number"
assert_contains "ls -: flags"           "ls -"  "--reverse"
assert_contains "ls -: flags"           "ls -"  "--json"
assert_contains "ls -: short flags"     "ls -"  "-n"
assert_contains "ls -: short flags"     "ls -"  "-r"

# stash push -<tab> → push flags, not ls flags
assert_contains "push -: flags"         "push -" "--attr"
assert_excludes "push -: no ls flags"   "push -" "--number"

# stash rm -<tab> → rm flags
assert_contains "rm -: flags"           "rm -"  "--force"
assert_contains "rm -: flags"           "rm -"  "--attr"
assert_excludes "rm -: no ls flags"     "rm -"  "--number"

echo ""
echo "Results: $PASS passed, $FAIL failed"
test $FAIL -eq 0
