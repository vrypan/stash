const std = @import("std");
const build_options = @import("build_options");
const cli = @import("cli");
const display = @import("display.zig");
const runtime = @import("runtime.zig");
const store = @import("store.zig");
const types = @import("types.zig");

const Allocator = std.mem.Allocator;
const Attr = types.Attr;
const Meta = types.Meta;
const PrintTarget = types.PrintTarget;
const IdMode = types.IdMode;
const DateMode = types.DateMode;
const AttrsMode = types.AttrsMode;
const AttrFilter = types.AttrFilter;
const MetaSelection = types.MetaSelection;
const version = build_options.version;

const PushMode = enum { push, tee, auto };

const CommandName = enum { root, push, tee, cat, ls, attr, attrs, path, rm };

const root_flags = [_]cli.FlagSpec{
    .{ .name = "attr", .short = 'a', .value = .string, .value_name = "key=value", .description = "Set attribute key=value", .repeatable = true, .attached_short_value = true },
    .{ .name = "pocket", .value = .string, .value_name = "VALUE", .description = "Alias for --attr pocket=VALUE", .repeatable = true },
    .{ .name = "print", .value = .string, .value_name = "TARGET", .description = "Where to print the generated entry ID: stdout, stderr, null, 1, 2, or 0", .default_value = "null" },
    .{ .name = "save-on-error", .value = .bool_optional, .value_name = "BOOL", .description = "Save captured input if the input stream is interrupted", .default_value = "true" },
    .{ .name = "version", .short = 'V', .description = "Print version" },
};

const push_flags = [_]cli.FlagSpec{
    .{ .name = "attr", .short = 'a', .value = .string, .value_name = "key=value", .description = "Set attribute key=value", .repeatable = true, .attached_short_value = true },
    .{ .name = "pocket", .value = .string, .value_name = "VALUE", .description = "Alias for --attr pocket=VALUE", .repeatable = true },
    .{ .name = "print", .value = .string, .value_name = "TARGET", .description = "Where to print the generated entry ID: stdout, stderr, null, 1, 2, or 0", .default_value = "null" },
    .{ .name = "save-on-error", .value = .bool_optional, .value_name = "BOOL", .description = "Save captured input if the input stream is interrupted", .default_value = "true" },
};

const tee_flags = [_]cli.FlagSpec{
    .{ .name = "attr", .short = 'a', .value = .string, .value_name = "key=value", .description = "Set attribute key=value", .repeatable = true, .attached_short_value = true },
    .{ .name = "pocket", .value = .string, .value_name = "VALUE", .description = "Alias for --attr pocket=VALUE", .repeatable = true },
    .{ .name = "print", .value = .string, .value_name = "TARGET", .description = "Where to print the generated entry ID: stdout, stderr, null, 1, 2, or 0", .default_value = "null" },
    .{ .name = "save-on-error", .value = .bool_optional, .value_name = "BOOL", .description = "Save captured input if the input stream is interrupted", .default_value = "true" },
};

const ref_filter_flags = [_]cli.FlagSpec{
    .{ .name = "attr", .short = 'a', .value = .string, .value_name = "FILTER", .description = "Attribute filter: name or name=value", .repeatable = true, .attached_short_value = true },
    .{ .name = "pocket", .value = .string, .value_name = "VALUE", .description = "Alias for --attr pocket=VALUE", .repeatable = true },
};

const ref_filter_reverse_flags = [_]cli.FlagSpec{
    .{ .name = "attr", .short = 'a', .value = .string, .value_name = "FILTER", .description = "Attribute filter: name or name=value", .repeatable = true, .attached_short_value = true },
    .{ .name = "pocket", .value = .string, .value_name = "VALUE", .description = "Alias for --attr pocket=VALUE", .repeatable = true },
    .{ .name = "reverse", .short = 'r', .description = "Print matching refs oldest first" },
};

