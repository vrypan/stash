const std = @import("std");
const cmd = @import("cmd.zig");
const runtime = @import("runtime.zig");

pub fn main(init: std.process.Init) !void {
    const allocator = init.arena.allocator();
    runtime.process_env = init.minimal.environ;
    runtime.process_io = init.io;

    const args = try init.minimal.args.toSlice(allocator);
    const code = cmd.run(allocator, args) catch |err| {
        const stderr = runtime.stderrWriter();
        try stderr.print("error: {s}\n", .{cmd.errorMessage(err)});
        return std.process.exit(1);
    };
    if (code != 0) std.process.exit(code);
}
