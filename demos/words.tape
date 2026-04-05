Output "words.gif"

Set Shell "bash"
Set FontSize 18
Set Width 1200
Set Height 900
Set Padding 24
Set TypingSpeed 0.02
Set Theme "Catppuccin Mocha"

Hide
Type "export STASH_DIR=$(mktemp -d)"
Enter
Type "export PATH=$PWD:$PATH"
Enter
Type "clear"
Enter
Show

Type "# Build an expensive filtered word list and stash it"
Enter
Sleep 0.8s

Type "grep -Ei '^[a-z]{8,}$' /usr/share/dict/words | grep -Ei '(.)\1' | grep -Ei '[aeiou].*[aeiou].*[aeiou]' | awk '{ print length($0), $0 }' | sort -nr |"
Enter
Type "stash -a label=filtered-words"
Enter
Sleep 1.4s

Type "# Show the stashed entry"
Enter
Sleep 0.8s

Type "stash ls -l --attr label"
Enter
Sleep 1.2s

Type "# Show the stored attributes"
Enter
Sleep 0.8s

Type "stash attr @1"
Enter
Sleep 1.6s

Type "# Use the stashed output to build a frequency table"
Enter
Sleep 0.8s

Type "stash cat @1 |"
Enter
Type "awk '{ print substr($2,1,1) }' | sort | uniq -c | sort -nr | pr -6 -t"
Enter
Sleep 1.6s

Type "# Use the same stashed output to show the first 10 rows as a table"
Enter
Sleep 0.8s

Type "{ printf 'length word\n'; stash cat @1 | head -n 10; } | column -t"
Enter
Sleep 3s