const ls_flags = [_]cli.FlagSpec{
    .{ .name = "after", .value = .string, .value_name = "REF", .description = "Show entries newer than the referenced entry" },
    .{ .name = "attr", .short = 'a', .value = .string, .value_name = "ATTR", .description = "Filter or display attributes", .repeatable = true, .attached_short_value = true },
    .{ .name = "attrs", .value = .string, .value_name = "MODE", .description = "Attribute display: none, list, count, or flag" },
    .{ .name = "before", .value = .string, .value_name = "REF", .description = "Show entries older than the referenced entry" },
    .{ .name = "bytes", .description = "Include raw byte-count column" },
    .{ .name = "color", .value = .bool_required, .value_name = "BOOL", .description = "Color output", .default_value = "true" },
    .{ .name = "date", .description = "Include date column using ls-style dates" },
    .{ .name = "format", .value = .string, .value_name = "FMT", .description = "Print entries using a format string" },
    .{ .name = "headers", .description = "Print a header row for tabular output" },
    .{ .name = "id", .value = .string, .value_name = "MODE", .description = "ID display: short, full, or pos" },
    .{ .name = "json", .description = "Output listing as rich JSON" },
    .{ .name = "long", .short = 'l', .description = "Alias for --date --size --attrs=flag --preview" },
    .{ .name = "name", .description = "Include filename attribute if available" },
    .{ .name = "number", .short = 'n', .value = .int, .value_name = "N", .description = "Limit number of entries shown", .attached_short_value = true },
    .{ .name = "pocket", .value = .string, .value_name = "VALUE", .description = "Alias for --attr pocket=VALUE", .repeatable = true },
    .{ .name = "preview", .short = 'p', .description = "Append compact preview text" },
    .{ .name = "reverse", .short = 'r', .description = "Show oldest first" },
    .{ .name = "size", .description = "Include human-readable size column" },
    .{ .name = "width", .short = 'w', .value = .int, .value_name = "N", .description = "Maximum output line width; 0 uses terminal width", .default_value = "0", .attached_short_value = true },
};

const attr_flags = [_]cli.FlagSpec{
    .{ .name = "json", .description = "Print attributes as JSON" },
    .{ .name = "preview", .short = 'p', .description = "Include the preview pseudo-attribute" },
    .{ .name = "separator", .value = .string, .value_name = "SEP", .description = "Separator for text output", .default_value = "\\t" },
    .{ .name = "unset", .value = .string, .value_name = "KEY", .description = "Remove a writable attribute", .repeatable = true },
};

const attrs_flags = [_]cli.FlagSpec{
    .{ .name = "count", .description = "Print counts" },
};

const path_flags = [_]cli.FlagSpec{
    .{ .name = "attr", .short = 'a', .description = "Print attr path instead of data path" },
    .{ .name = "dir", .short = 'd', .description = "Print stash root directory" },
};

const rm_flags = [_]cli.FlagSpec{
    .{ .name = "after", .value = .string, .value_name = "REF", .description = "Remove entries newer than the referenced entry" },
    .{ .name = "attr", .short = 'a', .value = .string, .value_name = "FILTER", .description = "Attribute filter: name or name=value", .repeatable = true, .attached_short_value = true },
    .{ .name = "before", .value = .string, .value_name = "REF", .description = "Remove entries older than the referenced entry" },
    .{ .name = "force", .short = 'f', .description = "Skip confirmation prompts" },
};

pub const commands = [_]cli.CommandEntry{
    .{ .name = "attr", .description = "Show or update entry attributes" },
    .{ .name = "attrs", .description = "List attribute keys across the stash" },
    .{ .name = "cat", .description = "Print an entry's raw data to stdout" },
    .{ .name = "ls", .description = "List entries" },
    .{ .name = "path", .description = "Print stash paths" },
    .{ .name = "push", .description = "Store stdin and return the entry key" },
    .{ .name = "rm", .description = "Remove entries" },
    .{ .name = "tee", .description = "Store stdin and forward it to stdout" },
};

const file_argument = [_]cli.ArgumentSpec{
    .{ .name = "FILE", .description = "Optional file to stash; reads stdin when omitted" },
};

const ref_arguments = [_]cli.ArgumentSpec{
    .{ .name = "REF", .description = "Entry ID, stack ref, or stack number", .repeatable = true },
};

const attr_arguments = [_]cli.ArgumentSpec{
    .{ .name = "REF", .description = "Entry ID, stack ref, or stack number", .required = true },
    .{ .name = "ARG", .description = "Attribute names or key=value assignments", .repeatable = true },
};

const attrs_arguments = [_]cli.ArgumentSpec{
    .{ .name = "KEY", .description = "Attribute key to list distinct values for" },
};

const path_arguments = [_]cli.ArgumentSpec{
    .{ .name = "REF", .description = "Entry ID, stack ref, or stack number" },
};

