const std = @import("std");
const builtin = @import("builtin");

pub var process_env: std.process.Environ = .empty;
pub var process_io: std.Io = undefined;

const supports_posix_signals = switch (builtin.os.tag) {
    .linux,
    .driverkit,
    .ios,
    .maccatalyst,
    .macos,
    .tvos,
    .visionos,
    .watchos,
    .freebsd,
    .netbsd,
    .openbsd,
    .illumos,
    .haiku,
    => true,
    else => false,
};

const SignalAction = if (supports_posix_signals) std.posix.Sigaction else void;
var interrupt_requested = std.atomic.Value(bool).init(false);

pub const InterruptGuard = struct {
    active: bool = false,
    old_int: SignalAction = undefined,
    old_term: SignalAction = undefined,

    pub fn deinit(self: *InterruptGuard) void {
        if (!self.active) return;
        if (comptime supports_posix_signals) {
            std.posix.sigaction(.INT, &self.old_int, null);
            std.posix.sigaction(.TERM, &self.old_term, null);
        }
        self.active = false;
    }
};

fn interruptHandler(_: std.posix.SIG) callconv(.c) void {
    interrupt_requested.store(true, .seq_cst);
}

pub fn installInterruptHandlers() InterruptGuard {
    interrupt_requested.store(false, .seq_cst);
    if (comptime !supports_posix_signals) return .{};

    var guard = InterruptGuard{ .active = true };
    const action = std.posix.Sigaction{
        .handler = .{ .handler = interruptHandler },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    };
    std.posix.sigaction(.INT, &action, &guard.old_int);
    std.posix.sigaction(.TERM, &action, &guard.old_term);
    return guard;
}

pub fn interrupted() bool {
    return interrupt_requested.load(.seq_cst);
}

pub const FileWriter = struct {
    file: std.Io.File,

    pub fn writeAll(self: FileWriter, data: []const u8) !void {
        try self.file.writeStreamingAll(process_io, data);
    }

    pub fn writeByte(self: FileWriter, byte: u8) !void {
        try self.writeAll(&.{byte});
    }

    pub fn writeByteNTimes(self: FileWriter, byte: u8, n: usize) !void {
        var buf: [256]u8 = undefined;
        @memset(&buf, byte);
        var remaining = n;
        while (remaining > 0) {
            const chunk = @min(remaining, buf.len);
            try self.writeAll(buf[0..chunk]);
            remaining -= chunk;
        }
    }

    pub fn print(self: FileWriter, comptime fmt: []const u8, args: anytype) !void {
        var sfb = std.heap.stackFallback(4096, std.heap.page_allocator);
        const ally = sfb.get();
        const data = try std.fmt.allocPrint(ally, fmt, args);
        defer ally.free(data);
        try self.writeAll(data);
    }
};

pub fn stdoutWriter() FileWriter {
    return .{ .file = .stdout() };
}

pub fn stderrWriter() FileWriter {
    return .{ .file = .stderr() };
}

pub fn stdinIsTty() bool {
    return std.Io.File.stdin().isTty(process_io) catch false;
}

pub fn stdoutIsTty() bool {
    return std.Io.File.stdout().isTty(process_io) catch false;
}

pub fn envOwned(allocator: std.mem.Allocator, key: []const u8) ?[]u8 {
    const value = std.process.Environ.getPosix(process_env, key) orelse return null;
    return allocator.dupe(u8, value) catch null;
}

pub fn cwd() std.Io.Dir {
    return std.Io.Dir.cwd();
}

pub fn fileRead(file: std.Io.File, buf: []u8) !usize {
    return file.readStreaming(process_io, &.{buf}) catch |err| switch (err) {
        error.EndOfStream => 0,
        else => |e| return e,
    };
}
