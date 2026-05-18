const std = @import("std");
const cli = @import("cli");

pub const CompletionSpec = struct {
    command: []const u8,
    commands: []const cli.CommandEntry,
    root: cli.CommandSpec,
    subcommands: []const cli.CommandSpec,
    help_flag: cli.FlagSpec = .{ .name = "help", .short = 'h', .description = "Print help" },
};

fn takesValue(spec: cli.FlagSpec) bool {
    return spec.value != .none;
}

fn hasFileArg(arguments: []const cli.ArgumentSpec) bool {
    for (arguments) |arg| if (std.mem.eql(u8, arg.name, "FILE")) return true;
    return false;
}

// ── Bash ─────────────────────────────────────────────────────────────────────

pub fn generateBash(writer: anytype, spec: CompletionSpec) !void {
    try writer.print("_{s}() {{\n", .{spec.command});
    try writer.writeAll(
        \\    local cur prev
        \\    cur="${COMP_WORDS[COMP_CWORD]}"
        \\    prev="${COMP_WORDS[COMP_CWORD-1]}"
        \\    _init_completion 2>/dev/null
        \\    COMPREPLY=()
        \\
        \\    local subcmd=""
        \\    local i
        \\    for ((i=1; i<COMP_CWORD; i++)); do
        \\        case "${COMP_WORDS[i]}" in
        \\
    );
    try writer.writeAll("            ");
    for (spec.commands, 0..) |c, i| {
        if (i > 0) try writer.writeByte('|');
        try writer.writeAll(c.name);
    }
    try writer.writeAll(
        \\)
        \\                subcmd="${COMP_WORDS[i]}"
        \\                break
        \\                ;;
        \\        esac
        \\    done
        \\
        \\    if [[ -z "$subcmd" ]]; then
        \\        if [[ "$cur" == -* ]]; then
        \\
    );
    try writer.writeAll("            COMPREPLY=($(compgen -W \"");
    try writeBashFlagWords(writer, spec.root.flags);
    try writeBashFlagWords(writer, &.{spec.help_flag});
    try writer.writeAll("\" -- \"$cur\"))\n");
    try writer.writeAll(
        \\        else
        \\            COMPREPLY=($(compgen -W "
    );
    for (spec.commands) |c| try writer.print("{s} ", .{c.name});
    try writer.writeAll(
        \\" -- "$cur"))
        \\        fi
        \\        return
        \\    fi
        \\
        \\    case "$subcmd" in
        \\
    );
    for (spec.subcommands) |sub| {
        try writer.print("        {s})\n", .{sub.name});
        try writer.writeAll("            COMPREPLY=($(compgen -W \"");
        try writeBashFlagWords(writer, sub.flags);
        try writeBashFlagWords(writer, &.{spec.help_flag});
        try writer.writeAll("\" -- \"$cur\"))\n");
        if (hasFileArg(sub.arguments)) {
            try writer.writeAll("            [[ ${#COMPREPLY[@]} -eq 0 ]] && COMPREPLY=($(compgen -f -- \"$cur\"))\n");
        }
        try writer.writeAll("            ;;\n");
    }
    try writer.writeAll("    esac\n}\n\n");
    try writer.print("complete -F _{s} {s}\n", .{ spec.command, spec.command });
}

fn writeBashFlagWords(writer: anytype, flags: []const cli.FlagSpec) !void {
    for (flags) |flag| {
        try writer.print("--{s} ", .{flag.name});
        if (flag.short) |s| try writer.print("-{c} ", .{s});
    }
}

// ── Zsh ──────────────────────────────────────────────────────────────────────

pub fn generateZsh(writer: anytype, spec: CompletionSpec) !void {
    try writer.print("_{s}() {{\n", .{spec.command});
    try writer.writeAll(
        \\    local state subcmd
        \\    local -a commands
        \\
        \\    commands=(
        \\
    );
    for (spec.commands) |c| try writer.print("        \"{s}:{s}\"\n", .{ c.name, c.description });
    try writer.writeAll(
        \\    )
        \\
        \\    if (( CURRENT == 2 )); then
        \\        _arguments -C \
        \\
    );
    for (spec.root.flags) |flag| try writeZshFlag(writer, "            ", flag);
    try writeZshFlag(writer, "            ", spec.help_flag);
    try writer.writeAll(
        \\            ':command:->command'
        \\        case $state in
        \\            command) _describe 'command' commands ;;
        \\        esac
        \\        return
        \\    fi
        \\
        \\    subcmd="${words[2]}"
        \\    words=(${words[1]} ${words[3,-1]})
        \\    (( CURRENT-- ))
        \\    case "$subcmd" in
        \\
    );
    for (spec.subcommands) |sub| {
        try writer.print("        {s})\n", .{sub.name});
        try writer.writeAll("            _arguments \\\n");
        for (sub.flags) |flag| try writeZshFlag(writer, "                ", flag);
        try writeZshFlag(writer, "                ", spec.help_flag);
        for (sub.arguments) |arg| try writeZshArg(writer, "                ", arg);
        try writer.writeAll("                && return\n");
        if (hasFileArg(sub.arguments)) try writer.writeAll("            _files\n");
        try writer.writeAll("            ;;\n");
    }
    try writer.writeAll("    esac\n}\n\n");
    try writer.print("compdef _{s} {s}\n", .{ spec.command, spec.command });
}