pub const root_spec = cli.CommandSpec{ .name = "stash", .description = "A local store for piped output and files.", .usage = "stash [options] [FILE]", .flags = &root_flags, .arguments = &file_argument };
pub const push_spec = cli.CommandSpec{ .name = "push", .description = "Store stdin and return the entry key", .usage = "stash push [options] [FILE]", .flags = &push_flags, .arguments = &file_argument };
pub const tee_spec = cli.CommandSpec{ .name = "tee", .description = "Store stdin and forward it to stdout", .usage = "stash tee [options]", .flags = &tee_flags };
pub const cat_spec = cli.CommandSpec{ .name = "cat", .description = "Print an entry's raw data to stdout", .usage = "stash cat [options] [REF]...", .flags = &ref_filter_reverse_flags, .arguments = &ref_arguments };
pub const ls_spec = cli.CommandSpec{ .name = "ls", .description = "List entries", .usage = "stash ls [options]", .flags = &ls_flags, .extra_help =
    \\Format tokens:
    \\  %i       short ID
    \\  %I       full ID
    \\  %n       stack position
    \\  %dt      raw timestamp
    \\  %dh      ls-style date
    \\  %di      ISO date
    \\  %sh      human-readable size
    \\  %sb      raw byte count
    \\  %p       preview
    \\  %a{key}  attribute value
    \\  %Af      attribute flag
    \\  %Al      attribute list
    \\  %Ac      attribute count
    \\  %%       literal %
    \\  \n \r \t \\ escapes
    \\
};
pub const attr_spec = cli.CommandSpec{ .name = "attr", .description = "Show or update entry attributes", .usage = "stash attr [options] REF [KEY|key=value]...", .flags = &attr_flags, .arguments = &attr_arguments };
pub const attrs_spec = cli.CommandSpec{ .name = "attrs", .description = "List attribute keys across the stash", .usage = "stash attrs [options] [KEY]", .flags = &attrs_flags, .arguments = &attrs_arguments };
pub const path_spec = cli.CommandSpec{ .name = "path", .description = "Print stash paths", .usage = "stash path [options] [REF]", .flags = &path_flags, .arguments = &path_arguments };
pub const rm_spec = cli.CommandSpec{ .name = "rm", .description = "Remove entries", .usage = "stash rm [options] [REF]...", .flags = &rm_flags, .arguments = &ref_arguments };

const LsCliOptions = struct {
    id: IdMode = .short,
    attr: [][]const u8 = &.{},
    pocket: [][]const u8 = &.{},
    attrs: AttrsMode = .none,
    number: usize = 0,
    before: ?[]const u8 = null,
    after: ?[]const u8 = null,
    reverse: bool = false,
    json: bool = false,
    headers: bool = false,
    date: bool = false,
    size: bool = false,
    bytes: bool = false,
    name: bool = false,
    preview: bool = false,
    format: ?[]const u8 = null,
    long: bool = false,
    width: usize = 0,
    color: bool = true,
};

pub fn errorMessage(err: anyerror) []const u8 {
    return switch (err) {
        error.InvalidArgument => "invalid argument",
        error.NotFound => "entry not found",
        error.StashEmpty => "stash is empty",
        error.IdTooShort => "id too short",
        error.AmbiguousId => "ambiguous id",
        error.InvalidAttr => "invalid attribute",
        error.InvalidRef => "invalid stack ref",
        error.ReadOnlyAttr => "only user-defined attributes are writable",
        error.InputInterrupted => "input interrupted",
        error.InputInterruptedSaved => "input interrupted; saved partial entry",
        else => @errorName(err),
    };
}

pub fn run(init: *const std.process.Init, allocator: Allocator, args: []const [:0]const u8) !u8 {
    _ = init;
    if (args.len <= 1) {
        return cmdPush(allocator, &.{}, .auto);
    }
    const first = args[1];
    if (std.mem.eql(u8, first, "--help") or std.mem.eql(u8, first, "-h")) {
        try printHelp(allocator, .root);
        return 0;
    }
    if (std.mem.eql(u8, first, "--version") or std.mem.eql(u8, first, "-V")) {
        try runtime.stdoutWriter().print("stash {s}\n", .{version});
        return 0;
    }
    if (args.len > 2 and (std.mem.eql(u8, args[2], "--help") or std.mem.eql(u8, args[2], "-h"))) {
        if (commandName(first)) |name| {
            try printHelp(allocator, name);
            return 0;
        }
        return error.InvalidArgument;
    }
    if (std.mem.startsWith(u8, first, "-")) {
        return cmdPush(allocator, args[1..], .auto);
    }
    if (std.mem.eql(u8, first, "push")) return cmdPush(allocator, args[2..], .push);
    if (std.mem.eql(u8, first, "tee")) return cmdTee(allocator, args[2..]);
    if (std.mem.eql(u8, first, "cat")) return cmdCat(allocator, args[2..]);
    if (std.mem.eql(u8, first, "path")) return cmdPath(allocator, args[2..]);
    if (std.mem.eql(u8, first, "attr")) return cmdAttr(allocator, args[2..]);
    if (std.mem.eql(u8, first, "attrs")) return cmdAttrs(allocator, args[2..]);
    if (std.mem.eql(u8, first, "ls")) return cmdLs(allocator, args[2..]);
    if (std.mem.eql(u8, first, "rm")) return cmdRm(allocator, args[2..]);

    return cmdPush(allocator, args[1..], .auto);
}

fn commandName(value: []const u8) ?CommandName {
    if (std.mem.eql(u8, value, "push")) return .push;
    if (std.mem.eql(u8, value, "tee")) return .tee;
    if (std.mem.eql(u8, value, "cat")) return .cat;
    if (std.mem.eql(u8, value, "ls")) return .ls;
    if (std.mem.eql(u8, value, "attr")) return .attr;
    if (std.mem.eql(u8, value, "attrs")) return .attrs;
    if (std.mem.eql(u8, value, "path")) return .path;
    if (std.mem.eql(u8, value, "rm")) return .rm;
    return null;
}

