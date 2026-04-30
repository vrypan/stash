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
    extra: ?[]const u8 = null,
};

pub const CommandEntry = struct {
    name: []const u8,
    description: []const u8,
};

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

pub fn parse(allocator: Allocator, args: []const [:0]const u8, specs: []const FlagSpec) !Parsed {
    var parsed = Parsed{};
    var i: usize = 0;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (std.mem.eql(u8, arg, "--")) {
            i += 1;
            while (i < args.len) : (i += 1) try parsed.positionals.append(allocator, args[i]);
            break;
        }
        if (std.mem.startsWith(u8, arg, "--")) {
            try parseLong(allocator, args, &i, specs, &parsed);
        } else if (std.mem.startsWith(u8, arg, "-") and arg.len > 1) {
            try parseShort(allocator, args, &i, specs, &parsed);
        } else {
            try parsed.positionals.append(allocator, arg);
        }
    }
    return parsed;
}

fn parseLong(allocator: Allocator, args: []const [:0]const u8, index: *usize, specs: []const FlagSpec, parsed: *Parsed) !void {
    const arg = args[index.*];
    const raw = arg[2..];
    const eql = std.mem.indexOfScalar(u8, raw, '=');
    const name = if (eql) |pos| raw[0..pos] else raw;
    const inline_value = if (eql) |pos| raw[pos + 1 ..] else null;
    const spec = findLong(specs, name) orelse return error.InvalidArgument;
    const value = try consumeValue(allocator, args, index, spec, inline_value);
    try appendFlag(allocator, parsed, spec, value);
}

fn parseShort(allocator: Allocator, args: []const [:0]const u8, index: *usize, specs: []const FlagSpec, parsed: *Parsed) !void {
    const arg = args[index.*];
    const body = arg[1..];
    if (body.len == 0) return error.InvalidArgument;
    const spec = findShort(specs, body[0]) orelse return error.InvalidArgument;
    const inline_value: ?[]const u8 = if (body.len > 1) blk: {
        if (spec.value == .none or !spec.attached_short_value) return error.InvalidArgument;
        break :blk body[1..];
    } else null;
    const value = try consumeValue(allocator, args, index, spec, inline_value);
    try appendFlag(allocator, parsed, spec, value);
}

fn consumeValue(allocator: Allocator, args: []const [:0]const u8, index: *usize, spec: FlagSpec, inline_value: ?[]const u8) !?[]const u8 {
    _ = allocator;
    switch (spec.value) {
        .none => {
            if (inline_value != null) return error.InvalidArgument;
            return null;
        },
        .string, .int, .bool_required => {
            const raw = inline_value orelse blk: {
                index.* += 1;
                if (index.* >= args.len) return error.InvalidArgument;
                break :blk args[index.*];
            };
            if (spec.value == .int) _ = std.fmt.parseInt(usize, raw, 10) catch return error.InvalidArgument;
            if (spec.value == .bool_required) _ = parseBool(raw) catch return error.InvalidArgument;
            return raw;
        },
        .bool_optional => {
            const raw = inline_value orelse blk: {
                if (index.* + 1 < args.len and !std.mem.startsWith(u8, args[index.* + 1], "-")) {
                    index.* += 1;
                    break :blk args[index.*];
                }
                return "true";
            };
            _ = parseBool(raw) catch return error.InvalidArgument;
            return raw;
        },
    }
}

fn appendFlag(allocator: Allocator, parsed: *Parsed, spec: FlagSpec, value: ?[]const u8) !void {
    if (!spec.repeatable) {
        var i: usize = 0;
        while (i < parsed.flags.items.len) : (i += 1) {
            if (std.mem.eql(u8, parsed.flags.items[i].name, spec.name)) {
                parsed.flags.items[i].value = value;
                return;
            }
        }
    }
    try parsed.flags.append(allocator, .{ .name = spec.name, .value = value });
}

fn findLong(specs: []const FlagSpec, name: []const u8) ?FlagSpec {
    for (specs) |spec| if (std.mem.eql(u8, spec.name, name)) return spec;
    return null;
}

fn findShort(specs: []const FlagSpec, short: u8) ?FlagSpec {
    for (specs) |spec| if (spec.short != null and spec.short.? == short) return spec;
    return null;
}

pub fn parseBool(value: []const u8) !bool {
    if (std.ascii.eqlIgnoreCase(value, "true") or std.mem.eql(u8, value, "1") or std.ascii.eqlIgnoreCase(value, "yes")) return true;
    if (std.ascii.eqlIgnoreCase(value, "false") or std.mem.eql(u8, value, "0") or std.ascii.eqlIgnoreCase(value, "no")) return false;
    return error.InvalidArgument;
}

pub fn printCommandHelp(allocator: Allocator, writer: anytype, spec: CommandSpec) !void {
    try writer.print("{s}\n\nUsage: {s}\n", .{ spec.description, spec.usage });
    try printOptions(allocator, writer, spec.flags, true);
    if (spec.extra) |extra| try writer.print("\n{s}", .{extra});
}

pub fn printOptions(allocator: Allocator, writer: anytype, flags: []const FlagSpec, include_help: bool) !void {
    if (flags.len == 0 and !include_help) return;
    try writer.writeAll("\nOptions:\n");

    var max_label_len: usize = 0;
    for (flags) |flag| {
        const label = try flagLabel(allocator, flag);
        defer allocator.free(label);
        max_label_len = @max(max_label_len, label.len);
    }
    if (include_help) max_label_len = @max(max_label_len, "  -h, --help".len);

    for (flags) |flag| {
        const label = try flagLabel(allocator, flag);
        defer allocator.free(label);
        try printOption(writer, label, flag.description, flag.default_value, max_label_len);
    }
    if (include_help) try printOption(writer, "  -h, --help", "Print help", null, max_label_len);
}

pub fn printCommandList(writer: anytype, commands: []const CommandEntry) !void {
    if (commands.len == 0) return;
    try writer.writeAll("\nCommands:\n");
    var max_name_len: usize = 0;
    for (commands) |command| max_name_len = @max(max_name_len, command.name.len);
    for (commands) |command| {
        try writer.print("  {s}", .{command.name});
        try writeSpaces(writer, max_name_len - command.name.len + 2);
        try writer.print("{s}\n", .{command.description});
    }
}

fn printOption(writer: anytype, label: []const u8, description: []const u8, default_value: ?[]const u8, max_label_len: usize) !void {
    try writer.print("{s}", .{label});
    try writeSpaces(writer, max_label_len - label.len + 2);
    try writer.print("{s}", .{description});
    if (default_value) |value| try writer.print(" [default: {s}]", .{value});
    try writer.writeByte('\n');
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

fn writeSpaces(writer: anytype, count: usize) !void {
    var i: usize = 0;
    while (i < count) : (i += 1) try writer.writeByte(' ');
}
