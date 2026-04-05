Output "demos/stash.gif"

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
Type "export SVC=api"
Enter
Type "export PATH=$PWD:$PATH"
Enter
Type "clear"
Enter
Show

Type "jq -n '{service:env.SVC, ok:true, latency_ms:18}' | stash -a label=raw"
Enter
Sleep 3s

Type "stash ls -l"
Enter
Sleep 3s

Type "stash attr @1"
Enter
Sleep 3s

Type "stash cat @1 | jq '{service, latency_ms}' | stash -a label=reduced"
Enter
Sleep 3s

Type "stash ls -l --attr label"
Enter
Sleep 3s

Type "stash cat @1 | jq ."
Enter
Sleep 3s

Type "printf 'alpha\nbeta\ngamma\n' | stash tee | wc -l"
Enter
Sleep 3s

Type "stash log -n 2"
Enter
Sleep 3s

Type "stash rm --before @2 -f"
Enter
Sleep 2s

Type "stash ls"
Enter
Sleep 3s
