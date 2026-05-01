const std = @import("std");

const Allocator = std.mem.Allocator;

pub const ValueKind = enum {
    none,
    string,
    int,
    bool_required,
    bool_optional,
};

pub const FlagSpec = struct {
    name: []const u8,
    short: ?u8 = null,
    value: ValueKind = .none,
    value_name: ?[]const u8 = null,
    description: []const u8 = "",
    default_value: ?[]const u8 = null,
    repeatable: bool = false,
    attached_short_value: bool = false,
};

pub const CommandSpec = struct {
    name: []const u8,
    description: []const u8,
    usage: []const u8,
    flags: []const FlagSpec = &.{},
    arguments: []const ArgumentSpec = &.{},
    extra_help: ?[]const u8 = null,
};

pub const ArgumentSpec = struct {
    name: []const u8,
    description: []const u8 = "",
    required: bool = false,
    repeatable: bool = false,
};

pub const CommandEntry = struct {
    name: []const u8,
    description: []const u8,
};

const help_line_width = 120;

pub const FlagValue = struct {
    name: []const u8,
    value: ?[]const u8 = null,
};

pub const Parsed = struct {
    flags: std.ArrayList(FlagValue) = .empty,
    positionals: std.ArrayList([]const u8) = .empty,

    pub fn present(self: *const Parsed, name: []const u8) bool {
        for (self.flags.items) |flag| {
            if (std.mem.eql(u8, flag.name, name)) return true;
        }
        return false;
    }

    pub fn last(self: *const Parsed, name: []const u8) ?[]const u8 {
        var i = self.flags.items.len;
        while (i > 0) {
            i -= 1;
            const flag = self.flags.items[i];
            if (std.mem.eql(u8, flag.name, name)) return flag.value;
        }
        return null;
    }
};

const ParseIssue = enum {
    unknown_option,
    missing_value,
    invalid_int,
    invalid_bool,
    unexpected_inline_value,
    unsupported_short_cluster,
    attached_short_value,
    too_few_arguments,
    too_many_arguments,
};

const ParseDiagnostic = struct {
    issue: ParseIssue = .unknown_option,
    token: []const u8 = "",
    flag_name: ?[]const u8 = null,
    value: ?[]const u8 = null,
    expected: ?[]const u8 = null,
};

pub fn parseCommand(allocator: Allocator, writer: anytype, args: []const [:0]const u8, spec: CommandSpec) !Parsed {
    var diagnostic = ParseDiagnostic{};
    const parsed = parseInternal(allocator, args, spec.flags, &diagnostic) catch |err| {
        if (err != error.InvalidArgument) return err;
        try printParseError(writer, spec, diagnostic);
        return error.ReportedCliError;
    };
    validateArguments(parsed.positionals.items, spec.arguments, &diagnostic) catch |err| {
        if (err != error.InvalidArgument) return err;
        try printParseError(writer, spec, diagnostic);
        return error.ReportedCliError;
    };
    return parsed;
}

pub fn parse(allocator: Allocator, args: []const [:0]const u8, specs: []const FlagSpec) !Parsed {
    return parseInternal(allocator, args, specs, null);
}

fn parseInternal(allocator: Allocator, args: []const [:0]const u8, specs: []const FlagSpec, diagnostic: ?*ParseDiagnostic) !Parsed {
    var parsed = Parsed{};
    var i: usize = 0;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (arg.len == 0) {
            try parsed.positionals.append(allocator, arg);
            continue;
        }
        if (arg[0] != '-') {
            try parsed.positionals.append(allocator, arg);
            continue;
        }
        if (arg.len == 1) {
            try parsed.positionals.append(allocator, arg);
            continue;
        }
        if (arg[1] == '-') {
            if (arg.len == 2) {
                // "--" separator
                i += 1;
                while (i < args.len) : (i += 1) try parsed.positionals.append(allocator, args[i]);
                break;
            }
            try parseLong(allocator, args, &i, specs, &parsed, diagnostic);
        } else {
            try parseShort(allocator, args, &i, specs, &parsed, diagnostic);
        }
    }
    return parsed;
}

