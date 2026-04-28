const std = @import("std");
const runtime = @import("runtime.zig");
const types = @import("types.zig");

const Allocator = std.mem.Allocator;
const Attr = types.Attr;
const Meta = types.Meta;
const Paths = types.Paths;

pub fn basePaths(allocator: Allocator) !Paths {
    const base = if (runtime.envOwned(allocator, "STASH_DIR")) |dir|
        dir
    else blk: {
        const home = runtime.envOwned(allocator, "HOME") orelse return error.InvalidArgument;
        break :blk try std.fs.path.join(allocator, &.{ home, ".stash" });
    };
    return .{
        .base = base,
        .data = try std.fs.path.join(allocator, &.{ base, "data" }),
        .attr = try std.fs.path.join(allocator, &.{ base, "attr" }),
        .tmp = try std.fs.path.join(allocator, &.{ base, "tmp" }),
        .cache = try std.fs.path.join(allocator, &.{ base, "cache" }),
    };
}

fn initStore(allocator: Allocator) !Paths {
    const p = try basePaths(allocator);
    try runtime.cwd().createDirPath(runtime.process_io, p.data);
    try runtime.cwd().createDirPath(runtime.process_io, p.attr);
    try runtime.cwd().createDirPath(runtime.process_io, p.tmp);
    try runtime.cwd().createDirPath(runtime.process_io, p.cache);
    return p;
}

fn invalidateCache(allocator: Allocator) void {
    const p = basePaths(allocator) catch return;
    const cache_path = std.fs.path.join(allocator, &.{ p.cache, "list.cache" }) catch return;
    runtime.cwd().deleteFile(runtime.process_io, cache_path) catch {};
}

pub fn dataPath(allocator: Allocator, id: []const u8) ![]u8 {
    const p = try basePaths(allocator);
    return std.fs.path.join(allocator, &.{ p.data, id });
}

pub fn attrPath(allocator: Allocator, id: []const u8) ![]u8 {
    const p = try basePaths(allocator);
    return std.fs.path.join(allocator, &.{ p.attr, id });
}

pub fn activePocket(allocator: Allocator) ?[]u8 {
    const value = runtime.envOwned(allocator, types.pocket_env) orelse return null;
    const trimmed = std.mem.trim(u8, value, " \t\r\n");
    if (trimmed.len == 0) return null;
    return allocator.dupe(u8, trimmed) catch null;
}

fn visible(meta: *const Meta, allocator: Allocator) bool {
    if (activePocket(allocator)) |pocket| {
        return if (meta.attr(types.pocket_attr)) |value| std.mem.eql(u8, value, pocket) else false;
    }
    return true;
}

pub fn pushInput(allocator: Allocator, file_arg: ?[]const u8, attrs: []const Attr, tee_mode: bool) ![]u8 {
    const p = try initStore(allocator);
    const id = try newUlid(allocator);
    const tmp_data = try std.fs.path.join(allocator, &.{ p.tmp, try std.fmt.allocPrint(allocator, "{s}.data", .{id}) });
    var out_file = try runtime.cwd().createFile(runtime.process_io, tmp_data, .{});

    var sample: std.ArrayList(u8) = .empty;
    var total: i64 = 0;
    var buf: [65536]u8 = undefined;
    const stdout = runtime.stdoutWriter();

    if (file_arg) |path| {
        var in_file = try runtime.cwd().openFile(runtime.process_io, path, .{});
        defer in_file.close(runtime.process_io);
        while (true) {
            const n = try runtime.fileRead(in_file, &buf);
            if (n == 0) break;
            if (sample.items.len < 512) {
                const need = @min(512 - sample.items.len, n);
                try sample.appendSlice(allocator, buf[0..need]);
            }
            try out_file.writeStreamingAll(runtime.process_io, buf[0..n]);
            total += @intCast(n);
        }
    } else {
        while (true) {
            const n_raw = std.c.read(std.posix.STDIN_FILENO, &buf, buf.len);
            if (n_raw < 0) return error.ReadFailed;
            const n: usize = @intCast(n_raw);
            if (n == 0) break;
            if (sample.items.len < 512) {
                const need = @min(512 - sample.items.len, n);
                try sample.appendSlice(allocator, buf[0..need]);
            }
            try out_file.writeStreamingAll(runtime.process_io, buf[0..n]);
            if (tee_mode) stdout.writeAll(buf[0..n]) catch |err| {
                if (err == error.BrokenPipe) break;
                return err;
            };
            total += @intCast(n);
        }
    }
    out_file.close(runtime.process_io);

    var meta = Meta.init();
    meta.id = id;
    meta.ts = try nowString(allocator);
    meta.size = total;
    meta.preview = try buildPreview(allocator, sample.items, 128);
    for (attrs) |item| try meta.setAttr(allocator, item.key, item.value);

    const tmp_attr = try std.fs.path.join(allocator, &.{ p.tmp, try std.fmt.allocPrint(allocator, "{s}.attr", .{id}) });
    try writeMetaFile(allocator, tmp_attr, &meta);
    try runtime.cwd().rename(tmp_data, runtime.cwd(), try dataPath(allocator, id), runtime.process_io);
    try runtime.cwd().rename(tmp_attr, runtime.cwd(), try attrPath(allocator, id), runtime.process_io);
    invalidateCache(allocator);
    return id;
}

