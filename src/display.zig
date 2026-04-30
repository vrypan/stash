const std = @import("std");
const types = @import("types.zig");
const runtime = @import("runtime.zig");

const Allocator = std.mem.Allocator;
const Meta = types.Meta;
const MetaSelection = types.MetaSelection;
const IdMode = types.IdMode;
const DateMode = types.DateMode;
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
    show_size: bool,
    show_bytes: bool,
    attrs_mode: AttrsMode,
    show_name: bool,
    show_preview: bool,
    headers: bool,
    width: usize,
    color: bool,
    selection: *const MetaSelection,
) !void {
    const has_date = date_mode != null;
    const show_count = attrs_mode == .count;
    const show_flag = attrs_mode == .flag;
    const has_display_tags = selection.display_tags.items.len > 0;
    const show_all_meta = selection.show_all;
    const line_width = resolveLineWidth(width);

    const simple_ids_only = !has_date and !show_size and !show_bytes and !show_name and !show_preview and
        !show_count and !show_flag and !show_all_meta and !has_display_tags;
    if (simple_ids_only and !headers) {
        for (items, 0..) |*meta, idx| {
            var line: std.ArrayList(u8) = .empty;
            try appendColorized(allocator, &line, try displayIdAlloc(allocator, meta, idx, id_mode), "1;33", color);
            try writeLine(out, allocator, line.items, line_width);
        }
        return;
    }

    const LsRow = struct {
        id: []u8,
        size: []u8,
        bytes: []u8,
        date: []u8,
        name: []u8,
        attr_count: []u8,
        attr_flag: []const u8,
        meta_vals: std.ArrayList([]u8),
        meta_inline: []u8,
        preview: []u8,
    };

    var rows: std.ArrayList(LsRow) = .empty;

    var max_id: usize = 0;
    var max_size: usize = 0;
    var max_bytes: usize = 0;
    var max_date: usize = 0;
    var max_name: usize = 0;
    var max_attr_count: usize = 0;
    var max_attr_flag: usize = 0;
    var max_inline_meta: usize = 0;
    var meta_widths: std.ArrayList(usize) = .empty;
    for (selection.display_tags.items) |_| try meta_widths.append(allocator, 0);

    for (items, 0..) |*meta, idx| {
        const id = try displayIdAlloc(allocator, meta, idx, id_mode);
        max_id = @max(max_id, visibleLen(id));

        const size_val = if (show_size) blk: {
            const s = try humanSize(allocator, meta.size);
            max_size = @max(max_size, visibleLen(s));
            break :blk s;
        } else try allocator.dupe(u8, "");

        const bytes_val = if (show_bytes) blk: {
            const s = try std.fmt.allocPrint(allocator, "{}", .{meta.size});
            max_bytes = @max(max_bytes, visibleLen(s));
            break :blk s;
        } else try allocator.dupe(u8, "");

        const date_val = if (date_mode) |mode| blk: {
            const s = try formatDate(allocator, meta.ts, mode);
            max_date = @max(max_date, visibleLen(s));
            break :blk s;
        } else try allocator.dupe(u8, "");

        const name_val = if (show_name) blk: {
            const s = try allocator.dupe(u8, meta.attr("filename") orelse id);
            max_name = @max(max_name, visibleLen(s));
            break :blk s;
        } else try allocator.dupe(u8, "");

        const attr_count_val = if (show_count) blk: {
            const s = try std.fmt.allocPrint(allocator, "{}", .{meta.attrs.items.len});
            max_attr_count = @max(max_attr_count, visibleLen(s));
            break :blk s;
        } else try allocator.dupe(u8, "");

        const attr_flag_val: []const u8 = if (show_flag and meta.attrs.items.len > 0) blk: {
            max_attr_flag = @max(max_attr_flag, 1);
            break :blk "*";
        } else "";

        var meta_vals: std.ArrayList([]u8) = .empty;
        for (selection.display_tags.items, 0..) |tag, tag_idx| {
            const s = if (meta.attr(tag)) |value| try escapedDisplayAlloc(allocator, value) else try allocator.dupe(u8, " ");
            meta_widths.items[tag_idx] = @max(meta_widths.items[tag_idx], visibleLen(s));
            try meta_vals.append(allocator, s);
        }

        var inline_meta = try allocator.dupe(u8, "");
        if (show_all_meta and meta.attrs.items.len > 0) {
            var buf: std.ArrayList(u8) = .empty;
            for (meta.attrs.items, 0..) |attr, n| {
                if (n > 0) try buf.appendSlice(allocator, "  ");
                try appendEscapedDisplay(allocator, &buf, attr.value);
            }
            inline_meta = try buf.toOwnedSlice(allocator);
            max_inline_meta = @max(max_inline_meta, visibleLen(inline_meta));
        }

        const preview_val = if (show_preview and meta.preview.len > 0)
            try allocator.dupe(u8, meta.preview)
        else
            try allocator.dupe(u8, "");

        try rows.append(allocator, .{
            .id = id,
            .size = size_val,
            .bytes = bytes_val,
            .date = date_val,
            .name = name_val,
            .attr_count = attr_count_val,
            .attr_flag = attr_flag_val,
            .meta_vals = meta_vals,
            .meta_inline = inline_meta,
            .preview = preview_val,
        });
    }

    const header_id = "id";
    const header_size = "size";
    const header_bytes = "bytes";
    const header_date = "date";
    const header_name = "name";
    const header_attrs = "attrs";
    const header_preview = "preview";
    if (headers) {
        max_id = @max(max_id, header_id.len);
        if (show_size) max_size = @max(max_size, header_size.len);
        if (show_bytes) max_bytes = @max(max_bytes, header_bytes.len);
        if (has_date) max_date = @max(max_date, header_date.len);
        if (show_name) max_name = @max(max_name, header_name.len);
        if (show_count) max_attr_count = @max(max_attr_count, header_attrs.len);
        if (show_flag) max_attr_flag = @max(max_attr_flag, header_attrs.len);
        if (show_all_meta) max_inline_meta = @max(max_inline_meta, header_attrs.len);
        for (selection.display_tags.items, 0..) |tag, tag_idx| {
            meta_widths.items[tag_idx] = @max(meta_widths.items[tag_idx], visibleLen(tag));
        }
    }

    var line: std.ArrayList(u8) = .empty;
    if (headers) {
        try appendStyledRight(allocator, &line, header_id, max_id, "1", color);
        if (show_size) {
            try line.appendSlice(allocator, "  ");
            try appendStyledRight(allocator, &line, header_size, max_size, "1", color);
        }
        if (show_bytes) {
            try line.appendSlice(allocator, "  ");
            try appendStyledRight(allocator, &line, header_bytes, max_bytes, "1", color);
        }
        if (has_date) {
            try line.appendSlice(allocator, "  ");
            try appendStyledRight(allocator, &line, header_date, max_date, "1", color);
        }
        if (show_count) {
            try line.appendSlice(allocator, "  ");
            try appendStyledRight(allocator, &line, header_attrs, max_attr_count, "1", color);
        }
        if (show_flag) {
            try line.appendSlice(allocator, "  ");
            try appendStyledRight(allocator, &line, header_attrs, max_attr_flag, "1", color);
        }
        if (show_name) {
            try line.appendSlice(allocator, "  ");
            try appendStyledRight(allocator, &line, header_name, max_name, "1", color);
        }
        for (selection.display_tags.items, 0..) |tag, tag_idx| {
            try line.appendSlice(allocator, "  ");
            try appendStyledRight(allocator, &line, tag, meta_widths.items[tag_idx], "1", color);
        }
        if (show_all_meta) {
            try line.appendSlice(allocator, "  ");
            try appendStyledRight(allocator, &line, header_attrs, max_inline_meta, "1", color);
        }
        if (show_preview) {
            try line.appendSlice(allocator, "  ");
            try appendColorized(allocator, &line, header_preview, "1", color);
        }
        try writeLine(out, allocator, line.items, line_width);
        line.clearRetainingCapacity();
    }

    for (rows.items) |*row| {
        try appendStyledRight(allocator, &line, row.id, max_id, "1;33", color);
        if (row.size.len > 0) {
            try line.appendSlice(allocator, "  ");
            try appendRawLeft(allocator, &line, row.size, max_size);
        }
        if (row.bytes.len > 0) {
            try line.appendSlice(allocator, "  ");
            try appendRawLeft(allocator, &line, row.bytes, max_bytes);
        }
        if (row.date.len > 0) {
            try line.appendSlice(allocator, "  ");
            try appendRawLeft(allocator, &line, row.date, max_date);
        }
        if (row.attr_count.len > 0) {
            try line.appendSlice(allocator, "  ");
            try appendStyledLeft(allocator, &line, row.attr_count, max_attr_count, "35", color);
        }
        if (max_attr_flag > 0) {
            try line.appendSlice(allocator, "  ");
            try appendStyledLeft(allocator, &line, row.attr_flag, max_attr_flag, "1;35", color);
        }
        if (row.name.len > 0) {
            try line.appendSlice(allocator, "  ");
            if (std.mem.eql(u8, row.name, row.id)) {
                try appendRawRight(allocator, &line, row.name, max_name);
            } else {
                try appendStyledRight(allocator, &line, row.name, max_name, "1;36", color);
            }
        }
        for (row.meta_vals.items, 0..) |value, tag_idx| {
            try line.appendSlice(allocator, "  ");
            try appendStyledRight(allocator, &line, value, meta_widths.items[tag_idx], "36", color);
        }
        if (max_inline_meta > 0) {
            try line.appendSlice(allocator, "  ");
            try appendStyledRight(allocator, &line, row.meta_inline, max_inline_meta, "36", color);
        }
        if (row.preview.len > 0) {
            try line.appendSlice(allocator, "  ");
            try line.appendSlice(allocator, row.preview);
        }
        try writeLine(out, allocator, line.items, line_width);
        line.clearRetainingCapacity();
    }
}

