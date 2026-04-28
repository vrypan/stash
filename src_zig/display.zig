const std = @import("std");
const types = @import("types.zig");

const Allocator = std.mem.Allocator;
const Meta = types.Meta;
const MetaSelection = types.MetaSelection;
const IdMode = types.IdMode;
const DateMode = types.DateMode;
const SizeMode = types.SizeMode;
const AttrsMode = types.AttrsMode;

pub fn attrValue(meta: *const Meta, key: []const u8, include_preview: bool) ?[]const u8 {
    if (std.mem.eql(u8, key, "id")) return meta.id;
    if (std.mem.eql(u8, key, "ts")) return meta.ts;
    if (std.mem.eql(u8, key, "size")) return null;
    if (std.mem.eql(u8, key, "preview")) return if (include_preview and meta.preview.len > 0) meta.preview else null;
    return meta.attr(key);
}

pub fn printAttrJson(writer: anytype, meta: *const Meta, keys: []const []const u8, include_preview: bool) !void {
    try writer.writeAll("{\n");
    var first = true;
    if (keys.len == 0) {
        try jsonField(writer, &first, "id", meta.id);
        try jsonField(writer, &first, "ts", meta.ts);
        try writer.print("{s}\"size\": {}\n", .{ if (first) "  " else ",\n  ", meta.size });
        first = false;
        for (meta.attrs.items) |item| try jsonField(writer, &first, item.key, item.value);
        if (include_preview and meta.preview.len > 0) try jsonField(writer, &first, "preview", meta.preview);
    } else {
        for (keys) |key| {
            const value = attrValue(meta, key, include_preview) orelse return error.NotFound;
            try jsonField(writer, &first, key, value);
        }
    }
    try writer.writeAll("\n}\n");
}

fn jsonField(writer: anytype, first: *bool, key: []const u8, value: []const u8) !void {
    try writer.writeAll(if (first.*) "  " else ",\n  ");
    first.* = false;
    try printJsonString(writer, key);
    try writer.writeAll(": ");
    try printJsonString(writer, value);
}

pub fn printLsTable(
    allocator: Allocator,
    out: anytype,
    items: []const Meta,
    id_mode: IdMode,
    date_mode: ?DateMode,
    size_mode: ?SizeMode,
    attrs_mode: AttrsMode,
    show_name: bool,
    show_preview: bool,
    headers: bool,
    chars: usize,
    selection: *const MetaSelection,
) !void {
    if (headers) {
        try out.writeAll("id");
        if (size_mode != null) try out.writeAll("\tsize");
        if (date_mode != null) try out.writeAll("\tdate");
        if (attrs_mode == .count or attrs_mode == .flag or selection.show_all) try out.writeAll("\tattrs");
        if (show_name) try out.writeAll("\tname");
        for (selection.display_tags.items) |tag| try out.print("\t{s}", .{tag});
        if (show_preview) try out.writeAll("\tpreview");
        try out.writeByte('\n');
    }
    for (items, 0..) |*meta, idx| {
        try out.print("{s}", .{displayId(meta, idx, id_mode)});
        if (size_mode) |mode| {
            const s = if (mode == .bytes) try std.fmt.allocPrint(allocator, "{}", .{meta.size}) else try humanSize(allocator, meta.size);
            try out.print("\t{s}", .{s});
        }
        if (date_mode) |mode| try out.print("\t{s}", .{try formatDate(allocator, meta.ts, mode)});
        if (attrs_mode == .count) try out.print("\t{}", .{meta.attrs.items.len});
        if (attrs_mode == .flag) try out.print("\t{s}", .{if (meta.attrs.items.len > 0) "*" else ""});
        if (show_name) try out.print("\t{s}", .{meta.attr("filename") orelse meta.id});
        for (selection.display_tags.items) |tag| try out.print("\t{s}", .{meta.attr(tag) orelse ""});
        if (selection.show_all) {
            try out.writeByte('\t');
            for (meta.attrs.items, 0..) |attr, n| {
                if (n > 0) try out.writeAll("  ");
                try printEscapedDisplay(out, attr.value);
            }
        }
        if (show_preview and meta.preview.len > 0) try out.print("\t{s}", .{try previewSnippet(allocator, meta.preview, chars)});
        try out.writeByte('\n');
    }
}

