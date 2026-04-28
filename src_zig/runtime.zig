const std = @import("std");

pub var process_env: std.process.Environ = .empty;
pub var process_io: std.Io = undefined;

pub const PosixWriter = struct {
    fd: std.c.fd_t,

    pub fn writeAll(self: PosixWriter, data: []const u8) !void {
        var off: usize = 0;
        while (off < data.len) {
            const n = std.c.write(self.fd, data[off..].ptr, data.len - off);
            if (n < 0) return error.WriteFailed;
            if (n == 0) return error.WriteFailed;
            off += @intCast(n);
        }
    }

    pub fn writeByte(self: PosixWriter, byte: u8) !void {
        try self.writeAll(&.{byte});
    }

    pub fn writeByteNTimes(self: PosixWriter, byte: u8, n: usize) !void {
        var i: usize = 0;
        while (i < n) : (i += 1) try self.writeByte(byte);
    }

    pub fn print(self: PosixWriter, comptime fmt: []const u8, args: anytype) !void {
        const data = try std.fmt.allocPrint(std.heap.page_allocator, fmt, args);
        try self.writeAll(data);
    }
};

pub const FileWriter = struct {
    file: std.Io.File,

    pub fn writeAll(self: FileWriter, data: []const u8) !void {
        try self.file.writeStreamingAll(process_io, data);
    }

    pub fn writeByte(self: FileWriter, byte: u8) !void {
        try self.writeAll(&.{byte});
    }

    pub fn print(self: FileWriter, comptime fmt: []const u8, args: anytype) !void {
        const data = try std.fmt.allocPrint(std.heap.page_allocator, fmt, args);
        try self.writeAll(data);
    }
};

pub fn stdoutWriter() PosixWriter {
    return .{ .fd = std.posix.STDOUT_FILENO };
}

pub fn stderrWriter() PosixWriter {
    return .{ .fd = std.posix.STDERR_FILENO };
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
