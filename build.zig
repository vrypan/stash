const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const zli_dep = b.dependency("zli", .{ .target = target, .optimize = optimize });
    const version = packageVersion(b);
    const build_options = b.addOptions();
    build_options.addOption([]const u8, "version", version);

    const exe = b.addExecutable(.{
        .name = "stash",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    exe.root_module.addImport("zli", zli_dep.module("zli"));
    exe.root_module.addOptions("build_options", build_options);

    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }
    const run_step = b.step("run", "Run the Zig stash binary");
    run_step.dependOn(&run_cmd.step);
}

fn packageVersion(b: *std.Build) []const u8 {
    const Manifest = struct {
        version: []const u8,
    };
    const source = b.build_root.handle.readFileAllocOptions(
        b.graph.io,
        "build.zig.zon",
        b.allocator,
        .limited(64 * 1024),
        .of(u8),
        0,
    ) catch @panic("failed to read build.zig.zon");
    const manifest = std.zon.parse.fromSliceAlloc(
        Manifest,
        b.allocator,
        source,
        null,
        .{ .ignore_unknown_fields = true },
    ) catch @panic("failed to parse build.zig.zon");
    return manifest.version;
}
