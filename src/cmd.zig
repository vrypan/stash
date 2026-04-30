const std = @import("std");
const build_options = @import("build_options");
const zli = @import("zli");
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

var active_allocator: Allocator = undefined;

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
    active_allocator = allocator;
    if (args.len <= 1) {
        return runZli(init);
    }
    const first = args[1];
    if (std.mem.eql(u8, first, "--help") or std.mem.eql(u8, first, "-h")) {
        return runZli(init);
    }
    if (std.mem.eql(u8, first, "--version") or std.mem.eql(u8, first, "-V")) {
        try runtime.stdoutWriter().print("stash {s}\n", .{version});
        return 0;
    }
    if (args.len > 2 and (std.mem.eql(u8, args[2], "--help") or std.mem.eql(u8, args[2], "-h"))) {
        return runZli(init);
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
    if (std.mem.eql(u8, first, "pop")) return cmdPop(allocator);

    return cmdPush(allocator, args[1..], .auto);
}

fn zliRoot(ctx: zli.CommandContext) !void {
    _ = ctx;
    _ = try cmdPush(active_allocator, &.{}, .auto);
}

fn zliHelp(ctx: zli.CommandContext) !void {
    try ctx.command.printHelp();
}

fn semanticVersion() ?std.SemanticVersion {
    return std.SemanticVersion.parse(version) catch null;
}

fn addBoolFlag(cmd: *zli.Command, name: []const u8, shortcut: ?[]const u8, description: []const u8, default: bool) !void {
    try cmd.addFlag(.{
        .name = name,
        .shortcut = shortcut,
        .description = description,
        .type = .Bool,
        .default_value = .{ .Bool = default },
    });
}

fn addStringFlag(cmd: *zli.Command, name: []const u8, shortcut: ?[]const u8, description: []const u8, default: []const u8) !void {
    try cmd.addFlag(.{
        .name = name,
        .shortcut = shortcut,
        .description = description,
        .type = .String,
        .default_value = .{ .String = default },
    });
}

fn addIntFlag(cmd: *zli.Command, name: []const u8, shortcut: ?[]const u8, description: []const u8, default: i32) !void {
    try cmd.addFlag(.{
        .name = name,
        .shortcut = shortcut,
        .description = description,
        .type = .Int,
        .default_value = .{ .Int = default },
    });
}

fn zliCommand(init_opts: zli.InitOptions, name: []const u8, description: []const u8) !*zli.Command {
    return zli.Command.init(init_opts, .{
        .name = name,
        .description = description,
    }, zliHelp);
}

