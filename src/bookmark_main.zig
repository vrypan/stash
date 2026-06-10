const std = @import("std");
const build_options = @import("build_options");
const stash = @import("stash");

const Allocator = std.mem.Allocator;
const Store = stash.store.Store;
const Meta = stash.types.Meta;
const Attr = stash.types.Attr;
const version = build_options.version;

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
    if (std.mem.eql(u8, command, "add")) {
        if (args.len != 3) return usage();
        return cmdAdd(allocator, args[2]);
    }
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
    if (std.mem.eql(u8, command, "--version") or std.mem.eql(u8, command, "-V")) {
        try stash.runtime.stdoutWriter().print("stash-bookmark {s}\n", .{version});
        return 0;
    }
    if (args.len == 2) {
        return cmdAdd(allocator, command);
    }

    return usage();
}

fn usage() u8 {
    const stderr = stash.runtime.stderrWriter();
    stderr.writeAll(
        \\usage:
        \\  stash-bookmark <url>
        \\  stash-bookmark add <url>
        \\  stash-bookmark ls
        \\  stash-bookmark find <pattern>
        \\  stash-bookmark grep <pattern>
        \\  stash-bookmark title <ref> <title>
        \\
    ) catch {};
    return 1;
}

fn cmdAdd(allocator: Allocator, url: []const u8) !u8 {
    const html = runCaptureReportError(allocator, &.{ "curl", "-fsSL", url }) catch |err| switch (err) {
        error.FileNotFound => return error.CurlRequired,
        else => |e| return e,
    };
    const title_raw = try extractTitle(allocator, html);
    var title = try htmlTitleToText(allocator, title_raw);
    if (std.mem.trim(u8, title, " \t\r\n").len == 0) title = try allocator.dupe(u8, url);

    const html_path = try writeTempFile(allocator, "html", html);
    defer stash.runtime.cwd().deleteFile(stash.runtime.process_io, html_path) catch {};

    const text = runCaptureReportError(allocator, &.{ "html2text", "-width", "1000", html_path }) catch |err| switch (err) {
        error.FileNotFound => return error.Html2TextRequired,
        else => |e| return e,
    };
    const text_path = try writeTempFile(allocator, "txt", text);
    defer stash.runtime.cwd().deleteFile(stash.runtime.process_io, text_path) catch {};

    const attrs = [_]Attr{
        .{ .key = try allocator.dupe(u8, "pocket"), .value = try allocator.dupe(u8, bookmark_pocket) },
        .{ .key = try allocator.dupe(u8, "title"), .value = try allocator.dupe(u8, title) },
        .{ .key = try allocator.dupe(u8, "url"), .value = try allocator.dupe(u8, url) },
    };
    _ = try stash.store.pushInput(allocator, text_path, &attrs, false, true);

    const stdout = stash.runtime.stdoutWriter();
    try stdout.print("URL: {s}\nTitle: {s}\n", .{ url, title });
    return 0;
}

fn cmdLs(allocator: Allocator) !u8 {
    const s = try Store.open(allocator, .{});
    const items = try s.list(allocator, .{ .pocket = bookmark_pocket });
    const style = stash.term.Style.init(true);
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
    const style = stash.term.Style.init(true);
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
    const style = stash.term.Style.init(true);
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
        if (bookmarkMetadataContains(meta, pattern)) {
            if (printed_any) try out.writeByte('\n');
            try printBookmark(&out, meta, style);
            context.printed_header = true;
            printed_any = true;
        }
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
    if (bookmarkMetadataContains(meta, pattern)) return true;

    var context = FindContext{ .pattern = pattern };
    try s.scanDataLines(allocator, meta.id, &context, findLine);
    return context.found;
}

fn bookmarkMetadataContains(meta: *const Meta, pattern: []const u8) bool {
    if (containsIgnoreCase(meta.id, pattern)) return true;
    if (containsIgnoreCase(meta.shortId(), pattern)) return true;
    if (containsIgnoreCase(meta.ts, pattern)) return true;
    if (containsIgnoreCase(stash.format.dateOnly(meta.ts), pattern)) return true;
    if (containsIgnoreCase(meta.preview, pattern)) return true;
    for (meta.attrs.items) |attr| {
        if (containsIgnoreCase(attr.value, pattern)) return true;
    }
    return false;
}

fn containsIgnoreCase(haystack: []const u8, needle: []const u8) bool {
    return stash.format.indexOfIgnoreCaseAscii(haystack, needle) != null;
}