fn printHelp(allocator: Allocator, name: CommandName) !void {
    const out = runtime.stdoutWriter();
    switch (name) {
        .root => {
            try out.print(
                \\{s}
                \\v{s}
                \\
                \\More info: https://github.com/vrypan/stash
                \\
                \\Usage: {s}
                \\
            , .{ root_spec.description, version, root_spec.usage });
            try cli.printCommandList(out, &commands);
            try cli.printArguments(out, root_spec.arguments);
            try cli.printOptions(allocator, out, root_spec.flags, true);
        },
        .push => try cli.printCommandHelp(allocator, out, push_spec),
        .tee => try cli.printCommandHelp(allocator, out, tee_spec),
        .cat => try cli.printCommandHelp(allocator, out, cat_spec),
        .ls => try cli.printCommandHelp(allocator, out, ls_spec),
        .attr => try cli.printCommandHelp(allocator, out, attr_spec),
        .attrs => try cli.printCommandHelp(allocator, out, attrs_spec),
        .path => try cli.printCommandHelp(allocator, out, path_spec),
        .rm => try cli.printCommandHelp(allocator, out, rm_spec),
    }
}

fn cmdLs(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var opts = LsCliOptions{};
    const parsed = try cli.parseCommand(allocator, runtime.stderrWriter(), raw_args, ls_spec);
    for (parsed.flags.items) |flag| {
        if (std.mem.eql(u8, flag.name, "after")) {
            opts.after = try allocator.dupe(u8, flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "attr")) {
            try appendConstSlice(allocator, &opts.attr, flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "attrs")) {
            opts.attrs = try parseAttrsMode(flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "before")) {
            opts.before = try allocator.dupe(u8, flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "bytes")) {
            opts.bytes = true;
        } else if (std.mem.eql(u8, flag.name, "color")) {
            opts.color = try parseBool(flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "date")) {
            opts.date = true;
        } else if (std.mem.eql(u8, flag.name, "format")) {
            opts.format = try allocator.dupe(u8, flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "headers")) {
            opts.headers = true;
        } else if (std.mem.eql(u8, flag.name, "id")) {
            opts.id = try parseIdMode(flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "json")) {
            opts.json = true;
        } else if (std.mem.eql(u8, flag.name, "long")) {
            opts.long = true;
        } else if (std.mem.eql(u8, flag.name, "name")) {
            opts.name = true;
        } else if (std.mem.eql(u8, flag.name, "number")) {
            opts.number = try parseUsize(flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "pocket")) {
            try appendConstSlice(allocator, &opts.pocket, flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "preview")) {
            opts.preview = true;
        } else if (std.mem.eql(u8, flag.name, "reverse")) {
            opts.reverse = true;
        } else if (std.mem.eql(u8, flag.name, "size")) {
            opts.size = true;
        } else if (std.mem.eql(u8, flag.name, "width")) {
            opts.width = try parseUsize(flag.value.?);
        }
    }
    try cmdLsFromOptions(allocator, &opts);
    return 0;
}

fn appendConstSlice(allocator: Allocator, list: *[][]const u8, value: []const u8) !void {
    const old = list.*;
    const next = try allocator.alloc([]const u8, old.len + 1);
    std.mem.copyForwards([]const u8, next[0..old.len], old);
    next[old.len] = value;
    list.* = next;
}

fn parseIdMode(value: []const u8) !IdMode {
    if (std.mem.eql(u8, value, "short")) return .short;
    if (std.mem.eql(u8, value, "full")) return .full;
    if (std.mem.eql(u8, value, "pos")) return .pos;
    return error.InvalidArgument;
}

fn parseAttrsMode(value: []const u8) !AttrsMode {
    if (std.mem.eql(u8, value, "none")) return .none;
    if (std.mem.eql(u8, value, "list")) return .list;
    if (std.mem.eql(u8, value, "count")) return .count;
    if (std.mem.eql(u8, value, "flag")) return .flag;
    return error.InvalidArgument;
}

fn parseUsize(value: []const u8) !usize {
    return std.fmt.parseInt(usize, value, 10) catch return error.InvalidArgument;
}

fn parseBool(value: []const u8) !bool {
    if (std.ascii.eqlIgnoreCase(value, "true") or std.mem.eql(u8, value, "1") or std.ascii.eqlIgnoreCase(value, "yes")) return true;
    if (std.ascii.eqlIgnoreCase(value, "false") or std.mem.eql(u8, value, "0") or std.ascii.eqlIgnoreCase(value, "no")) return false;
    return error.InvalidArgument;
}

