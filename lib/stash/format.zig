const std = @import("std");

pub fn dateOnly(timestamp: []const u8) []const u8 {
    if (std.mem.indexOfScalar(u8, timestamp, 'T')) |pos| return timestamp[0..pos];
    return timestamp;
}

pub fn indexOfIgnoreCaseAscii(haystack: []const u8, needle: []const u8) ?usize {
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

const runtime = @import("runtime.zig");

pub const DateParts = struct { year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32 };

pub fn nowNs() i128 {
    return std.Io.Clock.real.now(runtime.process_io).toNanoseconds();
}

pub fn civilFromDays(days: i64) struct { year: i32, month: u32, day: u32 } {
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

pub fn civilFromUnix(secs: i64) DateParts {
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

const testing = std.testing;

test "civilFromDays converts the Unix epoch correctly" {
    const parts = civilFromDays(0);
    try testing.expectEqual(@as(i32, 1970), parts.year);
    try testing.expectEqual(@as(u32, 1), parts.month);
    try testing.expectEqual(@as(u32, 1), parts.day);
}

test "civilFromUnix converts a known timestamp" {
    // 2024-01-15T12:10:45Z
    const parts = civilFromUnix(1705320645);
    try testing.expectEqual(@as(i32, 2024), parts.year);
    try testing.expectEqual(@as(u32, 1), parts.month);
    try testing.expectEqual(@as(u32, 15), parts.day);
    try testing.expectEqual(@as(u32, 12), parts.hour);
    try testing.expectEqual(@as(u32, 10), parts.min);
    try testing.expectEqual(@as(u32, 45), parts.sec);
}

test "civilFromDays handles a leap day" {
    // 2024-02-29 is 19782 days after the epoch.
    const parts = civilFromDays(19782);
    try testing.expectEqual(@as(i32, 2024), parts.year);
    try testing.expectEqual(@as(u32, 2), parts.month);
    try testing.expectEqual(@as(u32, 29), parts.day);
}
