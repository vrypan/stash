const std = @import("std");
const runtime = @import("runtime.zig");

const Allocator = std.mem.Allocator;

pub const Style = struct {
    dim: []const u8 = "",
    id: []const u8 = "",
    attr: []const u8 = "",
    reset: []const u8 = "",

    pub fn init(enabled: bool) Style {
        if (!enabled) return .{};
        return .{
            .dim = "\x1b[2m",
            .id = "\x1b[1;33m",
            .attr = "\x1b[36m",
            .reset = "\x1b[0m",
        };
    }
};

pub fn stdoutIsTerminal() bool {
    return runtime.stdoutIsTty();
}

pub const PageOptions = struct {
    disable_env: ?[]const u8 = null,
};

pub fn shouldPage(options: PageOptions) bool {
    if (!stdoutIsTerminal()) return false;
    if (options.disable_env) |key| {
        if (env(key) != null) return false;
    }
    return true;
}

pub const Output = struct {
    writer: runtime.FileWriter,
    child: ?std.process.Child = null,
    close_file: bool = false,
    broken_pipe: bool = false,

    pub fn init(allocator: Allocator, options: PageOptions) !Output {
        if (!shouldPage(options)) {
            return .{ .writer = runtime.stdoutWriter() };
        }

        var child = spawnPager(allocator) catch |err| switch (err) {
            error.FileNotFound => return .{ .writer = runtime.stdoutWriter() },
            else => |e| return e,
        };
        const stdin = child.stdin orelse {
            child.kill(runtime.process_io);
            return .{ .writer = runtime.stdoutWriter() };
        };
        child.stdin = null;
        return .{
            .writer = .{ .file = stdin },
            .child = child,
            .close_file = true,
        };
    }

    pub fn deinit(self: *Output) !void {
        if (self.close_file) {
            self.writer.file.close(runtime.process_io);
            self.close_file = false;
        }
        if (self.child) |*child| {
            _ = try child.wait(runtime.process_io);
            self.child = null;
        }
    }

    pub fn writeAll(self: *Output, data: []const u8) !void {
        if (self.broken_pipe) return;
        self.writer.writeAll(data) catch |err| switch (err) {
            error.BrokenPipe => self.broken_pipe = true,
            else => |e| return e,
        };
    }

    pub fn writeByte(self: *Output, byte: u8) !void {
        try self.writeAll(&.{byte});
    }

    pub fn print(self: *Output, comptime fmt: []const u8, args: anytype) !void {
        var sfb = std.heap.stackFallback(4096, std.heap.page_allocator);
        const ally = sfb.get();
        const data = try std.fmt.allocPrint(ally, fmt, args);
        defer ally.free(data);
        try self.writeAll(data);
    }
};

pub fn page(allocator: Allocator, content: []const u8) !void {
    var output = try Output.init(allocator, .{});
    try output.writeAll(content);
    try output.deinit();
}

fn spawnPager(allocator: Allocator) !std.process.Child {
    var argv: std.ArrayList([]const u8) = .empty;
    if (env("PAGER")) |pager| {
        var parts = std.mem.tokenizeAny(u8, pager, " \t\r\n");
        while (parts.next()) |part| try argv.append(allocator, part);
    } else {
        try argv.append(allocator, "less");
        try argv.append(allocator, "-R");
    }

    if (argv.items.len == 0) {
        return error.FileNotFound;
    }

    return spawnPagerWith(argv.items) catch |err| switch (err) {
        error.FileNotFound => {
            if (env("PAGER") == null and std.mem.eql(u8, argv.items[0], "less")) {
                return spawnPagerWith(&.{"more"});
            }
            return err;
        },
        else => |e| return e,
    };
}

fn spawnPagerWith(argv: []const []const u8) !std.process.Child {
    return std.process.spawn(runtime.process_io, .{
        .argv = argv,
        .stdin = .pipe,
        .stdout = .inherit,
        .stderr = .inherit,
    });
}

fn env(key: []const u8) ?[]const u8 {
    return std.process.Environ.getPosix(runtime.process_env, key);
}