fn cmdPush(allocator: Allocator, raw_args: []const [:0]const u8, mode: PushMode) !u8 {
    var attrs: std.ArrayList(Attr) = .empty;
    var print_target: PrintTarget = .none;
    var file_arg: ?[]const u8 = null;
    var save_on_error = true;

    const spec = if (mode == .tee) tee_spec else push_spec;
    const parsed = try cli.parseCommand(allocator, runtime.stderrWriter(), raw_args, spec);
    if (mode != .tee and parsed.positionals.items.len == 1) file_arg = parsed.positionals.items[0];

    for (parsed.flags.items) |flag| {
        if (std.mem.eql(u8, flag.name, "attr")) {
            try appendAttrFlag(allocator, &attrs, flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "pocket")) {
            try setAttrList(allocator, &attrs, types.pocket_attr, flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "print")) {
            print_target = try parsePrintTarget(flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "save-on-error")) {
            save_on_error = try parseBool(flag.value.?);
        }
    }

    if (!hasAttr(attrs.items, types.pocket_attr)) {
        if (store.activePocket(allocator)) |pocket| try setAttrList(allocator, &attrs, types.pocket_attr, pocket);
    }
    if (file_arg) |path| {
        const basename = std.fs.path.basename(path);
        if (basename.len > 0) try setAttrList(allocator, &attrs, "filename", basename);
    }

    const tee_mode = mode == .tee or (mode == .auto and file_arg == null and rootShouldTee());
    const result = try store.pushInput(allocator, file_arg, attrs.items, tee_mode, save_on_error);
    try emitId(print_target, result.id);
    if (result.interrupted) return error.InputInterruptedSaved;
    return 0;
}

fn cmdTee(allocator: Allocator, args: []const [:0]const u8) !u8 {
    return cmdPush(allocator, args, .tee);
}

fn rootShouldTee() bool {
    return !runtime.stdinIsTty() and !runtime.stdoutIsTty();
}

fn appendAttrFlag(allocator: Allocator, attrs: *std.ArrayList(Attr), pair: []const u8) !void {
    const pos = std.mem.indexOfScalar(u8, pair, '=') orelse return error.InvalidAttr;
    try setAttrList(allocator, attrs, pair[0..pos], pair[pos + 1 ..]);
}

fn setAttrList(allocator: Allocator, attrs: *std.ArrayList(Attr), key: []const u8, value: []const u8) !void {
    for (attrs.items) |*item| {
        if (std.mem.eql(u8, item.key, key)) {
            item.value = try allocator.dupe(u8, value);
            return;
        }
    }
    try attrs.append(allocator, .{ .key = try allocator.dupe(u8, key), .value = try allocator.dupe(u8, value) });
    types.sortAttrs(attrs.items);
}

fn hasAttr(attrs: []const Attr, key: []const u8) bool {
    for (attrs) |item| if (std.mem.eql(u8, item.key, key)) return true;
    return false;
}

fn parsePrintTarget(value: []const u8) !PrintTarget {
    if (std.mem.eql(u8, value, "stdout") or std.mem.eql(u8, value, "1")) return .stdout;
    if (std.mem.eql(u8, value, "stderr") or std.mem.eql(u8, value, "2")) return .stderr;
    if (std.mem.eql(u8, value, "null") or std.mem.eql(u8, value, "0")) return .none;
    return error.InvalidArgument;
}

fn emitId(target: PrintTarget, id: []const u8) !void {
    switch (target) {
        .stdout => try runtime.stdoutWriter().print("{s}\n", .{id}),
        .stderr => try runtime.stderrWriter().print("{s}\n", .{id}),
        .none => {},
    }
}

fn cmdCat(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var refs: std.ArrayList([]const u8) = .empty;
    var filters: std.ArrayList(AttrFilter) = .empty;
    var reverse = false;
    try parseRefsAndFilters(allocator, raw_args, &refs, &filters, &reverse, true);

    const stdout = runtime.stdoutWriter();
    if (filters.items.len > 0) {
        if (refs.items.len > 0) return error.InvalidArgument;
        const items = try store.visibleList(allocator);
        var i: usize = 0;
        while (i < items.items.len) : (i += 1) {
            const idx = if (reverse) i else items.items.len - 1 - i;
            if (matchesFilters(&items.items[idx], filters.items)) try store.catId(allocator, items.items[idx].id, stdout);
        }
    } else if (refs.items.len == 0) {
        const id = try store.resolve(allocator, "");
        try store.catId(allocator, id, stdout);
    } else {
        var i: usize = 0;
        while (i < refs.items.len) : (i += 1) {
            const idx = if (reverse) refs.items.len - 1 - i else i;
            const id = try store.resolve(allocator, refs.items[idx]);
            try store.catId(allocator, id, stdout);
        }
    }
    return 0;
}