fn buildZli(init_opts: zli.InitOptions) !*zli.Command {
    const root = try zli.Command.init(init_opts, .{
        .name = "stash",
        .description = "A local store for piped output and files.",
        .version = semanticVersion(),
        .help = "More info: https://github.com/vrypan/stash",
    }, zliRoot);
    try addStringFlag(root, "attr", "a", "Set attribute key=value", "");
    try addStringFlag(root, "pocket", null, "Alias for --attr pocket=VALUE", "");
    try addStringFlag(root, "print", null, "Where to print the generated entry ID: stdout, stderr, null, 1, 2, or 0", "null");
    try root.addPositionalArg(.{ .name = "FILE", .description = "Optional file to stash; reads stdin when omitted", .required = false });

    const push = try zliCommand(init_opts, "push", "Store stdin and return the entry key");
    try addStringFlag(push, "attr", "a", "Set attribute key=value", "");
    try addStringFlag(push, "pocket", null, "Alias for --attr pocket=VALUE", "");
    try addStringFlag(push, "print", null, "Where to print the generated entry ID: stdout, stderr, null, 1, 2, or 0", "null");
    try push.addPositionalArg(.{ .name = "FILE", .description = "Optional file to stash; reads stdin when omitted", .required = false });

    const tee = try zliCommand(init_opts, "tee", "Store stdin and forward it to stdout");
    try addStringFlag(tee, "attr", "a", "Set attribute key=value", "");
    try addStringFlag(tee, "pocket", null, "Alias for --attr pocket=VALUE", "");
    try addStringFlag(tee, "print", null, "Where to print the generated entry ID: stdout, stderr, null, 1, 2, or 0", "null");
    try addBoolFlag(tee, "save-on-error", null, "Save captured input if the input stream is interrupted", true);

    const cat = try zliCommand(init_opts, "cat", "Print an entry's raw data to stdout");
    try addStringFlag(cat, "attr", "a", "Attribute filter: name or name=value", "");
    try addStringFlag(cat, "pocket", null, "Alias for --attr pocket=VALUE", "");
    try addBoolFlag(cat, "reverse", "r", "Print matching refs oldest first", false);
    try cat.addPositionalArg(.{ .name = "REF", .description = "Entry ID, stack ref, or stack number", .required = false, .variadic = true });

    const ls = try zliCommand(init_opts, "ls", "List entries");
    try addStringFlag(ls, "after", null, "Show entries newer than the referenced entry", "");
    try addStringFlag(ls, "attr", "a", "Filter or display attributes", "");
    try addStringFlag(ls, "attrs", null, "Attribute display: list, count, or flag", "none");
    try addStringFlag(ls, "before", null, "Show entries older than the referenced entry", "");
    try addBoolFlag(ls, "bytes", null, "Use raw byte counts for the size column", false);
    try addBoolFlag(ls, "color", null, "Color output", true);
    try addBoolFlag(ls, "date", null, "Include date column using ls-style dates", false);
    try addStringFlag(ls, "format", null, "Print entries using a format string", "");
    try addBoolFlag(ls, "headers", null, "Print a header row for tabular output", false);
    try addStringFlag(ls, "id", null, "ID display: short, full, or pos", "short");
    try addBoolFlag(ls, "json", null, "Output listing as rich JSON", false);
    try addBoolFlag(ls, "long", "l", "Alias for --date --size --attrs=flag --preview", false);
    try addBoolFlag(ls, "name", null, "Include filename attribute if available", false);
    try addIntFlag(ls, "number", "n", "Limit number of entries shown", 0);
    try addStringFlag(ls, "pocket", null, "Alias for --attr pocket=VALUE", "");
    try addBoolFlag(ls, "preview", "p", "Append compact preview text", false);
    try addBoolFlag(ls, "reverse", "r", "Show oldest first", false);
    try addBoolFlag(ls, "size", null, "Include human-readable size column", false);
    try addIntFlag(ls, "width", "w", "Maximum output line width; 0 uses terminal width", 0);

    const attr = try zliCommand(init_opts, "attr", "Show or update entry attributes");
    try addBoolFlag(attr, "json", null, "Print attributes as JSON", false);
    try addBoolFlag(attr, "preview", "p", "Include the preview pseudo-attribute", false);
    try addStringFlag(attr, "separator", null, "Separator for text output", "\t");
    try addStringFlag(attr, "unset", null, "Remove a writable attribute", "");
    try attr.addPositionalArg(.{ .name = "ARG", .description = "Entry ref followed by attribute names or key=value assignments", .required = false, .variadic = true });

    const attrs = try zliCommand(init_opts, "attrs", "List attribute keys across the stash");
    try addBoolFlag(attrs, "count", null, "Print counts", false);
    try attrs.addPositionalArg(.{ .name = "KEY", .description = "Attribute key to list distinct values for", .required = false });

    const path = try zliCommand(init_opts, "path", "Print stash paths");
    try addBoolFlag(path, "attr", "a", "Print attr path instead of data path", false);
    try addBoolFlag(path, "dir", "d", "Print stash root directory", false);
    try path.addPositionalArg(.{ .name = "REF", .description = "Entry ID, stack ref, or stack number", .required = false });

    const rm = try zliCommand(init_opts, "rm", "Remove entries");
    try addBoolFlag(rm, "force", "f", "Skip confirmation prompts", false);
    try addStringFlag(rm, "attr", "a", "Attribute filter: name or name=value", "");
    try addStringFlag(rm, "before", null, "Remove entries older than the referenced entry", "");
    try addStringFlag(rm, "after", null, "Remove entries newer than the referenced entry", "");
    try rm.addPositionalArg(.{ .name = "REF", .description = "Entry ID, stack ref, or stack number", .required = false, .variadic = true });

    const pop = try zliCommand(init_opts, "pop", "Print the newest entry and remove it");

    try root.addCommands(&.{ push, tee, cat, ls, attr, attrs, path, rm, pop });
    return root;
}