pub fn catId(allocator: Allocator, id: []const u8, writer: anytype) !void {
    var file = try runtime.cwd().openFile(runtime.process_io, try dataPath(allocator, id), .{});
    defer file.close(runtime.process_io);
    var buf: [65536]u8 = undefined;
    while (true) {
        const n = try runtime.fileRead(file, &buf);
        if (n == 0) break;
        try writer.writeAll(buf[0..n]);
    }
}

pub fn removeId(allocator: Allocator, id: []const u8) !void {
    runtime.cwd().deleteFile(runtime.process_io, try dataPath(allocator, id)) catch |err| if (err != error.FileNotFound) return err;
    runtime.cwd().deleteFile(runtime.process_io, try attrPath(allocator, id)) catch |err| if (err != error.FileNotFound) return err;
    invalidateCache(allocator);
}

pub fn visibleList(allocator: Allocator) !std.ArrayList(Meta) {
    var out: std.ArrayList(Meta) = .empty;
    const ids = try listEntryIds(allocator);
    for (ids.items) |id| {
        var meta = getMeta(allocator, id) catch continue;
        if (visible(&meta, allocator)) try out.append(allocator, meta);
    }
    return out;
}

fn listEntryIds(allocator: Allocator) !std.ArrayList([]u8) {
    var out: std.ArrayList([]u8) = .empty;
    const p = try basePaths(allocator);
    var dir = runtime.cwd().openDir(runtime.process_io, p.attr, .{ .iterate = true }) catch |err| {
        if (err == error.FileNotFound) return out;
        return err;
    };
    defer dir.close(runtime.process_io);
    var it = dir.iterate();
    while (try it.next(runtime.process_io)) |entry| {
        if (entry.kind == .file) try out.append(allocator, try allocator.dupe(u8, entry.name));
    }
    std.mem.sort([]u8, out.items, {}, descSlices);
    return out;
}

pub fn resolve(allocator: Allocator, input: []const u8) ![]u8 {
    const raw = std.mem.trim(u8, input, " \t\r\n");
    if (raw.len == 0) {
        const items = try visibleList(allocator);
        if (items.items.len == 0) return error.StashEmpty;
        return items.items[0].id;
    }
    if (raw[0] == '@') {
        const n = try std.fmt.parseInt(usize, raw[1..], 10);
        return nthNewest(allocator, n);
    }
    if (allDigits(raw)) {
        const n = try std.fmt.parseInt(usize, raw, 10);
        return nthNewest(allocator, n);
    }
    const lower = try asciiLower(allocator, raw);
    if (lower.len < types.min_id_len) return error.IdTooShort;
    const items = try visibleList(allocator);
    var prefix: ?[]u8 = null;
    var suffix: ?[]u8 = null;
    var prefix_ambig = false;
    var suffix_ambig = false;
    for (items.items) |item| {
        if (std.mem.eql(u8, item.id, lower)) return item.id;
        if (std.mem.startsWith(u8, item.id, lower)) {
            if (prefix != null) prefix_ambig = true else prefix = item.id;
        }
        if (std.mem.endsWith(u8, item.id, lower)) {
            if (suffix != null) suffix_ambig = true else suffix = item.id;
        }
    }
    if (prefix) |id| {
        if (prefix_ambig) return error.AmbiguousId;
        return id;
    }
    if (suffix) |id| {
        if (suffix_ambig) return error.AmbiguousId;
        return id;
    }
    return error.NotFound;
}