fn runCapture(allocator: Allocator, argv: []const []const u8) ![]u8 {
    const result = try runProcess(allocator, argv);
    switch (result.term) {
        .exited => |code| if (code == 0) return result.stdout,
        else => {},
    }
    return error.ExternalCommandFailed;
}

fn runCaptureReportError(allocator: Allocator, argv: []const []const u8) ![]u8 {
    const result = try runProcess(allocator, argv);
    switch (result.term) {
        .exited => |code| if (code == 0) return result.stdout,
        else => {},
    }
    const stderr = stash.runtime.stderrWriter();
    const message = std.mem.trim(u8, result.stderr, " \t\r\n");
    if (message.len > 0) {
        try stderr.print("{s}\n", .{message});
    }
    return error.ExternalCommandFailed;
}

fn runProcess(allocator: Allocator, argv: []const []const u8) !std.process.RunResult {
    const result = try std.process.run(allocator, stash.runtime.process_io, .{
        .argv = argv,
        .stdout_limit = .limited(64 * 1024 * 1024),
        .stderr_limit = .limited(1024 * 1024),
    });
    return result;
}

fn writeTempFile(allocator: Allocator, suffix: []const u8, data: []const u8) ![]u8 {
    const ns = std.Io.Clock.real.now(stash.runtime.process_io).toNanoseconds();
    const path = try std.fmt.allocPrint(allocator, "/tmp/stash-bookmark-{d}.{s}", .{ ns, suffix });
    var file = try stash.runtime.cwd().createFile(stash.runtime.process_io, path, .{});
    defer file.close(stash.runtime.process_io);
    try file.writeStreamingAll(stash.runtime.process_io, data);
    return path;
}

fn extractTitle(allocator: Allocator, html: []const u8) ![]const u8 {
    const lower = try allocator.dupe(u8, html);
    for (lower) |*ch| ch.* = std.ascii.toLower(ch.*);
    const open_start = std.mem.indexOf(u8, lower, "<title") orelse return "";
    const open_rel_end = std.mem.indexOfScalar(u8, lower[open_start..], '>') orelse return "";
    const content_start = open_start + open_rel_end + 1;
    const close_rel_start = std.mem.indexOf(u8, lower[content_start..], "</title") orelse return "";
    return std.mem.trim(u8, html[content_start .. content_start + close_rel_start], " \t\r\n");
}

fn htmlTitleToText(allocator: Allocator, input: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    var in_tag = false;
    var i: usize = 0;
    while (i < input.len) : (i += 1) {
        const ch = input[i];
        if (ch == '<') {
            in_tag = true;
            continue;
        }
        if (ch == '>') {
            in_tag = false;
            continue;
        }
        if (in_tag) continue;
        if (ch == '&') {
            if (decodeHtmlEntity(allocator, input, &i)) |decoded| {
                try out.appendSlice(allocator, decoded);
                continue;
            }
        }
        try out.append(allocator, ch);
    }
    return allocator.dupe(u8, std.mem.trim(u8, out.items, " \t\r\n"));
}

fn decodeHtmlEntity(allocator: Allocator, input: []const u8, index: *usize) ?[]const u8 {
    const start = index.*;
    const rel_end = std.mem.indexOfScalar(u8, input[start..], ';') orelse return null;
    if (rel_end > 16) return null;
    const name = input[start + 1 .. start + rel_end];
    index.* = start + rel_end;
    if (std.mem.eql(u8, name, "amp")) return "&";
    if (std.mem.eql(u8, name, "lt")) return "<";
    if (std.mem.eql(u8, name, "gt")) return ">";
    if (std.mem.eql(u8, name, "quot")) return "\"";
    if (std.mem.eql(u8, name, "apos")) return "'";
    if (std.mem.eql(u8, name, "nbsp")) return " ";
    if (name.len > 1 and name[0] == '#') {
        const base: u8 = if (name[1] == 'x' or name[1] == 'X') 16 else 10;
        const digits = if (base == 16) name[2..] else name[1..];
        const cp = std.fmt.parseInt(u21, digits, base) catch return null;
        var buf: [4]u8 = undefined;
        const len = std.unicode.utf8Encode(cp, &buf) catch return null;
        return allocator.dupe(u8, buf[0..len]) catch null;
    }
    return null;
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
        error.CurlRequired => "curl is required",
        error.Html2TextRequired => "html2text is required",
        error.ExternalCommandFailed => "external command failed",
        error.NotFound => "entry not found",
        error.StashEmpty => "stash is empty",
        error.IdTooShort => "id too short",
        error.AmbiguousId => "ambiguous id",
        error.InvalidRef => "invalid stack ref",
        else => @errorName(err),
    };
}