pub fn printLsJson(allocator: Allocator, out: anytype, items: []const Meta, date_mode: DateMode) !void {
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
            try printJsonString(out, meta.preview);
            try out.writeByte(']');
        }
        try out.writeAll("\n  }");
    }
    try out.writeAll("\n]\n");
}

pub fn printLsFormat(allocator: Allocator, out: anytype, items: []const Meta, format: []const u8, width: usize) !void {
    const line_width = resolveLineWidth(width);
    for (items, 0..) |*meta, idx| {
        var line: std.ArrayList(u8) = .empty;
        var i: usize = 0;
        while (i < format.len) : (i += 1) {
            const ch = format[i];
            if (ch == '\\') {
                i += 1;
                if (i >= format.len) return error.InvalidArgument;
                switch (format[i]) {
                    'n' => {
                        try writeLine(out, allocator, line.items, line_width);
                        line.clearRetainingCapacity();
                    },
                    'r' => try line.append(allocator, '\r'),
                    't' => try line.append(allocator, '\t'),
                    '\\' => try line.append(allocator, '\\'),
                    else => return error.InvalidArgument,
                }
                continue;
            }
            if (ch == '\n') {
                try writeLine(out, allocator, line.items, line_width);
                line.clearRetainingCapacity();
                continue;
            }
            if (ch != '%') {
                try line.append(allocator, ch);
                continue;
            }

            i += 1;
            if (i >= format.len) return error.InvalidArgument;
            switch (format[i]) {
                '%' => try line.append(allocator, '%'),
                'i' => try appendEscapedDisplay(allocator, &line, meta.shortId()),
                'I' => try appendEscapedDisplay(allocator, &line, meta.id),
                'n' => try appendFmt(allocator, &line, "{}", .{idx + 1}),
                'p' => try appendEscapedDisplay(allocator, &line, meta.preview),
                'a' => {
                    const key = try parseFormatAttrKey(format, &i);
                    try appendEscapedDisplay(allocator, &line, meta.attr(key) orelse "");
                },
                'A' => try appendFormatAttrs(allocator, &line, meta, format, &i),
                's' => try appendFormatSize(allocator, &line, meta, format, &i),
                'd' => try appendFormatDate(allocator, &line, meta, format, &i),
                else => return error.InvalidArgument,
            }
        }
        if (line.items.len > 0) try writeRawLine(out, allocator, line.items, line_width);
    }
}