fn parseRefsAndFilters(
    allocator: Allocator,
    raw_args: []const [:0]const u8,
    refs: *std.ArrayList([]const u8),
    filters: *std.ArrayList(AttrFilter),
    reverse: *bool,
    allow_reverse: bool,
) !void {
    const spec = if (allow_reverse) cat_spec else cli.CommandSpec{ .name = "cat", .description = cat_spec.description, .usage = cat_spec.usage, .flags = &ref_filter_flags, .arguments = &ref_arguments };
    const parsed = try cli.parseCommand(allocator, runtime.stderrWriter(), raw_args, spec);
    for (parsed.flags.items) |flag| {
        if (std.mem.eql(u8, flag.name, "attr")) {
            try appendFilter(allocator, filters, flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "pocket")) {
            try filters.append(allocator, .{ .key = types.pocket_attr, .value = flag.value.? });
        } else if (std.mem.eql(u8, flag.name, "reverse")) {
            reverse.* = true;
        }
    }
    for (parsed.positionals.items) |ref| try refs.append(allocator, ref);
}

fn appendFilter(allocator: Allocator, filters: *std.ArrayList(AttrFilter), value: []const u8) !void {
    if (value.len == 0 or std.mem.indexOfScalar(u8, value, ',') != null) return error.InvalidArgument;
    if (std.mem.indexOfScalar(u8, value, '=')) |pos| {
        try filters.append(allocator, .{ .key = value[0..pos], .value = value[pos + 1 ..] });
    } else {
        try filters.append(allocator, .{ .key = value, .value = null });
    }
}

fn matchesFilters(meta: *const Meta, filters: []const AttrFilter) bool {
    for (filters) |filter| {
        const value = meta.attr(filter.key) orelse return false;
        if (filter.value) |wanted| {
            if (!std.mem.eql(u8, value, wanted)) return false;
        }
    }
    return true;
}

fn cmdPath(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var want_attr = false;
    var want_dir = false;
    var ref: ?[]const u8 = null;
    const parsed = try cli.parseCommand(allocator, runtime.stderrWriter(), raw_args, path_spec);
    if (parsed.positionals.items.len == 1) ref = parsed.positionals.items[0];
    for (parsed.flags.items) |flag| {
        if (std.mem.eql(u8, flag.name, "attr")) want_attr = true else if (std.mem.eql(u8, flag.name, "dir")) want_dir = true;
    }
    const p = try store.basePaths(allocator);
    const out = runtime.stdoutWriter();
    if (ref) |r| {
        const id = try store.resolve(allocator, r);
        if (want_dir) {
            try out.print("{s}\n", .{p.base});
        } else if (want_attr) {
            try out.print("{s}\n", .{try store.attrPath(allocator, id)});
        } else {
            try out.print("{s}\n", .{try store.dataPath(allocator, id)});
        }
    } else {
        if (want_dir) try out.print("{s}\n", .{p.base}) else if (want_attr) try out.print("{s}\n", .{p.attr}) else try out.print("{s}\n", .{p.data});
    }
    return 0;
}

fn cmdAttr(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var json = false;
    var preview = false;
    var separator: []const u8 = "\t";
    var unset: std.ArrayList([]const u8) = .empty;
    var items: std.ArrayList([]const u8) = .empty;

    const parsed = try cli.parseCommand(allocator, runtime.stderrWriter(), raw_args, attr_spec);
    const id = try store.resolve(allocator, parsed.positionals.items[0]);
    var meta = try store.getMeta(allocator, id);
    for (parsed.positionals.items[1..]) |item| try items.append(allocator, item);
    for (parsed.flags.items) |flag| {
        if (std.mem.eql(u8, flag.name, "json")) {
            json = true;
        } else if (std.mem.eql(u8, flag.name, "preview")) {
            preview = true;
        } else if (std.mem.eql(u8, flag.name, "separator")) {
            separator = flag.value.?;
        } else if (std.mem.eql(u8, flag.name, "unset")) {
            try unset.append(allocator, flag.value.?);
        }
    }

    if (unset.items.len > 0) {
        if (items.items.len > 0) return error.InvalidArgument;
        for (unset.items) |key| {
            if (!display.writableAttrKey(key)) return error.ReadOnlyAttr;
            meta.unsetAttr(key);
        }
        try store.writeMeta(allocator, id, &meta);
        return 0;
    }

    var has_write = false;
    var has_read = false;
    for (items.items) |item| {
        if (std.mem.indexOfScalar(u8, item, '=') != null) {
            has_write = true;
        } else {
            has_read = true;
        }
    }
    if (has_write and has_read) return error.InvalidArgument;
    if (has_write) {
        for (items.items) |pair| {
            const pos = std.mem.indexOfScalar(u8, pair, '=') orelse return error.InvalidArgument;
            if (!display.writableAttrKey(pair[0..pos])) return error.ReadOnlyAttr;
            try meta.setAttr(allocator, pair[0..pos], pair[pos + 1 ..]);
        }
        try store.writeMeta(allocator, id, &meta);
        return 0;
    }

    const out = runtime.stdoutWriter();
    if (json) {
        try display.printAttrJson(out, &meta, items.items, preview);
        return 0;
    }
    if (items.items.len == 1) {
        const value = display.attrValue(&meta, items.items[0], preview) orelse return error.NotFound;
        try display.printEscapedDisplay(out, value);
        try out.writeByte('\n');
        return 0;
    }
    if (items.items.len > 0) {
        for (items.items) |key| {
            const value = display.attrValue(&meta, key, preview) orelse return error.NotFound;
            try out.print("{s}{s}", .{ key, separator });
            try display.printEscapedDisplay(out, value);
            try out.writeByte('\n');
        }
        return 0;
    }
    try out.print("id{s}{s}\nts{s}{s}\nsize{s}{}\n", .{ separator, meta.id, separator, meta.ts, separator, meta.size });
    for (meta.attrs.items) |item| {
        try out.print("{s}{s}", .{ item.key, separator });
        try display.printEscapedDisplay(out, item.value);
        try out.writeByte('\n');
    }
    if (preview and meta.preview.len > 0) {
        try out.print("preview{s}", .{separator});
        try display.printEscapedDisplay(out, meta.preview);
        try out.writeByte('\n');
    }
    return 0;
}