pub fn printLsJson(allocator: Allocator, out: anytype, items: []const Meta, date_mode: DateMode, chars: usize) !void {
    try out.writeAll("[\n");
    for (items, 0..) |*meta, idx| {
        if (idx > 0) try out.writeAll(",\n");
        try out.writeAll("  {\n");
        var first = true;
        try jsonFieldIndented(out, &first, "id", meta.id, 4);
        try jsonFieldIndented(out, &first, "short_id", meta.shortId(), 4);
        try jsonFieldIndented(out, &first, "stack_ref", try std.fmt.allocPrint(allocator, "{}", .{idx + 1}), 4);
        try jsonFieldIndented(out, &first, "ts", meta.ts, 4);
        try jsonFieldIndented(out, &first, "date", try formatDate(allocator, meta.ts, date_mode), 4);
        try out.print(",\n    \"size\": {}", .{meta.size});
        try jsonFieldIndented(out, &first, "size_human", try humanSize(allocator, meta.size), 4);
        for (meta.attrs.items) |attr| try jsonFieldIndented(out, &first, attr.key, attr.value, 4);
        if (meta.preview.len > 0) {
            try out.writeAll(",\n    \"preview\": [");
            try printJsonString(out, try previewSnippet(allocator, meta.preview, chars));
            try out.writeByte(']');
        }
        try out.writeAll("\n  }");
    }
    try out.writeAll("\n]\n");
}

fn jsonFieldIndented(writer: anytype, first: *bool, key: []const u8, value: []const u8, indent: usize) !void {
    if (first.*) {
        first.* = false;
    } else {
        try writer.writeAll(",\n");
    }
    try writer.writeByteNTimes(' ', indent);
    try printJsonString(writer, key);
    try writer.writeAll(": ");
    try printJsonString(writer, value);
}

fn displayId(meta: *const Meta, idx: usize, mode: IdMode) []const u8 {
    return switch (mode) {
        .full => meta.id,
        .short => meta.shortId(),
        .pos => std.fmt.allocPrint(std.heap.page_allocator, "{}", .{idx + 1}) catch meta.shortId(),
    };
}

pub fn writableAttrKey(key: []const u8) bool {
    if (std.mem.eql(u8, key, "id") or std.mem.eql(u8, key, "ts") or std.mem.eql(u8, key, "size") or std.mem.eql(u8, key, "preview")) return false;
    if (key.len == 0 or key[0] == '-' or key[key.len - 1] == '-') return false;
    var prev_dash = false;
    for (key) |ch| {
        const ok = std.ascii.isAlphanumeric(ch) or ch == '_' or ch == '-';
        if (!ok) return false;
        if (ch == '-') {
            if (prev_dash) return false;
            prev_dash = true;
        } else prev_dash = false;
    }
    return true;
}

fn formatDate(allocator: Allocator, ts: []const u8, mode: DateMode) ![]u8 {
    if (mode == .iso) return allocator.dupe(u8, ts);
    const parts = parseTsParts(ts) orelse return allocator.dupe(u8, ts);
    if (mode == .ls) {
        const now = civilFromUnix(@intCast(@divFloor(nowNs(), std.time.ns_per_s)));
        const mons = [_][]const u8{ "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec" };
        if (parts.year == now.year) return std.fmt.allocPrint(allocator, "{s} {d: >2} {d:0>2}:{d:0>2}", .{ mons[parts.month - 1], parts.day, parts.hour, parts.min });
        return std.fmt.allocPrint(allocator, "{s} {d: >2}  {d}", .{ mons[parts.month - 1], parts.day, parts.year });
    }
    const then = unixFromParts(parts);
    const delta = @max(@as(i64, 0), @as(i64, @intCast(@divFloor(nowNs(), std.time.ns_per_s))) - then);
    if (delta < 60) return std.fmt.allocPrint(allocator, "{}s ago", .{delta});
    if (delta < 3600) return std.fmt.allocPrint(allocator, "{}m ago", .{@divFloor(delta, 60)});
    if (delta < 86400) return std.fmt.allocPrint(allocator, "{}h ago", .{@divFloor(delta, 3600)});
    return std.fmt.allocPrint(allocator, "{}d ago", .{@divFloor(delta, 86400)});
}

const DateParts = struct { year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32 };

fn parseTsParts(ts: []const u8) ?DateParts {
    if (ts.len < 19) return null;
    return .{
        .year = std.fmt.parseInt(i32, ts[0..4], 10) catch return null,
        .month = std.fmt.parseInt(u32, ts[5..7], 10) catch return null,
        .day = std.fmt.parseInt(u32, ts[8..10], 10) catch return null,
        .hour = std.fmt.parseInt(u32, ts[11..13], 10) catch return null,
        .min = std.fmt.parseInt(u32, ts[14..16], 10) catch return null,
        .sec = std.fmt.parseInt(u32, ts[17..19], 10) catch return null,
    };
}