fn appendFormatAttrs(allocator: Allocator, line: *std.ArrayList(u8), meta: *const Meta, format: []const u8, index: *usize) !void {
    if (index.* + 1 >= format.len) return error.InvalidArgument;
    index.* += 1;
    switch (format[index.*]) {
        'f' => {
            if (meta.attrs.items.len > 0) try line.append(allocator, '*');
        },
        'l' => {
            for (meta.attrs.items, 0..) |attr, n| {
                if (n > 0) try line.appendSlice(allocator, "  ");
                try appendEscapedDisplay(allocator, line, attr.value);
            }
        },
        'c' => try appendFmt(allocator, line, "{}", .{meta.attrs.items.len}),
        else => return error.InvalidArgument,
    }
}

fn appendFormatSize(allocator: Allocator, line: *std.ArrayList(u8), meta: *const Meta, format: []const u8, index: *usize) !void {
    if (index.* + 1 >= format.len) return error.InvalidArgument;
    index.* += 1;
    switch (format[index.*]) {
        'b' => try appendFmt(allocator, line, "{}", .{meta.size}),
        'h' => try appendEscapedDisplay(allocator, line, try humanSize(allocator, meta.size)),
        else => return error.InvalidArgument,
    }
}

fn appendFormatDate(allocator: Allocator, line: *std.ArrayList(u8), meta: *const Meta, format: []const u8, index: *usize) !void {
    if (index.* + 1 >= format.len) return error.InvalidArgument;
    index.* += 1;
    switch (format[index.*]) {
        't' => try appendEscapedDisplay(allocator, line, meta.ts),
        'h' => try appendEscapedDisplay(allocator, line, try formatDate(allocator, meta.ts, .ls)),
        'i' => try appendEscapedDisplay(allocator, line, try formatDate(allocator, meta.ts, .iso)),
        else => return error.InvalidArgument,
    }
}

