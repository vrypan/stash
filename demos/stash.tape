Output "stash.gif"

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

Type "curl -s 'https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/2.5_month.geojson' | "
Enter
Sleep 0.1
Type "stash -a source=usgs-earthquakes -a stage=raw --print"
Enter

Sleep 2s
Type "stash ls -l"
Enter

Sleep 3s
Type "stash attr @1"
Enter

Sleep 3s
Type "stash cat @1 | jq '{title: .metadata.title, count: .metadata.count}' | stash -a source=usgs-earthquakes -a stage=summary"
Enter

Sleep 3s
Type "stash ls -l --attr stage"
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

Sleep 4s
Type "stash rm --before @2 -f"
Enter

Sleep 2s
Type "stash ls"
Enter

Sleep 5s
