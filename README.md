<img width="1280" height="640" alt="social-banner" src="https://github.com/user-attachments/assets/c98cfa58-2976-4b8d-9e5b-d9e5314348e6" />

# ~{@ stash

`sstash` is a small, fast command line tool that saves data for future reference and integrates nicely
with command line workflows.

Stash piped output or files
```
$ echo "hello world" | stash
$ stash Cargo.toml
$ stash demos/words.gif
```

Inspect your stash
```
$ stash ls -lp

jhec0b9h  292.1K  Apr 11 05:15  words.gif   GIF89a......1......... .. !..$..... ..&..#..(..%..
z70pc1xm    777B  Apr 11 05:11  Cargo.toml  [package] name = "stash-cli" version = "0.6.0" edi
wve1pgsd     12B  Apr 11 05:08  wve1pgsd    hello world
```
Read stash entries
```
$ stash cat wve1pgsd
hello world

$stash cat @2 | wc -l
  35
```

Stash in the middle of a pipe
```
$ find src -type f | stash tee | wc -l
      15

$ stash cat @1 | head

src/cmd/rm.rs
src/cmd/ls.rs
src/cmd/log.rs
src/cmd/attr.rs
src/cmd/path.rs
src/cmd/mod.rs
src/cmd/push.rs
src/display.rs
src/lib.rs
src/preview.rs
```

Set arbitrary attributes to each entry, either at creation time or later.

```
$ stash attr wve1pgsd note=test
$ stash attr jhec0b9h note=test type=gif

$ stash ls -l -a note -a type
jhec0b9h  292.1K  Apr 11 05:15  words.gif  test  gif
wve1pgsd     12B  Apr 11 05:08  wve1pgsd   test
```

Everything is stored as plain files under `~/.shtash` (or `$STASH_DIR` if you set it).
No fancy databases or complex data formats.

```
stash ls --date --id=full --preview --reverse
01knxf1n5ffvk9jsm8wve1pgsd  Apr 11 05:08  hello world
01knxf6yj2tdgj4k8kz70pc1xm  Apr 11 05:11  [package] name = "stash-cli" version = "0.6.0" edition = "2024" d
01knxfeb2hgmybg39ajhec0b9h  Apr 11 05:15  GIF89a......1......... .. !..$..... ..&..#..(..%..+.....(..R..8..
01knxhb7pa2wnsm0djghvg4s0b  Apr 11 05:48  src/cmd/rm.rs src/cmd/ls.rs src/cmd/log.rs src/cmd/attr.rs src/cm

file .stash/data/*
.stash/data/01knxf1n5ffvk9jsm8wve1pgsd: ASCII text
.stash/data/01knxf6yj2tdgj4k8kz70pc1xm: ASCII text
.stash/data/01knxfeb2hgmybg39ajhec0b9h: GIF image data, version 89a, 1200 x 900
.stash/data/01knxhb7pa2wnsm0djghvg4s0b: ASCII text
```

## Documentation
- Usage guide, and examples: [docs/](docs)
- Scrips, and cli integrations (bash/zsh/fish, ripgrep, starship, ssh, fzf, etc.): [scripts/](scripts/)


## Installation

### From Source

Build the Rust binary and copy it to a location in your `PATH`:

```bash
cargo build --release
cp target/release/stash /usr/local/bin/stash
```

### With Cargo

```bash
cargo install stash-cli
```

### Pre-built binaries

Available under [releases](https://github.com/vrypan/stash/releases).

### Homebrew

```bash
brew install vrypan/tap/stash
```

### Shell support

For auto-completion and other shell-specific features, check:
- [docs/bash.md](docs/bash.md)
- [docs/fish.md](docs/fish.md)
- [docs/zsh.md](docs/zsh.md)

## ~{@
In case you wonder, `~{@` is an ASCII-Art acorn


