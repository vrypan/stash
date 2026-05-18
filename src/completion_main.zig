const std = @import("std");
const completion = @import("completion.zig");
const runtime = @import("runtime.zig");

pub fn main(init: std.process.Init) !void {
    const allocator = init.arena.allocator();
    runtime.process_io = init.io;

    const args = try init.minimal.args.toSlice(allocator);
    const stdout = runtime.stdoutWriter();
    const stderr = runtime.stderrWriter();

    if (args.len < 2) {
        try stderr.writeAll("usage: stash-completion <bash|zsh|fish>\n");
        std.process.exit(1);
    }

    const shell = args[1];
    const result = if (std.mem.eql(u8, shell, "bash"))
        completion.generateBash(stdout)
    else if (std.mem.eql(u8, shell, "zsh"))
        completion.generateZsh(stdout)
    else if (std.mem.eql(u8, shell, "fish"))
        completion.generateFish(stdout)
    else {
        try stderr.print("error: unsupported shell '{s}'; expected bash, zsh, or fish\n", .{shell});
        std.process.exit(1);
    };
    result catch |err| {
        if (err != error.BrokenPipe) return err;
    };
}
