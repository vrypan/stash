const cmd = @import("cmd.zig");
const lib = @import("completion");

const subcommands = [_]@import("cli").CommandSpec{
    cmd.push_spec,
    cmd.tee_spec,
    cmd.cat_spec,
    cmd.ls_spec,
    cmd.attr_spec,
    cmd.attrs_spec,
    cmd.path_spec,
    cmd.rm_spec,
};

const spec = lib.CompletionSpec{
    .command = "stash",
    .commands = &cmd.commands,
    .root = cmd.root_spec,
    .subcommands = &subcommands,
};

pub fn generateBash(writer: anytype) !void {
    return lib.generateBash(writer, spec);
}

pub fn generateZsh(writer: anytype) !void {
    return lib.generateZsh(writer, spec);
}

pub fn generateFish(writer: anytype) !void {
    return lib.generateFish(writer, spec);
}
