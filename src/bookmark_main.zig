const std = @import("std");
const stash = @import("stash");

const Allocator = std.mem.Allocator;
const Store = stash.store.Store;
const Meta = stash.types.Meta;

const bookmark_pocket = "bookmarks";

pub fn main(init: std.process.Init) !void {
    const allocator = init.arena.allocator();
    stash.runtime.process_env = init.minimal.environ;
    stash.runtime.process_io = init.io;

    const args = try init.minimal.args.toSlice(allocator);
    const code = run(allocator, args) catch |err| {
        const stderr = stash.runtime.stderrWriter();
        try stderr.print("error: {s}\n", .{errorMessage(err)});
        std.process.exit(1);
    };
    if (code != 0) std.process.exit(code);
}

fn run(allocator: Allocator, args: []const [:0]const u8) !u8 {
    if (args.len < 2) return usage();

    const command = args[1];
    if (std.mem.eql(u8, command, "ls")) {
        if (args.len != 2) return usage();
        return cmdLs(allocator);
    }
    if (std.mem.eql(u8, command, "find")) {
        if (args.len != 3) return usage();
        return cmdFind(allocator, args[2]);
    }
    if (std.mem.eql(u8, command, "grep")) {
        if (args.len != 3) return usage();
        return cmdGrep(allocator, args[2]);
    }
    if (std.mem.eql(u8, command, "title")) {
        if (args.len < 4) return usage();
        return cmdTitle(allocator, args[2], args[3..]);
    }
    if (std.mem.eql(u8, command, "--help") or std.mem.eql(u8, command, "-h")) {
        return usage();
    }

    return usage();
}

fn usage() u8 {
    const stderr = stash.runtime.stderrWriter();
    stderr.writeAll(
        \\usage:
        \\  stash-bookmark ls
        \\  stash-bookmark find <pattern>
        \\  stash-bookmark grep <pattern>
        \\  stash-bookmark title <ref> <title>
        \\
    ) catch {};
    return 1;
}

fn cmdLs(allocator: Allocator) !u8 {
    const s = try Store.open(allocator, .{});
    const items = try s.list(allocator, .{ .pocket = bookmark_pocket });
    const style = stash.term.Style.init(stash.term.stdoutIsTerminal());
    var out = try stash.term.Output.init(allocator, .{ .disable_env = "BOOKMARK_NO_PAGER" });
    errdefer out.deinit() catch {};

    for (items.items) |*meta| {
        try printBookmark(&out, meta, style);
        try out.writeByte('\n');
    }
    try out.deinit();
    return 0;
}

fn cmdFind(allocator: Allocator, pattern: []const u8) !u8 {
    const s = try Store.open(allocator, .{});
    const items = try s.list(allocator, .{ .pocket = bookmark_pocket });
    const style = stash.term.Style.init(stash.term.stdoutIsTerminal());
    var out = try stash.term.Output.init(allocator, .{ .disable_env = "BOOKMARK_NO_PAGER" });
    errdefer out.deinit() catch {};

    for (items.items) |*meta| {
        if (try bookmarkContains(allocator, &s, meta, pattern)) {
            try printBookmark(&out, meta, style);
            try out.writeByte('\n');
        }
    }
    try out.deinit();
    return 0;
}

fn cmdGrep(allocator: Allocator, pattern: []const u8) !u8 {
    const s = try Store.open(allocator, .{});
    const items = try s.list(allocator, .{ .pocket = bookmark_pocket });
    const style = stash.term.Style.init(stash.term.stdoutIsTerminal());
    var out = try stash.term.Output.init(allocator, .{ .disable_env = "BOOKMARK_NO_PAGER" });
    errdefer out.deinit() catch {};
    var printed_any = false;

    for (items.items) |*meta| {
        var context = GrepContext{
            .meta = meta,
            .out = &out,
            .style = style,
            .pattern = pattern,
            .printed_any = &printed_any,
        };
        s.scanDataLines(allocator, meta.id, &context, grepLine) catch continue;
        if (context.printed_header) try out.writeByte('\n');
    }
    try out.deinit();
    return 0;
}

fn cmdTitle(allocator: Allocator, ref: []const u8, parts: []const [:0]const u8) !u8 {
    const s = try Store.open(allocator, .{});
    const id = try s.resolve(allocator, ref, .{ .pocket = bookmark_pocket });
    var title: std.ArrayList(u8) = .empty;
    for (parts, 0..) |part, idx| {
        if (idx > 0) try title.append(allocator, ' ');
        try title.appendSlice(allocator, part);
    }
    try s.setAttr(allocator, id, "title", title.items);
    return 0;
}

fn printBookmark(out: anytype, meta: *const Meta, style: stash.term.Style) !void {
    const title = attrOrEmpty(meta, "title");
    const url = attrOrEmpty(meta, "url");
    try out.print("{s}{s}{s} > {s}{s}{s}\n", .{ style.id, meta.shortId(), style.reset, style.attr, title, style.reset });
    try out.print("{s} {s}\n", .{ stash.format.dateOnly(meta.ts), url });
}

fn attrOrEmpty(meta: *const Meta, key: []const u8) []const u8 {
    return meta.attr(key) orelse "";
}

fn bookmarkContains(allocator: Allocator, s: *const Store, meta: *const Meta, pattern: []const u8) !bool {
    var context = FindContext{ .pattern = pattern };
    try s.scanDataLines(allocator, meta.id, &context, findLine);
    return context.found;
}

fn printGrepLine(out: anytype, line_no: usize, line: []const u8, pattern: []const u8, style: stash.term.Style) !void {
    try out.print("{s}{}: ", .{ style.dim, line_no });
    if (pattern.len == 0) {
        try out.print("{s}{s}\n", .{ line, style.reset });
        return;
    }

    var rest = line;
    while (stash.format.indexOfIgnoreCaseAscii(rest, pattern)) |pos| {
        try out.writeAll(rest[0..pos]);
        const end = pos + pattern.len;
        try out.print("{s}{s}{s}", .{ style.reset, rest[pos..end], style.dim });
        rest = rest[end..];
    }

    try out.writeAll(rest);
    try out.print("{s}\n", .{style.reset});
}

const FindContext = struct {
    pattern: []const u8,
    found: bool = false,
};

fn findLine(context: *FindContext, _: usize, line: []const u8) !bool {
    if (stash.format.indexOfIgnoreCaseAscii(line, context.pattern) != null) {
        context.found = true;
        return false;
    }
    return true;
}

const GrepContext = struct {
    meta: *const Meta,
    out: *stash.term.Output,
    style: stash.term.Style,
    pattern: []const u8,
    printed_any: *bool,
    printed_header: bool = false,
};

fn grepLine(context: *GrepContext, line_no: usize, line: []const u8) !bool {
    if (stash.format.indexOfIgnoreCaseAscii(line, context.pattern) == null) return true;
    if (!context.printed_header) {
        if (context.printed_any.*) try context.out.writeByte('\n');
        try printBookmark(context.out, context.meta, context.style);
        context.printed_header = true;
        context.printed_any.* = true;
    }
    try printGrepLine(context.out, line_no, line, context.pattern, context.style);
    return true;
}

fn errorMessage(err: anyerror) []const u8 {
    return switch (err) {
        error.NotFound => "entry not found",
        error.StashEmpty => "stash is empty",
        error.IdTooShort => "id too short",
        error.AmbiguousId => "ambiguous id",
        error.InvalidRef => "invalid stack ref",
        else => @errorName(err),
    };
}