fn parseLong(allocator: Allocator, args: []const [:0]const u8, index: *usize, specs: []const FlagSpec, parsed: *Parsed, diagnostic: ?*ParseDiagnostic) !void {
    const arg = args[index.*];
    const raw = arg[2..];
    const eql = std.mem.indexOfScalar(u8, raw, '=');
    const name = if (eql) |pos| raw[0..pos] else raw;
    const inline_value = if (eql) |pos| raw[pos + 1 ..] else null;
    const spec = findLong(specs, name) orelse {
        setDiagnostic(diagnostic, .{ .issue = .unknown_option, .token = arg });
        return error.InvalidArgument;
    };
    const value = try consumeValue(args, index, spec, inline_value, arg, diagnostic);
    try appendFlag(allocator, parsed, spec, value);
}

fn parseShort(allocator: Allocator, args: []const [:0]const u8, index: *usize, specs: []const FlagSpec, parsed: *Parsed, diagnostic: ?*ParseDiagnostic) !void {
    const arg = args[index.*];
    const body = arg[1..];
    if (body.len == 0) return error.InvalidArgument;
    const spec = findShort(specs, body[0]) orelse {
        setDiagnostic(diagnostic, .{ .issue = .unknown_option, .token = arg });
        return error.InvalidArgument;
    };
    const inline_value: ?[]const u8 = if (body.len > 1) blk: {
        if (spec.value == .none) {
            setDiagnostic(diagnostic, .{ .issue = .unsupported_short_cluster, .token = arg });
            return error.InvalidArgument;
        }
        if (!spec.attached_short_value) {
            setDiagnostic(diagnostic, .{ .issue = .attached_short_value, .token = arg, .flag_name = spec.name });
            return error.InvalidArgument;
        }
        break :blk body[1..];
    } else null;
    const value = try consumeValue(args, index, spec, inline_value, arg, diagnostic);
    try appendFlag(allocator, parsed, spec, value);
}

fn consumeValue(args: []const [:0]const u8, index: *usize, spec: FlagSpec, inline_value: ?[]const u8, token: []const u8, diagnostic: ?*ParseDiagnostic) !?[]const u8 {
    switch (spec.value) {
        .none => {
            if (inline_value != null) {
                setDiagnostic(diagnostic, .{ .issue = .unexpected_inline_value, .token = token, .flag_name = spec.name });
                return error.InvalidArgument;
            }
            return null;
        },
        .string, .int, .bool_required => {
            const raw = inline_value orelse blk: {
                index.* += 1;
                if (index.* >= args.len) {
                    setDiagnostic(diagnostic, .{ .issue = .missing_value, .token = token, .flag_name = spec.name, .expected = valueName(spec) });
                    return error.InvalidArgument;
                }
                break :blk args[index.*];
            };
            if (spec.value == .int) _ = std.fmt.parseInt(usize, raw, 10) catch {
                setDiagnostic(diagnostic, .{ .issue = .invalid_int, .token = token, .flag_name = spec.name, .value = raw, .expected = valueName(spec) });
                return error.InvalidArgument;
            };
            if (spec.value == .bool_required) _ = parseBool(raw) catch {
                setDiagnostic(diagnostic, .{ .issue = .invalid_bool, .token = token, .flag_name = spec.name, .value = raw, .expected = valueName(spec) });
                return error.InvalidArgument;
            };
            return raw;
        },
        .bool_optional => {
            const raw = inline_value orelse blk: {
                if (index.* + 1 < args.len) {
                    const next = args[index.* + 1];
                    if (next.len == 0 or next[0] != '-') {
                        index.* += 1;
                        break :blk next;
                    }
                }
                return "true";
            };
            _ = parseBool(raw) catch {
                setDiagnostic(diagnostic, .{ .issue = .invalid_bool, .token = token, .flag_name = spec.name, .value = raw, .expected = valueName(spec) });
                return error.InvalidArgument;
            };
            return raw;
        },
    }
}