fn writeZshArg(writer: anytype, indent: []const u8, arg: cli.ArgumentSpec) !void {
    const action = if (std.mem.eql(u8, arg.name, "FILE")) "_files" else "";
    if (arg.repeatable) {
        try writer.print("{s}'*:{s}:{s}' \\\n", .{ indent, arg.name, action });
    } else if (arg.required) {
        try writer.print("{s}':{s}:{s}' \\\n", .{ indent, arg.name, action });
    } else {
        try writer.print("{s}'::{s}:{s}' \\\n", .{ indent, arg.name, action });
    }
}

fn writeZshFlag(writer: anytype, indent: []const u8, flag: cli.FlagSpec) !void {
    if (flag.short) |s| {
        try writer.print("{s}'(-{c} --{s})'", .{ indent, s, flag.name });
        try writer.writeByte('{');
        try writer.print("-{c},--{s}", .{ s, flag.name });
        try writer.writeByte('}');
        if (takesValue(flag)) {
            try writer.print("'[{s}]:value:' \\\n", .{flag.name});
        } else {
            try writer.print("'[{s}]' \\\n", .{flag.name});
        }
    } else {
        if (takesValue(flag)) {
            try writer.print("{s}'--{s}[{s}]:value:' \\\n", .{ indent, flag.name, flag.name });
        } else {
            try writer.print("{s}'--{s}[{s}]' \\\n", .{ indent, flag.name, flag.name });
        }
    }
}

// ── Fish ─────────────────────────────────────────────────────────────────────

pub fn generateFish(writer: anytype, spec: CompletionSpec) !void {
    try writer.print("# fish completion for {s}\n\n", .{spec.command});
    try writer.print("complete -c {s} -f\n\n", .{spec.command});

    try writer.print("function __{s}_no_subcommand\n", .{spec.command});
    try writer.writeAll("    for i in (commandline -opc)\n");
    try writer.writeAll("        switch $i\n");
    try writer.writeAll("            case");
    for (spec.commands) |c| try writer.print(" {s}", .{c.name});
    try writer.writeAll(
        \\
        \\                return 1
        \\        end
        \\    end
        \\    return 0
        \\end
        \\
        \\
    );

    const no_sub_fn = try std.fmt.allocPrint(std.heap.page_allocator, "__{s}_no_subcommand", .{spec.command});
    defer std.heap.page_allocator.free(no_sub_fn);

    for (spec.commands) |c| {
        try writer.print("complete -c {s} -n {s} -a {s} -d \"{s}\"\n", .{ spec.command, no_sub_fn, c.name, c.description });
    }
    try writer.writeByte('\n');

    try writer.writeAll("# Root flags\n");
    try writeFishFlags(writer, spec.command, no_sub_fn, spec.root.flags, spec.help_flag);
    try writer.writeByte('\n');

    for (spec.subcommands) |sub| {
        try writer.print("# {s}\n", .{sub.name});
        try writeFishFlags(writer, spec.command, sub.name, sub.flags, spec.help_flag);
        if (hasFileArg(sub.arguments)) {
            try writer.print("complete -c {s} -n '__fish_seen_subcommand_from {s}' -F\n", .{ spec.command, sub.name });
        }
        try writer.writeByte('\n');
    }
}

fn writeFishFlags(writer: anytype, command: []const u8, cond: []const u8, flags: []const cli.FlagSpec, help_flag: cli.FlagSpec) !void {
    for (flags) |flag| try writeFishFlag(writer, command, cond, flag);
    try writeFishFlag(writer, command, cond, help_flag);
}

fn writeFishFlag(writer: anytype, command: []const u8, cond: []const u8, flag: cli.FlagSpec) !void {
    const is_func = std.mem.startsWith(u8, cond, "__");
    const n = flag.short != null;
    const r = takesValue(flag);
    if (n) {
        const s = flag.short.?;
        if (is_func) {
            try writer.print("complete -c {s} -n {s} -l {s} -s {c}{s}\n", .{ command, cond, flag.name, s, if (r) " -r" else "" });
        } else {
            try writer.print("complete -c {s} -n '__fish_seen_subcommand_from {s}' -l {s} -s {c}{s}\n", .{ command, cond, flag.name, s, if (r) " -r" else "" });
        }
    } else {
        if (is_func) {
            try writer.print("complete -c {s} -n {s} -l {s}{s}\n", .{ command, cond, flag.name, if (r) " -r" else "" });
        } else {
            try writer.print("complete -c {s} -n '__fish_seen_subcommand_from {s}' -l {s}{s}\n", .{ command, cond, flag.name, if (r) " -r" else "" });
        }
    }
}