fn cmdAttrs(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var key: ?[]const u8 = null;
    var count = false;
    const parsed = try cli.parseCommand(allocator, runtime.stderrWriter(), raw_args, attrs_spec);
    if (parsed.positionals.items.len == 1) key = parsed.positionals.items[0];
    count = parsed.present("count");
    const items = try store.visibleList(allocator);
    var counts = std.StringHashMap(usize).init(allocator);
    for (items.items) |*meta| {
        if (key) |wanted| {
            if (meta.attr(wanted)) |value| {
                const entry = try counts.getOrPut(value);
                if (!entry.found_existing) entry.value_ptr.* = 0;
                entry.value_ptr.* += 1;
            }
        } else {
            for (meta.attrs.items) |attr| {
                const entry = try counts.getOrPut(attr.key);
                if (!entry.found_existing) entry.value_ptr.* = 0;
                entry.value_ptr.* += 1;
            }
        }
    }
    var names: std.ArrayList([]const u8) = .empty;
    var it = counts.iterator();
    while (it.next()) |entry| try names.append(allocator, entry.key_ptr.*);
    std.mem.sort([]const u8, names.items, {}, ascSlices);
    const out = runtime.stdoutWriter();
    for (names.items) |name| {
        if (count) try out.print("{s}\t{}\n", .{ name, counts.get(name).? }) else try out.print("{s}\n", .{name});
    }
    return 0;
}

fn cmdLsFromOptions(allocator: Allocator, opts: *const LsCliOptions) !void {
    const id_mode = opts.id;
    var date_mode: ?DateMode = if (opts.date) .ls else null;
    var show_size = opts.size;
    const show_bytes = opts.bytes;
    var attrs_mode: AttrsMode = opts.attrs;
    var show_preview = opts.preview;
    var selection = MetaSelection{};

    if (opts.long) {
        date_mode = .ls;
        show_size = true;
        if (attrs_mode == .none) attrs_mode = .flag;
        show_preview = true;
    }
    if (attrs_mode == .list) selection.show_all = true;

    for (opts.attr) |value| {
        try parseMetaSelectionArg(allocator, &selection, value);
    }
    for (opts.pocket) |value| {
        try selection.filter_values.append(allocator, .{ .key = types.pocket_attr, .value = value });
    }
    const before_ref = opts.before;
    const after_ref = opts.after;
    if (before_ref != null and after_ref != null) return error.InvalidArgument;

    var items = try store.visibleList(allocator);
    if (before_ref) |reference| {
        const id = try store.resolve(allocator, reference);
        keepOlderThan(&items, id);
    } else if (after_ref) |reference| {
        const id = try store.resolve(allocator, reference);
        keepNewerThan(&items, id);
    }
    filterItems(&items, &selection);
    if (opts.reverse) std.mem.reverse(Meta, items.items);
    if (opts.number > 0 and items.items.len > opts.number) items.items.len = opts.number;

    if (opts.json and opts.format != null) return error.InvalidArgument;

    if (opts.format) |format| {
        try display.printLsFormat(allocator, runtime.stdoutWriter(), items.items, format, opts.width);
    } else if (opts.json) {
        try display.printLsJson(allocator, runtime.stdoutWriter(), items.items, date_mode orelse .ls);
    } else {
        try display.printLsTable(allocator, runtime.stdoutWriter(), items.items, id_mode, date_mode, show_size, show_bytes, attrs_mode, opts.name, show_preview, opts.headers, opts.width, opts.color, &selection);
    }
}