fn nthNewest(allocator: Allocator, n: usize) ![]u8 {
    if (n == 0) return error.InvalidRef;
    const items = try visibleList(allocator);
    if (n > items.items.len) return error.NotFound;
    return items.items[n - 1].id;
}

pub fn getMeta(allocator: Allocator, id: []const u8) !Meta {
    const path = try attrPath(allocator, id);
    const data = try runtime.cwd().readFileAlloc(runtime.process_io, path, allocator, .limited(16 * 1024 * 1024));
    return parseAttrFile(allocator, data);
}

pub fn writeMeta(allocator: Allocator, id: []const u8, meta: *const Meta) !void {
    try writeMetaFile(allocator, try attrPath(allocator, id), meta);
    invalidateCache(allocator);
}

fn writeMetaFile(allocator: Allocator, path: []const u8, meta: *const Meta) !void {
    var file = try runtime.cwd().createFile(runtime.process_io, path, .{});
    defer file.close(runtime.process_io);
    var writer = runtime.FileWriter{ .file = file };
    try writeAttrLine(allocator, writer, "id", meta.id);
    try writeAttrLine(allocator, writer, "ts", meta.ts);
    try writer.print("size={}\n", .{meta.size});
    if (std.mem.trim(u8, meta.preview, " \t\r\n").len > 0) try writeAttrLine(allocator, writer, "preview", meta.preview);
    for (meta.attrs.items) |item| try writeAttrLine(allocator, writer, item.key, item.value);
}

fn writeAttrLine(allocator: Allocator, writer: anytype, key: []const u8, value: []const u8) !void {
    _ = allocator;
    try writeEscapedAttr(writer, key);
    try writer.writeByte('=');
    try writeEscapedAttr(writer, value);
    try writer.writeByte('\n');
}

fn parseAttrFile(allocator: Allocator, input: []const u8) !Meta {
    var meta = Meta.init();
    var lines = std.mem.splitScalar(u8, input, '\n');
    while (lines.next()) |raw_line| {
        const line = std.mem.trim(u8, raw_line, " \t\r\n");
        if (line.len == 0) continue;
        const pos = splitAttrLine(line) orelse return error.InvalidAttr;
        const key = try unescapeAttr(allocator, line[0..pos]);
        const value = try unescapeAttr(allocator, line[pos + 1 ..]);
        if (std.mem.eql(u8, key, "id")) meta.id = value else if (std.mem.eql(u8, key, "ts")) meta.ts = value else if (std.mem.eql(u8, key, "size")) meta.size = try std.fmt.parseInt(i64, value, 10) else if (std.mem.eql(u8, key, "preview")) meta.preview = value else try meta.setAttr(allocator, key, value);
    }
    return meta;
}

fn splitAttrLine(line: []const u8) ?usize {
    var escaped = false;
    for (line, 0..) |b, idx| {
        if (b == '\\') escaped = !escaped else if (b == '=' and !escaped) return idx else escaped = false;
    }
    return null;
}

fn writeEscapedAttr(writer: anytype, value: []const u8) !void {
    for (value) |b| switch (b) {
        '\\' => try writer.writeAll("\\\\"),
        '\n' => try writer.writeAll("\\n"),
        '\r' => try writer.writeAll("\\r"),
        '\t' => try writer.writeAll("\\t"),
        '=' => try writer.writeAll("\\="),
        else => try writer.writeByte(b),
    };
}

