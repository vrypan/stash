# lib/cli

A self-contained Zig library for CLI argument parsing and shell completion generation. Copy the two files to any project and wire them up as named modules in `build.zig`.

## Files

- **`cli.zig`** — flag/argument parsing, help formatting
- **`completion.zig`** — generates bash, zsh, and fish completion scripts

## Usage

```zig
// build.zig
const cli_mod = b.createModule(.{ .root_source_file = b.path("lib/cli/cli.zig") });
const completion_mod = b.createModule(.{ .root_source_file = b.path("lib/cli/completion.zig") });
completion_mod.addImport("cli", cli_mod);

exe.root_module.addImport("cli", cli_mod);
exe.root_module.addImport("completion", completion_mod);
```

```zig
// your_completion_main.zig
const cli = @import("cli");
const completion = @import("completion");

const spec = completion.CompletionSpec{
    .command = "mytool",
    .commands = &commands,       // []const cli.CommandEntry
    .root = root_spec,           // cli.CommandSpec
    .subcommands = &subcommands, // []const cli.CommandSpec
};

try completion.generateBash(writer, spec);
try completion.generateZsh(writer, spec);
try completion.generateFish(writer, spec);
```
