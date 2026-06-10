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
    var out: std.ArrayList(u8) = .empty;

    for (items.items) |*meta| {
        try appendBookmark(allocator, &out, meta, style, .date_dim);
        try out.append(allocator, '\n');
    }
    try writeOutput(allocator, out.items);
    return 0;
}

fn cmdFind(allocator: Allocator, pattern: []const u8) !u8 {
    const s = try Store.open(allocator, .{});
    const items = try s.list(allocator, .{ .pocket = bookmark_pocket });
    const style = stash.term.Style.init(stash.term.stdoutIsTerminal());
    var out: std.ArrayList(u8) = .empty;

    for (items.items) |*meta| {
        if (try bookmarkContains(allocator, &s, meta, pattern)) {
            try appendBookmark(allocator, &out, meta, style, .date_dim);
            try out.append(allocator, '\n');
        }
    }
    try writeOutput(allocator, out.items);
    return 0;
}

fn cmdGrep(allocator: Allocator, pattern: []const u8) !u8 {
    const s = try Store.open(allocator, .{});
    const items = try s.list(allocator, .{ .pocket = bookmark_pocket });
    const style = stash.term.Style.init(stash.term.stdoutIsTerminal());
    var out: std.ArrayList(u8) = .empty;
    var printed_any = false;

    for (items.items) |*meta| {
        const path = try s.dataPath(allocator, meta.id);
        const data = stash.runtime.cwd().readFileAlloc(stash.runtime.process_io, path, allocator, .limited(64 * 1024 * 1024)) catch continue;
        var lines = std.mem.splitScalar(u8, data, '\n');
        var line_no: usize = 1;
        var printed_header = false;
        while (lines.next()) |raw_line| : (line_no += 1) {
            const line = std.mem.trim(u8, raw_line, "\r");
            if (indexOfIgnoreCase(line, pattern) == null) continue;
            if (!printed_header) {
                if (printed_any) try out.append(allocator, '\n');
                try appendBookmark(allocator, &out, meta, style, .normal_header);
                printed_header = true;
                printed_any = true;
            }
            try appendGrepLine(allocator, &out, line_no, line, pattern, style);
        }
        if (printed_header) try out.append(allocator, '\n');
    }
    try writeOutput(allocator, out.items);
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

const BookmarkHeader = enum { date_dim, normal_header };

fn appendBookmark(allocator: Allocator, out: *std.ArrayList(u8), meta: *const Meta, style: stash.term.Style, header: BookmarkHeader) !void {
    const title = attrOrEmpty(meta, "title");
    const url = attrOrEmpty(meta, "url");
    switch (header) {
        .date_dim => try appendFmt(allocator, out, "{s}{s}{s} {s}\n", .{ style.dim, bookmarkDate(meta.ts), style.reset, title }),
        .normal_header => try appendFmt(allocator, out, "{s} {s}\n", .{ bookmarkDate(meta.ts), title }),
    }
    try appendFmt(allocator, out, "{s}>>{s} {s}{s}\n", .{ style.dim, meta.shortId(), url, style.reset });
}

fn bookmarkDate(ts: []const u8) []const u8 {
    if (std.mem.indexOfScalar(u8, ts, 'T')) |pos| return ts[0..pos];
    return ts;
}

fn attrOrEmpty(meta: *const Meta, key: []const u8) []const u8 {
    return meta.attr(key) orelse "";
}

fn bookmarkContains(allocator: Allocator, s: *const Store, meta: *const Meta, pattern: []const u8) !bool {
    const path = try s.dataPath(allocator, meta.id);
    const data = stash.runtime.cwd().readFileAlloc(stash.runtime.process_io, path, allocator, .limited(64 * 1024 * 1024)) catch return false;
    return indexOfIgnoreCase(data, pattern) != null;
}

fn appendGrepLine(allocator: Allocator, out: *std.ArrayList(u8), line_no: usize, line: []const u8, pattern: []const u8, style: stash.term.Style) !void {
    try appendFmt(allocator, out, "{s}{}: ", .{ style.dim, line_no });
    if (pattern.len == 0) {
        try appendFmt(allocator, out, "{s}{s}\n", .{ line, style.reset });
        return;
    }

    var rest = line;
    while (indexOfIgnoreCase(rest, pattern)) |pos| {
        try out.appendSlice(allocator, rest[0..pos]);
        const end = pos + pattern.len;
        try appendFmt(allocator, out, "{s}{s}{s}", .{ style.reset, rest[pos..end], style.dim });
        rest = rest[end..];
    }

    try out.appendSlice(allocator, rest);
    try appendFmt(allocator, out, "{s}\n", .{style.reset});
}

fn indexOfIgnoreCase(haystack: []const u8, needle: []const u8) ?usize {
    if (needle.len == 0) return 0;
    if (needle.len > haystack.len) return null;
    var i: usize = 0;
    while (i + needle.len <= haystack.len) : (i += 1) {
        var j: usize = 0;
        while (j < needle.len) : (j += 1) {
            if (std.ascii.toLower(haystack[i + j]) != std.ascii.toLower(needle[j])) break;
        } else {
            return i;
        }
    }
    return null;
}

fn appendFmt(allocator: Allocator, out: *std.ArrayList(u8), comptime fmt: []const u8, args: anytype) !void {
    const text = try std.fmt.allocPrint(allocator, fmt, args);
    try out.appendSlice(allocator, text);
}

fn writeOutput(allocator: Allocator, content: []const u8) !void {
    if (stash.term.shouldPage()) {
        try stash.term.page(allocator, content);
    } else {
        try stash.runtime.stdoutWriter().writeAll(content);
    }
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