fn appendFlag(allocator: Allocator, parsed: *Parsed, spec: FlagSpec, value: ?[]const u8) !void {
    if (!spec.repeatable) {
        const items = parsed.flags.items;
        for (items) |*item| {
            if (std.mem.eql(u8, item.name, spec.name)) {
                item.value = value;
                return;
            }
        }
    }
    try parsed.flags.append(allocator, .{ .name = spec.name, .value = value });
}

fn findLong(specs: []const FlagSpec, name: []const u8) ?FlagSpec {
    for (specs) |spec| {
        if (spec.name.len == name.len and std.mem.eql(u8, spec.name, name)) return spec;
    }
    return null;
}

fn findShort(specs: []const FlagSpec, short: u8) ?FlagSpec {
    for (specs) |spec| if (spec.short == short) return spec;
    return null;
}

fn validateArguments(positionals: []const []const u8, arguments: []const ArgumentSpec, diagnostic: *ParseDiagnostic) !void {
    var min: usize = 0;
    var max: usize = 0;
    var unlimited = false;
    for (arguments, 0..) |argument, i| {
        if (argument.required) min += 1;
        if (argument.repeatable) {
            std.debug.assert(i == arguments.len - 1);
            unlimited = true;
        } else {
            max += 1;
        }
    }
    if (positionals.len < min) {
        diagnostic.* = .{ .issue = .too_few_arguments, .expected = expectedArguments(arguments) };
        return error.InvalidArgument;
    }
    if (!unlimited and positionals.len > max) {
        diagnostic.* = .{ .issue = .too_many_arguments, .value = positionals[positionals.len - 1], .expected = expectedArguments(arguments) };
        return error.InvalidArgument;
    }
}

fn expectedArguments(arguments: []const ArgumentSpec) []const u8 {
    for (arguments) |argument| if (argument.required) return argument.name;
    if (arguments.len > 0) return arguments[0].name;
    return "";
}

fn setDiagnostic(diagnostic: ?*ParseDiagnostic, value: ParseDiagnostic) void {
    if (diagnostic) |target| target.* = value;
}

pub fn parseBool(value: []const u8) !bool {
    if (value.len == 0) return error.InvalidArgument;
    switch (value[0]) {
        't', 'T' => if (std.ascii.eqlIgnoreCase(value, "true")) return true,
        'f', 'F' => if (std.ascii.eqlIgnoreCase(value, "false")) return false,
        'y', 'Y' => if (std.ascii.eqlIgnoreCase(value, "yes")) return true,
        'n', 'N' => if (std.ascii.eqlIgnoreCase(value, "no")) return false,
        '1' => if (value.len == 1) return true,
        '0' => if (value.len == 1) return false,
        else => {},
    }
    return error.InvalidArgument;
}

fn printParseError(writer: anytype, spec: CommandSpec, diagnostic: ParseDiagnostic) !void {
    switch (diagnostic.issue) {
        .unknown_option => try writer.print("error: unknown option '{s}'\n", .{diagnostic.token}),
        .missing_value => try writer.print("error: option '--{s}' requires <{s}>\n", .{ diagnostic.flag_name orelse diagnostic.token, diagnostic.expected orelse "VALUE" }),
        .invalid_int => try writer.print("error: invalid value for '--{s}': expected {s}, got '{s}'\n", .{ diagnostic.flag_name orelse diagnostic.token, diagnostic.expected orelse "N", diagnostic.value orelse "" }),
        .invalid_bool => try writer.print("error: invalid value for '--{s}': expected {s}, got '{s}'\n", .{ diagnostic.flag_name orelse diagnostic.token, diagnostic.expected orelse "BOOL", diagnostic.value orelse "" }),
        .unexpected_inline_value => try writer.print("error: option '--{s}' does not accept a value\n", .{diagnostic.flag_name orelse diagnostic.token}),
        .unsupported_short_cluster => try writer.print("error: unsupported short option cluster '{s}'\n", .{diagnostic.token}),
        .attached_short_value => try writer.print("error: option '-{c}' does not accept an attached value\n", .{shortName(spec.flags, diagnostic.flag_name)}),
        .too_few_arguments => try writer.print("error: missing required argument <{s}> for '{s}'\n", .{ diagnostic.expected orelse "ARG", commandLabel(spec) }),
        .too_many_arguments => try writer.print("error: too many arguments for '{s}'\n", .{commandLabel(spec)}),
    }
    try writer.print("\nUsage: {s}\n\nTry '{s} --help' for more information.\n", .{ spec.usage, commandLabel(spec) });
}

