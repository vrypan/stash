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
