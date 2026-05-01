const std = @import("std");
const cli = @import("cli.zig");
const cmd = @import("cmd.zig");

const help_flag = cli.FlagSpec{ .name = "help", .short = 'h', .description = "Print help" };

const subcommand_specs = [_]cli.CommandSpec{
    cmd.push_spec,
    cmd.tee_spec,
    cmd.cat_spec,
    cmd.ls_spec,
    cmd.attr_spec,
    cmd.attrs_spec,
    cmd.path_spec,
    cmd.rm_spec,
};

fn takesValue(spec: cli.FlagSpec) bool {
    return spec.value != .none;
}

pub fn generateBash(writer: anytype) !void {
    try writer.writeAll(
        \\_stash() {
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
        \\            attr|attrs|cat|ls|path|push|rm|tee)
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
    try writeBashFlagWords(writer, cmd.root_spec.flags);
    try writeBashFlagWords(writer, &.{help_flag});
    try writer.writeAll("\" -- \"$cur\"))\n");
    try writer.writeAll(
        \\        else
        \\            COMPREPLY=($(compgen -W "
    );
    for (cmd.commands) |c| try writer.print("{s} ", .{c.name});
    try writer.writeAll(
        \\" -- "$cur"))
        \\        fi
        \\        return
        \\    fi
        \\
        \\    case "$subcmd" in
        \\
    );
    for (subcommand_specs) |spec| {
        try writer.print("        {s})\n", .{spec.name});
        try writer.writeAll("            COMPREPLY=($(compgen -W \"");
        try writeBashFlagWords(writer, spec.flags);
        try writeBashFlagWords(writer, &.{help_flag});
        try writer.writeAll("\" -- \"$cur\"))\n");
        if (hasFileArg(spec.arguments)) {
            try writer.writeAll("            [[ ${#COMPREPLY[@]} -eq 0 ]] && COMPREPLY=($(compgen -f -- \"$cur\"))\n");
        }
        try writer.writeAll("            ;;\n");
    }
    try writer.writeAll(
        \\    esac
        \\}
        \\
        \\complete -F _stash stash
        \\
    );
}

fn writeBashFlagWords(writer: anytype, flags: []const cli.FlagSpec) !void {
    for (flags) |flag| {
        try writer.print("--{s} ", .{flag.name});
        if (flag.short) |s| try writer.print("-{c} ", .{s});
    }
}

fn hasFileArg(arguments: []const cli.ArgumentSpec) bool {
    for (arguments) |arg| if (std.mem.eql(u8, arg.name, "FILE")) return true;
    return false;
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

pub fn generateZsh(writer: anytype) !void {
    try writer.writeAll(
        \\_stash() {
        \\    local state subcmd
        \\    local -a commands
        \\
        \\    commands=(
        \\
    );
    for (cmd.commands) |c| try writer.print("        \"{s}:{s}\"\n", .{ c.name, c.description });
    try writer.writeAll(
        \\    )
        \\
        \\    if (( CURRENT == 2 )); then
        \\        _arguments -C \
        \\
    );
    for (cmd.root_spec.flags) |flag| try writeZshFlag(writer, "            ", flag);
    try writeZshFlag(writer, "            ", help_flag);
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
    for (subcommand_specs) |spec| {
        try writer.print("        {s})\n", .{spec.name});
        try writer.writeAll("            _arguments \\\n");
        for (spec.flags) |flag| try writeZshFlag(writer, "                ", flag);
        try writeZshFlag(writer, "                ", help_flag);
        for (spec.arguments) |arg| try writeZshArg(writer, "                ", arg);
        try writer.writeAll("                && return\n");
        if (hasFileArg(spec.arguments)) try writer.writeAll("            _files\n");
        try writer.writeAll("            ;;\n");
    }
    try writer.writeAll(
        \\    esac
        \\}
        \\
        \\compdef _stash stash
        \\
    );
}

pub fn generateFish(writer: anytype) !void {
    try writer.writeAll("# fish completion for stash\n\n");
    try writer.writeAll("complete -c stash -f\n\n");

    try writer.writeAll("function __stash_no_subcommand\n");
    try writer.writeAll("    for i in (commandline -opc)\n");
    try writer.writeAll("        switch $i\n");
    try writer.writeAll("            case");
    for (cmd.commands) |c| try writer.print(" {s}", .{c.name});
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

    for (cmd.commands) |c| {
        try writer.print("complete -c stash -n __stash_no_subcommand -a {s} -d \"{s}\"\n", .{ c.name, c.description });
    }
    try writer.writeByte('\n');

    try writer.writeAll("# Root flags\n");
    try writeFishFlags(writer, "__stash_no_subcommand", cmd.root_spec.flags);
    try writeFishFlag(writer, "__stash_no_subcommand", help_flag);
    try writer.writeByte('\n');

    for (subcommand_specs) |spec| {
        try writer.print("# {s}\n", .{spec.name});
        try writeFishFlags(writer, spec.name, spec.flags);
        try writeFishFlag(writer, spec.name, help_flag);
        if (hasFileArg(spec.arguments)) {
            try writer.print("complete -c stash -n '__fish_seen_subcommand_from {s}' -F\n", .{spec.name});
        }
        try writer.writeByte('\n');
    }
}

fn writeFishFlags(writer: anytype, cond: []const u8, flags: []const cli.FlagSpec) !void {
    for (flags) |flag| try writeFishFlag(writer, cond, flag);
}

fn writeFishFlag(writer: anytype, cond: []const u8, flag: cli.FlagSpec) !void {
    // For subcommand conditions use __fish_seen_subcommand_from, for the
    // no-subcommand guard use the function name directly.
    const is_func = std.mem.startsWith(u8, cond, "__stash");
    if (flag.short) |s| {
        if (takesValue(flag)) {
            if (is_func) {
                try writer.print("complete -c stash -n {s} -l {s} -s {c} -r\n", .{ cond, flag.name, s });
            } else {
                try writer.print("complete -c stash -n '__fish_seen_subcommand_from {s}' -l {s} -s {c} -r\n", .{ cond, flag.name, s });
            }
        } else {
            if (is_func) {
                try writer.print("complete -c stash -n {s} -l {s} -s {c}\n", .{ cond, flag.name, s });
            } else {
                try writer.print("complete -c stash -n '__fish_seen_subcommand_from {s}' -l {s} -s {c}\n", .{ cond, flag.name, s });
            }
        }
    } else {
        if (takesValue(flag)) {
            if (is_func) {
                try writer.print("complete -c stash -n {s} -l {s} -r\n", .{ cond, flag.name });
            } else {
                try writer.print("complete -c stash -n '__fish_seen_subcommand_from {s}' -l {s} -r\n", .{ cond, flag.name });
            }
        } else {
            if (is_func) {
                try writer.print("complete -c stash -n {s} -l {s}\n", .{ cond, flag.name });
            } else {
                try writer.print("complete -c stash -n '__fish_seen_subcommand_from {s}' -l {s}\n", .{ cond, flag.name });
            }
        }
    }
}
