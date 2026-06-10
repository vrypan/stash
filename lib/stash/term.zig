const std = @import("std");
const runtime = @import("runtime.zig");

const Allocator = std.mem.Allocator;

pub const Style = struct {
    dim: []const u8 = "",
    reset: []const u8 = "",

    pub fn init(enabled: bool) Style {
        if (!enabled) return .{};
        return .{
            .dim = "\x1b[2m",
            .reset = "\x1b[0m",
        };
    }
};

pub fn stdoutIsTerminal() bool {
    return runtime.stdoutIsTty();
}

pub fn shouldPage() bool {
    return stdoutIsTerminal() and env("BOOKMARK_NO_PAGER") == null;
}

pub fn page(allocator: Allocator, content: []const u8) !void {
    var argv: std.ArrayList([]const u8) = .empty;
    if (env("PAGER")) |pager| {
        var parts = std.mem.tokenizeAny(u8, pager, " \t\r\n");
        while (parts.next()) |part| try argv.append(allocator, part);
    } else {
        try argv.append(allocator, "less");
        try argv.append(allocator, "-R");
    }

    if (argv.items.len == 0) {
        try runtime.stdoutWriter().writeAll(content);
        return;
    }

    pageWith(argv.items, content) catch |err| switch (err) {
        error.FileNotFound => {
            if (env("PAGER") == null and std.mem.eql(u8, argv.items[0], "less")) {
                try pageWith(&.{"more"}, content);
                return;
            }
            try runtime.stdoutWriter().writeAll(content);
        },
        else => |e| return e,
    };
}

fn pageWith(argv: []const []const u8, content: []const u8) !void {
    var child = try std.process.spawn(runtime.process_io, .{
        .argv = argv,
        .stdin = .pipe,
        .stdout = .inherit,
        .stderr = .inherit,
    });
    defer child.kill(runtime.process_io);

    if (child.stdin) |stdin| {
        const writer = runtime.FileWriter{ .file = stdin };
        writer.writeAll(content) catch |err| switch (err) {
            error.BrokenPipe => {},
            else => |e| return e,
        };
        stdin.close(runtime.process_io);
        child.stdin = null;
    }

    _ = try child.wait(runtime.process_io);
}

fn env(key: []const u8) ?[]const u8 {
    return std.process.Environ.getPosix(runtime.process_env, key);
}