fn civilFromUnix(secs: i64) DateParts {
    const days = @divFloor(secs, 86400);
    const rem = @mod(secs, 86400);
    const ymd = civilFromDays(days);
    return .{
        .year = ymd.year,
        .month = ymd.month,
        .day = ymd.day,
        .hour = @intCast(@divFloor(rem, 3600)),
        .min = @intCast(@divFloor(@mod(rem, 3600), 60)),
        .sec = @intCast(@mod(rem, 60)),
    };
}

fn civilFromDays(days: i64) struct { year: i32, month: u32, day: u32 } {
    const z = days + 719468;
    const era = @divFloor(if (z >= 0) z else z - 146096, 146097);
    const doe = z - era * 146097;
    const yoe = @divFloor(doe - @divFloor(doe, 1460) + @divFloor(doe, 36524) - @divFloor(doe, 146096), 365);
    const y = yoe + era * 400;
    const doy = doe - (365 * yoe + @divFloor(yoe, 4) - @divFloor(yoe, 100));
    const mp = @divFloor(5 * doy + 2, 153);
    const d = doy - @divFloor(153 * mp + 2, 5) + 1;
    const m = mp + if (mp < 10) @as(i64, 3) else @as(i64, -9);
    return .{ .year = @intCast(y + if (m <= 2) @as(i64, 1) else @as(i64, 0)), .month = @intCast(m), .day = @intCast(d) };
}

fn unixFromParts(p: DateParts) i64 {
    var y: i64 = p.year;
    const m: i64 = p.month;
    y -= if (m <= 2) 1 else 0;
    const era = @divFloor(if (y >= 0) y else y - 399, 400);
    const yoe = y - era * 400;
    const doy = @divFloor(153 * (m + if (m > 2) @as(i64, -3) else @as(i64, 9)) + 2, 5) + p.day - 1;
    const doe = yoe * 365 + @divFloor(yoe, 4) - @divFloor(yoe, 100) + doy;
    return (era * 146097 + doe - 719468) * 86400 + p.hour * 3600 + p.min * 60 + p.sec;
}

fn nowNs() i128 {
    var ts: std.c.timespec = undefined;
    if (std.c.clock_gettime(.REALTIME, &ts) != 0) return 0;
    return @as(i128, ts.sec) * std.time.ns_per_s + ts.nsec;
}

fn previewSnippet(allocator: Allocator, preview: []const u8, chars: usize) ![]u8 {
    if (chars == 0) return allocator.dupe(u8, "");
    var view = try std.unicode.Utf8View.init(preview);
    var it = view.iterator();
    var out: std.ArrayList(u8) = .empty;
    var count: usize = 0;
    while (count < chars) : (count += 1) {
        const cp = it.nextCodepoint() orelse return out.toOwnedSlice(allocator);
        var tmp: [4]u8 = undefined;
        const n = try std.unicode.utf8Encode(cp, &tmp);
        try out.appendSlice(allocator, tmp[0..n]);
    }
    if (it.nextCodepoint() != null and chars > 3) try out.appendSlice(allocator, "...");
    return out.toOwnedSlice(allocator);
}

fn humanSize(allocator: Allocator, n: i64) ![]u8 {
    if (n < 1024) return std.fmt.allocPrint(allocator, "{}B", .{n});
    if (n < 1024 * 1024) return std.fmt.allocPrint(allocator, "{d:.1}K", .{@as(f64, @floatFromInt(n)) / 1024.0});
    if (n < 1024 * 1024 * 1024) return std.fmt.allocPrint(allocator, "{d:.1}M", .{@as(f64, @floatFromInt(n)) / (1024.0 * 1024.0)});
    return std.fmt.allocPrint(allocator, "{d:.1}G", .{@as(f64, @floatFromInt(n)) / (1024.0 * 1024.0 * 1024.0)});
}

pub fn printEscapedDisplay(writer: anytype, value: []const u8) !void {
    for (value) |b| switch (b) {
        '\\' => try writer.writeAll("\\\\"),
        '\n' => try writer.writeAll("\\n"),
        '\r' => try writer.writeAll("\\r"),
        '\t' => try writer.writeAll("\\t"),
        else => try writer.writeByte(b),
    };
}

pub fn printJsonString(writer: anytype, value: []const u8) !void {
    try writer.writeByte('"');
    for (value) |b| switch (b) {
        '"' => try writer.writeAll("\\\""),
        '\\' => try writer.writeAll("\\\\"),
        '\n' => try writer.writeAll("\\n"),
        '\r' => try writer.writeAll("\\r"),
        '\t' => try writer.writeAll("\\t"),
        0...8, 11, 12, 14...31 => try writer.print("\\u{x:0>4}", .{b}),
        else => try writer.writeByte(b),
    };
    try writer.writeByte('"');
}