fn shortName(flags: []const FlagSpec, name: ?[]const u8) u8 {
    const wanted = name orelse return '?';
    for (flags) |flag| {
        if (std.mem.eql(u8, flag.name, wanted)) return flag.short orelse '?';
    }
    return '?';
}

fn commandLabel(spec: CommandSpec) []const u8 {
    const marker = std.mem.indexOf(u8, spec.usage, " [") orelse spec.usage.len;
    return spec.usage[0..marker];
}

pub fn printCommandHelp(allocator: Allocator, writer: anytype, spec: CommandSpec) !void {
    _ = try printWrapped(writer, spec.description, 0, 0);
    try writer.print("\n\nUsage: {s}\n", .{spec.usage});
    try printArguments(writer, spec.arguments);
    try printOptions(allocator, writer, spec.flags, true);
    if (spec.extra_help) |extra| try writer.print("\n{s}", .{extra});
}

pub fn printArguments(writer: anytype, arguments: []const ArgumentSpec) !void {
    if (arguments.len == 0) return;
    try writer.writeAll("\nArguments:\n");
    var max_label_len: usize = 0;
    for (arguments) |argument| max_label_len = @max(max_label_len, argumentLabelLen(argument));
    for (arguments) |argument| {
        try writeArgumentLabel(writer, argument);
        const description_col = max_label_len + 4;
        try writeSpaces(writer, max_label_len - argumentLabelLen(argument) + 2);
        _ = try printWrapped(writer, argument.description, description_col, description_col);
        try writer.writeByte('\n');
    }
}

pub fn printOptions(allocator: Allocator, writer: anytype, flags: []const FlagSpec, include_help: bool) !void {
    if (flags.len == 0 and !include_help) return;
    try writer.writeAll("\nOptions:\n");

    var max_label_len: usize = 0;
    for (flags) |flag| max_label_len = @max(max_label_len, flagLabelLen(flag));
    if (include_help) max_label_len = @max(max_label_len, "  -h, --help".len);

    for (flags) |flag| {
        const label = try flagLabel(allocator, flag);
        defer allocator.free(label);
        try printOption(allocator, writer, label, flag.description, flag.default_value, flag.repeatable, max_label_len);
    }
    if (include_help) try printOption(allocator, writer, "  -h, --help", "Print help", null, false, max_label_len);
}

pub fn printCommandList(writer: anytype, commands: []const CommandEntry) !void {
    if (commands.len == 0) return;
    try writer.writeAll("\nCommands:\n");
    var max_name_len: usize = 0;
    for (commands) |command| max_name_len = @max(max_name_len, command.name.len);
    for (commands) |command| {
        try writer.print("  {s}", .{command.name});
        const description_col = max_name_len + 4;
        try writeSpaces(writer, max_name_len - command.name.len + 2);
        _ = try printWrapped(writer, command.description, description_col, description_col);
        try writer.writeByte('\n');
    }
}

fn printOption(allocator: Allocator, writer: anytype, label: []const u8, description: []const u8, default_value: ?[]const u8, repeatable: bool, max_label_len: usize) !void {
    try writer.print("{s}", .{label});
    const description_col = max_label_len + 2;
    try writeSpaces(writer, max_label_len - label.len + 2);
    var line_len = try printWrapped(writer, description, description_col, description_col);
    if (default_value) |value| {
        const suffix = try std.fmt.allocPrint(allocator, "[default: {s}]", .{value});
        defer allocator.free(suffix);
        line_len = try printWrapped(writer, suffix, description_col, line_len);
    }
    if (repeatable) _ = try printWrapped(writer, "[repeatable]", description_col, line_len);
    try writer.writeByte('\n');
}