fn unescapeAttr(allocator: Allocator, input: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    var i: usize = 0;
    while (i < input.len) : (i += 1) {
        if (input[i] != '\\') {
            try out.append(allocator, input[i]);
            continue;
        }
        i += 1;
        if (i >= input.len) return error.InvalidAttr;
        try out.append(allocator, switch (input[i]) {
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '=' => '=',
            else => return error.InvalidAttr,
        });
    }
    return out.toOwnedSlice(allocator);
}

fn nowNs() i128 {
    var ts: std.c.timespec = undefined;
    if (std.c.clock_gettime(.REALTIME, &ts) != 0) return 0;
    return @as(i128, ts.sec) * std.time.ns_per_s + ts.nsec;
}

fn newUlid(allocator: Allocator) ![]u8 {
    var bytes: [16]u8 = undefined;
    const millis: u64 = @intCast(@divFloor(nowNs(), std.time.ns_per_ms));
    for (0..6) |i| bytes[i] = @intCast((millis >> @intCast(8 * (5 - i))) & 0xff);
    runtime.process_io.randomSecure(bytes[6..]) catch runtime.process_io.random(bytes[6..]);
    var value: u128 = 0;
    for (bytes) |b| value = (value << 8) | b;
    const alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    var out = try allocator.alloc(u8, 26);
    var i: usize = 26;
    while (i > 0) {
        i -= 1;
        out[i] = std.ascii.toLower(alphabet[@intCast(value & 0x1f)]);
        value >>= 5;
    }
    return out;
}

fn nowString(allocator: Allocator) ![]u8 {
    const ns = nowNs();
    const secs = @divFloor(ns, std.time.ns_per_s);
    const nanos = @mod(ns, std.time.ns_per_s);
    const dt = civilFromUnix(@intCast(secs));
    return std.fmt.allocPrint(allocator, "{d:0>4}-{d:0>2}-{d:0>2}T{d:0>2}:{d:0>2}:{d:0>2}.{d:0>9}Z", .{
        @as(u32, @intCast(dt.year)),
        dt.month,
        dt.day,
        dt.hour,
        dt.min,
        dt.sec,
        @as(u32, @intCast(nanos)),
    });
}

const DateParts = struct { year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32 };

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

fn buildPreview(allocator: Allocator, buf: []const u8, limit: usize) ![]u8 {
    var decoded = std.unicode.Utf8View.init(buf) catch {
        var out: std.ArrayList(u8) = .empty;
        var count: usize = 0;
        var last_space = false;
        for (buf) |b| {
            if (count >= limit) break;
            const ch: u8 = switch (b) {
                '\n', '\r', '\t' => ' ',
                0...8, 11, 12, 14...31, 127...255 => '.',
                else => b,
            };
            if (last_space and ch == ' ') continue;
            try out.append(allocator, ch);
            last_space = ch == ' ';
            count += 1;
        }
        return allocator.dupe(u8, std.mem.trim(u8, try out.toOwnedSlice(allocator), " \t\r\n"));
    };
    var it = decoded.iterator();
    var out: std.ArrayList(u8) = .empty;
    var count: usize = 0;
    var last_space = false;
    while (it.nextCodepoint()) |cp| {
        if (count >= limit) break;
        if (cp == '\n' or cp == '\r' or cp == '\t') {
            if (!last_space) try out.append(allocator, ' ');
            last_space = true;
        } else if (cp < 32 or cp == 127) {
            try out.append(allocator, '.');
            last_space = false;
        } else {
            var tmp: [4]u8 = undefined;
            const n = try std.unicode.utf8Encode(cp, &tmp);
            try out.appendSlice(allocator, tmp[0..n]);
            last_space = false;
        }
        count += 1;
    }
    return allocator.dupe(u8, std.mem.trim(u8, try out.toOwnedSlice(allocator), " \t\r\n"));
}

fn asciiLower(allocator: Allocator, value: []const u8) ![]u8 {
    const out = try allocator.dupe(u8, value);
    for (out) |*b| b.* = std.ascii.toLower(b.*);
    return out;
}

fn allDigits(value: []const u8) bool {
    if (value.len == 0) return false;
    for (value) |b| if (!std.ascii.isDigit(b)) return false;
    return true;
}

fn descSlices(_: void, a: []u8, b: []u8) bool {
    return std.mem.lessThan(u8, b, a);
}
