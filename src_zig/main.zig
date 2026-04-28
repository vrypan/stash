const std = @import("std");
const app = @import("app.zig");

pub fn main(init: std.process.Init) !void {
    try app.main(init);
}