fn filterItems(items: *std.ArrayList(Meta), selection: *const MetaSelection) void {
    var write: usize = 0;
    for (items.items) |item| {
        if (matchesMetaSelection(&item, selection)) {
            items.items[write] = item;
            write += 1;
        }
    }
    items.items.len = write;
}

fn keepOlderThan(items: *std.ArrayList(Meta), id: []const u8) void {
    for (items.items, 0..) |item, idx| {
        if (std.mem.eql(u8, item.id, id)) {
            const older = items.items[idx + 1 ..];
            std.mem.copyForwards(Meta, items.items[0..older.len], older);
            items.items.len = older.len;
            return;
        }
    }
    items.items.len = 0;
}

fn keepNewerThan(items: *std.ArrayList(Meta), id: []const u8) void {
    for (items.items, 0..) |item, idx| {
        if (std.mem.eql(u8, item.id, id)) {
            items.items.len = idx;
            return;
        }
    }
    items.items.len = 0;
}

fn matchesMetaSelection(meta: *const Meta, selection: *const MetaSelection) bool {
    for (selection.filter_tags.items) |key| if (meta.attr(key) == null) return false;
    for (selection.filter_values.items) |filter| {
        const value = meta.attr(filter.key) orelse return false;
        if (!std.mem.eql(u8, value, filter.value.?)) return false;
    }
    return true;
}

fn cmdRm(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var refs: std.ArrayList([]const u8) = .empty;
    var filters: std.ArrayList(AttrFilter) = .empty;
    var before_ref: ?[]const u8 = null;
    var after_ref: ?[]const u8 = null;
    const parsed = try cli.parseCommand(allocator, runtime.stderrWriter(), raw_args, rm_spec);
    for (parsed.positionals.items) |ref| try refs.append(allocator, ref);
    for (parsed.flags.items) |flag| {
        if (std.mem.eql(u8, flag.name, "after")) {
            after_ref = flag.value.?;
        } else if (std.mem.eql(u8, flag.name, "attr")) {
            try appendFilter(allocator, &filters, flag.value.?);
        } else if (std.mem.eql(u8, flag.name, "before")) {
            before_ref = flag.value.?;
        } else if (std.mem.eql(u8, flag.name, "force")) {
            // Confirmation prompts are not implemented yet; accept the flag.
        }
    }
    if (before_ref != null and after_ref != null) return error.InvalidArgument;
    if (filters.items.len > 0) {
        if (refs.items.len > 0 or before_ref != null or after_ref != null) return error.InvalidArgument;
        const items = try store.visibleList(allocator);
        var to_remove: std.ArrayList([]const u8) = .empty;
        for (items.items) |*meta| if (matchesFilters(meta, filters.items)) try to_remove.append(allocator, meta.id);
        try store.removeIds(allocator, to_remove.items);
        return 0;
    }
    if (before_ref) |reference| {
        if (refs.items.len > 0) return error.InvalidArgument;
        const id = try store.resolve(allocator, reference);
        var items = try store.visibleList(allocator);
        keepOlderThan(&items, id);
        var to_remove: std.ArrayList([]const u8) = .empty;
        for (items.items) |meta| try to_remove.append(allocator, meta.id);
        try store.removeIds(allocator, to_remove.items);
        return 0;
    }
    if (after_ref) |reference| {
        if (refs.items.len > 0) return error.InvalidArgument;
        const id = try store.resolve(allocator, reference);
        var items = try store.visibleList(allocator);
        keepNewerThan(&items, id);
        var to_remove: std.ArrayList([]const u8) = .empty;
        for (items.items) |meta| try to_remove.append(allocator, meta.id);
        try store.removeIds(allocator, to_remove.items);
        return 0;
    }
    if (refs.items.len == 0) return error.InvalidArgument;
    for (refs.items) |r| try store.removeId(allocator, try store.resolve(allocator, r));
    return 0;
}

fn parseMetaSelectionArg(allocator: Allocator, sel: *MetaSelection, value: []const u8) !void {
    if (std.mem.startsWith(u8, value, "++")) {
        const rest = value[2..];
        if (std.mem.indexOfScalar(u8, rest, '=')) |pos| {
            try sel.display_tags.append(allocator, rest[0..pos]);
            try sel.filter_values.append(allocator, .{ .key = rest[0..pos], .value = rest[pos + 1 ..] });
        } else {
            try sel.display_tags.append(allocator, rest);
            try sel.filter_tags.append(allocator, rest);
        }
    } else if (std.mem.startsWith(u8, value, "+")) {
        try sel.display_tags.append(allocator, value[1..]);
    } else if (std.mem.indexOfScalar(u8, value, '=')) |pos| {
        try sel.filter_values.append(allocator, .{ .key = value[0..pos], .value = value[pos + 1 ..] });
    } else {
        try sel.filter_tags.append(allocator, value);
    }
}

fn ascSlices(_: void, a: []const u8, b: []const u8) bool {
    return std.mem.lessThan(u8, a, b);
}