fn printWrapped(writer: anytype, text: []const u8, indent: usize, initial_line_len: usize) !usize {
    var line_len = initial_line_len;
    var pos: usize = 0;
    while (pos < text.len) {
        while (pos < text.len and text[pos] == ' ') pos += 1;
        if (pos >= text.len) break;
        const start = pos;
        while (pos < text.len and text[pos] != ' ') pos += 1;
        const word = text[start..pos];
        var remaining = word;
        while (remaining.len > 0) {
            const sep: usize = if (line_len == indent) 0 else 1;
            const available = if (help_line_width > line_len + sep) help_line_width - line_len - sep else 0;
            if (remaining.len <= available) {
                if (sep == 1) {
                    try writer.writeByte(' ');
                    line_len += 1;
                }
                try writer.writeAll(remaining);
                line_len += remaining.len;
                break;
            }
            if (line_len > indent) {
                try writer.writeByte('\n');
                try writeSpaces(writer, indent);
                line_len = indent;
                continue;
            }
            const chunk_len = @min(remaining.len, if (help_line_width > indent) help_line_width - indent else 1);
            try writer.writeAll(remaining[0..chunk_len]);
            line_len += chunk_len;
            remaining = remaining[chunk_len..];
            if (remaining.len > 0) {
                try writer.writeByte('\n');
                try writeSpaces(writer, indent);
                line_len = indent;
            }
        }
    }
    return line_len;
}

fn flagLabelLen(spec: FlagSpec) usize {
    const vname = valueName(spec);
    const short_prefix: usize = if (spec.short != null) 6 else 6; // "  -x, " or "      "
    const name_len = 2 + spec.name.len; // "--name"
    const value_suffix: usize = switch (spec.value) {
        .none => 0,
        .string, .int, .bool_required => 3 + vname.len, // " <VALUE>"
        .bool_optional => 3 + vname.len, // "[=VALUE]"
    };
    return short_prefix + name_len + value_suffix;
}

fn flagLabel(allocator: Allocator, spec: FlagSpec) ![]const u8 {
    if (spec.short) |short| {
        return switch (spec.value) {
            .none => try std.fmt.allocPrint(allocator, "  -{c}, --{s}", .{ short, spec.name }),
            .string, .int, .bool_required => try std.fmt.allocPrint(allocator, "  -{c}, --{s} <{s}>", .{ short, spec.name, valueName(spec) }),
            .bool_optional => try std.fmt.allocPrint(allocator, "  -{c}, --{s}[={s}]", .{ short, spec.name, valueName(spec) }),
        };
    }
    return switch (spec.value) {
        .none => try std.fmt.allocPrint(allocator, "      --{s}", .{spec.name}),
        .string, .int, .bool_required => try std.fmt.allocPrint(allocator, "      --{s} <{s}>", .{ spec.name, valueName(spec) }),
        .bool_optional => try std.fmt.allocPrint(allocator, "      --{s}[={s}]", .{ spec.name, valueName(spec) }),
    };
}

fn valueName(spec: FlagSpec) []const u8 {
    if (spec.value_name) |name| return name;
    return switch (spec.value) {
        .none => "",
        .string => "VALUE",
        .int => "N",
        .bool_required, .bool_optional => "BOOL",
    };
}

fn argumentLabelLen(argument: ArgumentSpec) usize {
    const brackets: usize = 2;
    const repeat: usize = if (argument.repeatable) 3 else 0;
    return argument.name.len + brackets + repeat;
}

fn writeArgumentLabel(writer: anytype, argument: ArgumentSpec) !void {
    try writer.writeAll("  ");
    if (argument.required) {
        try writer.print("<{s}>", .{argument.name});
    } else {
        try writer.print("[{s}]", .{argument.name});
    }
    if (argument.repeatable) try writer.writeAll("...");
}

fn writeSpaces(writer: anytype, count: usize) !void {
    const spaces = "                                                                                                                        ";
    var remaining = count;
    while (remaining > 0) {
        const chunk = @min(remaining, spaces.len);
        try writer.writeAll(spaces[0..chunk]);
        remaining -= chunk;
    }
}