fn runZli(init: *const std.process.Init) !u8 {
    var stdout_buf: [4096]u8 = undefined;
    var stdout_writer = std.Io.File.stdout().writer(runtime.process_io, &stdout_buf);
    var stdin_buf: [4096]u8 = undefined;
    var stdin_reader = std.Io.File.stdin().reader(runtime.process_io, &stdin_buf);

    const init_opts = zli.InitOptions{
        .io = runtime.process_io,
        .writer = &stdout_writer.interface,
        .reader = &stdin_reader.interface,
        .allocator = active_allocator,
    };
    var root = try buildZli(init_opts);
    defer root.deinit();
    var args_iter = try init.minimal.args.iterateAllocator(active_allocator);
    defer args_iter.deinit();
    try root.execute(&args_iter, .{});
    try stdout_writer.interface.flush();
    return 0;
}

fn cmdLs(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var opts = LsCliOptions{};
    var i: usize = 0;
    while (i < raw_args.len) : (i += 1) {
        const arg = raw_args[i];
        if (std.mem.eql(u8, arg, "--id")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            opts.id = try parseIdMode(raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--id=")) {
            opts.id = try parseIdMode(arg["--id=".len..]);
        } else if (std.mem.eql(u8, arg, "-a") or std.mem.eql(u8, arg, "--attr")) {
            try appendConstSlice(allocator, &opts.attr, try nextArgValue(raw_args, &i));
        } else if (std.mem.startsWith(u8, arg, "-a") and arg.len > 2) {
            try appendConstSlice(allocator, &opts.attr, arg[2..]);
        } else if (std.mem.startsWith(u8, arg, "--attr=")) {
            try appendConstSlice(allocator, &opts.attr, arg["--attr=".len..]);
        } else if (std.mem.eql(u8, arg, "--pocket")) {
            try appendConstSlice(allocator, &opts.pocket, try nextArgValue(raw_args, &i));
        } else if (std.mem.startsWith(u8, arg, "--pocket=")) {
            try appendConstSlice(allocator, &opts.pocket, arg["--pocket=".len..]);
        } else if (std.mem.eql(u8, arg, "--attrs")) {
            opts.attrs = try parseAttrsMode(try nextArgValue(raw_args, &i));
        } else if (std.mem.startsWith(u8, arg, "--attrs=")) {
            opts.attrs = try parseAttrsMode(arg["--attrs=".len..]);
        } else if (std.mem.eql(u8, arg, "-n") or std.mem.eql(u8, arg, "--number")) {
            opts.number = try parseUsize(try nextArgValue(raw_args, &i));
        } else if (std.mem.startsWith(u8, arg, "-n") and arg.len > 2) {
            opts.number = try parseUsize(arg[2..]);
        } else if (std.mem.startsWith(u8, arg, "--number=")) {
            opts.number = try parseUsize(arg["--number=".len..]);
        } else if (std.mem.eql(u8, arg, "--before")) {
            opts.before = try allocator.dupe(u8, try nextArgValue(raw_args, &i));
        } else if (std.mem.startsWith(u8, arg, "--before=")) {
            opts.before = try allocator.dupe(u8, arg["--before=".len..]);
        } else if (std.mem.eql(u8, arg, "--after")) {
            opts.after = try allocator.dupe(u8, try nextArgValue(raw_args, &i));
        } else if (std.mem.startsWith(u8, arg, "--after=")) {
            opts.after = try allocator.dupe(u8, arg["--after=".len..]);
        } else if (std.mem.eql(u8, arg, "-r") or std.mem.eql(u8, arg, "--reverse")) {
            opts.reverse = true;
        } else if (std.mem.eql(u8, arg, "--json")) {
            opts.json = true;
        } else if (std.mem.eql(u8, arg, "--headers")) {
            opts.headers = true;
        } else if (std.mem.eql(u8, arg, "--date")) {
            opts.date = true;
        } else if (std.mem.eql(u8, arg, "--size")) {
            opts.size = true;
        } else if (std.mem.eql(u8, arg, "--bytes")) {
            opts.bytes = true;
        } else if (std.mem.eql(u8, arg, "--name")) {
            opts.name = true;
        } else if (std.mem.eql(u8, arg, "-p") or std.mem.eql(u8, arg, "--preview")) {
            opts.preview = true;
        } else if (std.mem.eql(u8, arg, "--format")) {
            opts.format = try allocator.dupe(u8, try nextArgValue(raw_args, &i));
        } else if (std.mem.startsWith(u8, arg, "--format=")) {
            opts.format = try allocator.dupe(u8, arg["--format=".len..]);
        } else if (std.mem.eql(u8, arg, "-l") or std.mem.eql(u8, arg, "--long")) {
            opts.long = true;
        } else if (std.mem.eql(u8, arg, "-w") or std.mem.eql(u8, arg, "--width")) {
            opts.width = try parseUsize(try nextArgValue(raw_args, &i));
        } else if (std.mem.startsWith(u8, arg, "-w") and arg.len > 2) {
            opts.width = try parseUsize(arg[2..]);
        } else if (std.mem.startsWith(u8, arg, "--width=")) {
            opts.width = try parseUsize(arg["--width=".len..]);
        } else if (std.mem.eql(u8, arg, "--color")) {
            opts.color = try parseBool(try nextArgValue(raw_args, &i));
        } else if (std.mem.startsWith(u8, arg, "--color=")) {
            opts.color = try parseBool(arg["--color=".len..]);
        } else {
            return error.InvalidArgument;
        }
    }
    try cmdLsFromOptions(allocator, &opts);
    return 0;
}

fn nextArgValue(raw_args: []const [:0]const u8, index: *usize) ![]const u8 {
    index.* += 1;
    if (index.* >= raw_args.len) return error.InvalidArgument;
    return raw_args[index.*];
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

    var i: usize = 0;
    while (i < raw_args.len) : (i += 1) {
        const arg = raw_args[i];
        if (std.mem.eql(u8, arg, "-a") or std.mem.eql(u8, arg, "--attr")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try appendAttrFlag(allocator, &attrs, raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "-a") and arg.len > 2) {
            try appendAttrFlag(allocator, &attrs, arg[2..]);
        } else if (std.mem.startsWith(u8, arg, "--attr=")) {
            try appendAttrFlag(allocator, &attrs, arg["--attr=".len..]);
        } else if (std.mem.eql(u8, arg, "--pocket")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try setAttrList(allocator, &attrs, types.pocket_attr, raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--pocket=")) {
            try setAttrList(allocator, &attrs, types.pocket_attr, arg["--pocket=".len..]);
        } else if (std.mem.eql(u8, arg, "--print")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            print_target = try parsePrintTarget(raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--print=")) {
            print_target = try parsePrintTarget(arg["--print=".len..]);
        } else if (std.mem.eql(u8, arg, "--save-on-error")) {
            if (i + 1 < raw_args.len and !std.mem.startsWith(u8, raw_args[i + 1], "-")) {
                i += 1;
                save_on_error = try parseBool(raw_args[i]);
            } else {
                save_on_error = true;
            }
        } else if (std.mem.startsWith(u8, arg, "--save-on-error=")) {
            save_on_error = try parseBool(arg["--save-on-error=".len..]);
        } else if (mode != .tee and file_arg == null) {
            file_arg = arg;
        } else {
            return error.InvalidArgument;
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
    var i: usize = 0;
    while (i < raw_args.len) : (i += 1) {
        const arg = raw_args[i];
        if (allow_reverse and (std.mem.eql(u8, arg, "-r") or std.mem.eql(u8, arg, "--reverse"))) {
            reverse.* = true;
        } else if (std.mem.eql(u8, arg, "-a") or std.mem.eql(u8, arg, "--attr")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try appendFilter(allocator, filters, raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--attr=")) {
            try appendFilter(allocator, filters, arg["--attr=".len..]);
        } else if (std.mem.eql(u8, arg, "--pocket")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try filters.append(allocator, .{ .key = types.pocket_attr, .value = raw_args[i] });
        } else if (std.mem.startsWith(u8, arg, "--pocket=")) {
            try filters.append(allocator, .{ .key = types.pocket_attr, .value = arg["--pocket=".len..] });
        } else {
            try refs.append(allocator, arg);
        }
    }
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
    for (raw_args) |raw| {
        const arg = raw;
        if (std.mem.eql(u8, arg, "-a") or std.mem.eql(u8, arg, "--attr")) want_attr = true else if (std.mem.eql(u8, arg, "-d") or std.mem.eql(u8, arg, "--dir")) want_dir = true else ref = arg;
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
    if (raw_args.len == 0) return error.InvalidArgument;
    const id = try store.resolve(allocator, raw_args[0]);
    var meta = try store.getMeta(allocator, id);
    var json = false;
    var preview = false;
    var separator: []const u8 = "\t";
    var unset: std.ArrayList([]const u8) = .empty;
    var items: std.ArrayList([]const u8) = .empty;

    var i: usize = 1;
    while (i < raw_args.len) : (i += 1) {
        const arg = raw_args[i];
        if (std.mem.eql(u8, arg, "--json")) json = true else if (std.mem.eql(u8, arg, "-p") or std.mem.eql(u8, arg, "--preview")) preview = true else if (std.mem.eql(u8, arg, "--separator")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            separator = raw_args[i];
        } else if (std.mem.startsWith(u8, arg, "--separator=")) {
            separator = arg["--separator=".len..];
        } else if (std.mem.eql(u8, arg, "--unset")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try unset.append(allocator, raw_args[i]);
        } else {
            try items.append(allocator, arg);
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
    for (raw_args) |raw| {
        const arg = raw;
        if (std.mem.eql(u8, arg, "--count")) count = true else key = arg;
    }
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
    var i: usize = 0;
    while (i < raw_args.len) : (i += 1) {
        const arg = raw_args[i];
        if (std.mem.eql(u8, arg, "-f") or std.mem.eql(u8, arg, "--force")) {
            // Confirmation prompts are not implemented yet; accept the flag.
        } else if (std.mem.eql(u8, arg, "-a") or std.mem.eql(u8, arg, "--attr")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try appendFilter(allocator, &filters, raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--attr=")) {
            try appendFilter(allocator, &filters, arg["--attr=".len..]);
        } else if (std.mem.eql(u8, arg, "--before")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            before_ref = raw_args[i];
        } else if (std.mem.startsWith(u8, arg, "--before=")) {
            before_ref = arg["--before=".len..];
        } else if (std.mem.eql(u8, arg, "--after")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            after_ref = raw_args[i];
        } else if (std.mem.startsWith(u8, arg, "--after=")) {
            after_ref = arg["--after=".len..];
        } else {
            try refs.append(allocator, arg);
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

fn cmdPop(allocator: Allocator) !u8 {
    const id = try store.resolve(allocator, "");
    try store.catId(allocator, id, runtime.stdoutWriter());
    try store.removeId(allocator, id);
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