fn parseFormatAttrKey(format: []const u8, index: *usize) ![]const u8 {
    if (index.* + 1 >= format.len or format[index.* + 1] != '{') return error.InvalidArgument;
    const start = index.* + 2;
    var end = start;
    while (end < format.len and format[end] != '}') end += 1;
    if (end >= format.len or end == start) return error.InvalidArgument;
    index.* = end;
    return format[start..end];
}

fn appendFmt(allocator: Allocator, line: *std.ArrayList(u8), comptime fmt: []const u8, args: anytype) !void {
    try line.appendSlice(allocator, try std.fmt.allocPrint(allocator, fmt, args));
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

fn displayIdAlloc(allocator: Allocator, meta: *const Meta, idx: usize, mode: IdMode) ![]u8 {
    return switch (mode) {
        .full => allocator.dupe(u8, meta.id),
        .short => allocator.dupe(u8, meta.shortId()),
        .pos => std.fmt.allocPrint(allocator, "{}", .{idx + 1}),
    };
}

fn stdoutIsTerminal() bool {
    return std.Io.File.stdout().isTty(runtime.process_io) catch false;
}

fn colorEnabled(value: bool) bool {
    return value and stdoutIsTerminal();
}

fn terminalWidth() ?usize {
    const is_tty = stdoutIsTerminal();
    if (is_tty) {
        var ws: std.posix.winsize = .{ .row = 0, .col = 0, .xpixel = 0, .ypixel = 0 };
        const rc = (runtime.process_io.operate(.{ .device_io_control = .{
            .file = .stdout(),
            .code = @intCast(std.posix.T.IOCGWINSZ),
            .arg = &ws,
        } }) catch null);
        if (rc) |result| {
            if (result.device_io_control == 0 and ws.col > 0) return ws.col;
        }
    }
    if (std.process.Environ.getPosix(runtime.process_env, "COLUMNS")) |value| {
        const columns = std.fmt.parseInt(usize, value, 10) catch 0;
        if (columns > 0) return columns;
    }
    if (is_tty) return 80;
    return null;
}

fn resolveLineWidth(width: usize) ?usize {
    if (width > 0) return width;
    return terminalWidth();
}

fn visibleLen(value: []const u8) usize {
    var view = std.unicode.Utf8View.init(value) catch return value.len;
    var it = view.iterator();
    var count: usize = 0;
    while (it.nextCodepoint() != null) count += 1;
    return count;
}

fn appendSpaces(allocator: Allocator, line: *std.ArrayList(u8), count: usize) !void {
    const old_len = line.items.len;
    try line.ensureUnusedCapacity(allocator, count);
    line.items.len += count;
    @memset(line.items[old_len..], ' ');
}

fn appendRawRight(allocator: Allocator, line: *std.ArrayList(u8), value: []const u8, width: usize) !void {
    try line.appendSlice(allocator, value);
    const len = visibleLen(value);
    if (len < width) try appendSpaces(allocator, line, width - len);
}

fn appendRawLeft(allocator: Allocator, line: *std.ArrayList(u8), value: []const u8, width: usize) !void {
    const len = visibleLen(value);
    if (len < width) try appendSpaces(allocator, line, width - len);
    try line.appendSlice(allocator, value);
}

fn appendColorStart(allocator: Allocator, line: *std.ArrayList(u8), code: []const u8, enabled: bool) !void {
    if (!enabled) return;
    try line.appendSlice(allocator, "\x1b[");
    try line.appendSlice(allocator, code);
    try line.appendSlice(allocator, "m");
}

fn appendColorEnd(allocator: Allocator, line: *std.ArrayList(u8), enabled: bool) !void {
    if (enabled) try line.appendSlice(allocator, "\x1b[0m");
}

fn appendColorized(allocator: Allocator, line: *std.ArrayList(u8), value: []const u8, code: []const u8, color: bool) !void {
    const enabled = colorEnabled(color) and value.len > 0;
    try appendColorStart(allocator, line, code, enabled);
    try line.appendSlice(allocator, value);
    try appendColorEnd(allocator, line, enabled);
}

fn appendStyledRight(allocator: Allocator, line: *std.ArrayList(u8), value: []const u8, width: usize, code: []const u8, color: bool) !void {
    const enabled = colorEnabled(color) and value.len > 0;
    try appendColorStart(allocator, line, code, enabled);
    try appendRawRight(allocator, line, value, width);
    try appendColorEnd(allocator, line, enabled);
}

fn appendStyledLeft(allocator: Allocator, line: *std.ArrayList(u8), value: []const u8, width: usize, code: []const u8, color: bool) !void {
    const enabled = colorEnabled(color) and value.len > 0;
    try appendColorStart(allocator, line, code, enabled);
    try appendRawLeft(allocator, line, value, width);
    try appendColorEnd(allocator, line, enabled);
}

fn writeLine(writer: anytype, allocator: Allocator, line: []const u8, width: ?usize) !void {
    try writeRawLine(writer, allocator, line, width);
    try writer.writeByte('\n');
}

fn writeRawLine(writer: anytype, allocator: Allocator, line: []const u8, width: ?usize) !void {
    if (width) |w| {
        const trimmed = try trimAnsiToWidth(allocator, line, w);
        try writer.writeAll(trimmed);
    } else {
        try writer.writeAll(line);
    }
}

fn trimAnsiToWidth(allocator: Allocator, value: []const u8, width: usize) ![]u8 {
    if (width == 0) return allocator.dupe(u8, "");
    var out: std.ArrayList(u8) = .empty;
    var visible: usize = 0;
    var i: usize = 0;
    var saw_ansi = false;
    while (i < value.len) {
        if (value[i] == 0x1b and i + 1 < value.len and value[i + 1] == '[') {
            var end = i + 2;
            while (end < value.len and !(value[end] >= 0x40 and value[end] <= 0x7e)) end += 1;
            if (end < value.len) end += 1;
            try out.appendSlice(allocator, value[i..end]);
            saw_ansi = true;
            i = end;
            continue;
        }
        if (visible >= width) break;
        const len = std.unicode.utf8ByteSequenceLength(value[i]) catch 1;
        const end = @min(i + len, value.len);
        try out.appendSlice(allocator, value[i..end]);
        visible += 1;
        i = end;
    }
    if (visible >= width and saw_ansi) try out.appendSlice(allocator, "\x1b[0m");
    return out.toOwnedSlice(allocator);
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
    return std.Io.Clock.real.now(runtime.process_io).toNanoseconds();
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

fn appendEscapedDisplay(allocator: Allocator, out: *std.ArrayList(u8), value: []const u8) !void {
    for (value) |b| switch (b) {
        '\\' => try out.appendSlice(allocator, "\\\\"),
        '\n' => try out.appendSlice(allocator, "\\n"),
        '\r' => try out.appendSlice(allocator, "\\r"),
        '\t' => try out.appendSlice(allocator, "\\t"),
        else => try out.append(allocator, b),
    };
}

fn escapedDisplayAlloc(allocator: Allocator, value: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    try appendEscapedDisplay(allocator, &out, value);
    return out.toOwnedSlice(allocator);
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
