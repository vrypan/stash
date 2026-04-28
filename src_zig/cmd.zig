const std = @import("std");
const display = @import("display.zig");
const runtime = @import("runtime.zig");
const store = @import("store.zig");
const types = @import("types.zig");

const Allocator = std.mem.Allocator;
const Attr = types.Attr;
const Meta = types.Meta;
const PrintTarget = types.PrintTarget;
const IdMode = types.IdMode;
const DateMode = types.DateMode;
const SizeMode = types.SizeMode;
const AttrsMode = types.AttrsMode;
const AttrFilter = types.AttrFilter;
const MetaSelection = types.MetaSelection;
pub fn errorMessage(err: anyerror) []const u8 {
    return switch (err) {
        error.InvalidArgument => "invalid argument",
        error.NotFound => "entry not found",
        error.StashEmpty => "stash is empty",
        error.IdTooShort => "id too short",
        error.AmbiguousId => "ambiguous id",
        error.InvalidAttr => "invalid attribute",
        error.InvalidRef => "invalid stack ref",
        error.ReadOnlyAttr => "only user-defined attributes are writable",
        else => @errorName(err),
    };
}

pub fn run(allocator: Allocator, args: []const [:0]const u8) !u8 {
    if (args.len <= 1) {
        return cmdPush(allocator, args[1..], false);
    }
    const first = args[1];
    if (std.mem.eql(u8, first, "--help") or std.mem.eql(u8, first, "-h")) {
        try printHelp();
        return 0;
    }
    if (std.mem.eql(u8, first, "--version") or std.mem.eql(u8, first, "-V")) {
        try runtime.stdoutWriter().writeAll("stash-cli 0.9.1\n");
        return 0;
    }
    if (std.mem.eql(u8, first, "push")) return cmdPush(allocator, args[2..], false);
    if (std.mem.eql(u8, first, "tee")) return cmdTee(allocator, args[2..]);
    if (std.mem.eql(u8, first, "cat")) return cmdCat(allocator, args[2..]);
    if (std.mem.eql(u8, first, "path")) return cmdPath(allocator, args[2..]);
    if (std.mem.eql(u8, first, "attr")) return cmdAttr(allocator, args[2..]);
    if (std.mem.eql(u8, first, "attrs")) return cmdAttrs(allocator, args[2..]);
    if (std.mem.eql(u8, first, "ls")) return cmdLs(allocator, args[2..]);
    if (std.mem.eql(u8, first, "rm")) return cmdRm(allocator, args[2..]);
    if (std.mem.eql(u8, first, "pop")) return cmdPop(allocator);

    return cmdPush(allocator, args[1..], false);
}

fn printHelp() !void {
    try runtime.stdoutWriter().writeAll(
        \\A local store for pipeline output and ad hoc file snapshots.
        \\
        \\Usage: stash [COMMAND] [ARGS]
        \\
        \\Commands: push, tee, cat, ls, attr, attrs, path, rm, pop
        \\
    );
}

fn cmdPush(allocator: Allocator, raw_args: []const [:0]const u8, tee_mode: bool) !u8 {
    var attrs: std.ArrayList(Attr) = .empty;
    var print_target: PrintTarget = .none;
    var file_arg: ?[]const u8 = null;

    var i: usize = 0;
    while (i < raw_args.len) : (i += 1) {
        const arg = raw_args[i];
        if (std.mem.eql(u8, arg, "-a") or std.mem.eql(u8, arg, "--attr")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try appendAttrFlag(allocator, &attrs, raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "-a") and arg.len > 2) {
            try appendAttrFlag(allocator, &attrs, arg[2..]);
        } else if (std.mem.eql(u8, arg, "--pocket")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try setAttrList(allocator, &attrs, types.pocket_attr, raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--pocket=")) {
            try setAttrList(allocator, &attrs, types.pocket_attr, arg["--pocket=".len..]);
        } else if (std.mem.eql(u8, arg, "--print")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            print_target = try parsePrintTarget(raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--print=")) {
            print_target = try parsePrintTarget(arg["--print=".len..]);
        } else if (std.mem.startsWith(u8, arg, "--save-on-error")) {
            // Accepted for documented tee compatibility; partial signal saving is
            // handled in a later slice.
        } else if (file_arg == null and !tee_mode) {
            file_arg = arg;
        } else {
            return error.InvalidArgument;
        }
    }

    if (!hasAttr(attrs.items, types.pocket_attr)) {
        if (store.activePocket(allocator)) |pocket| try setAttrList(allocator, &attrs, types.pocket_attr, pocket);
    }
    if (file_arg) |path| {
        if (std.fs.path.basename(path).len > 0) {
            try setAttrList(allocator, &attrs, "filename", std.fs.path.basename(path));
        }
    }

    const id = try store.pushInput(allocator, file_arg, attrs.items, tee_mode);
    try emitId(print_target, id);
    return 0;
}

fn cmdTee(allocator: Allocator, args: []const [:0]const u8) !u8 {
    return cmdPush(allocator, args, true);
}

fn appendAttrFlag(allocator: Allocator, attrs: *std.ArrayList(Attr), pair: []const u8) !void {
    const pos = std.mem.indexOfScalar(u8, pair, '=') orelse return error.InvalidAttr;
    try setAttrList(allocator, attrs, pair[0..pos], pair[pos + 1 ..]);
}

fn setAttrList(allocator: Allocator, attrs: *std.ArrayList(Attr), key: []const u8, value: []const u8) !void {
    for (attrs.items) |*item| {
        if (std.mem.eql(u8, item.key, key)) {
            item.value = try allocator.dupe(u8, value);
            return;
        }
    }
    try attrs.append(allocator, .{ .key = try allocator.dupe(u8, key), .value = try allocator.dupe(u8, value) });
    types.sortAttrs(attrs.items);
}

fn hasAttr(attrs: []const Attr, key: []const u8) bool {
    for (attrs) |item| if (std.mem.eql(u8, item.key, key)) return true;
    return false;
}

fn parsePrintTarget(value: []const u8) !PrintTarget {
    if (std.mem.eql(u8, value, "stdout") or std.mem.eql(u8, value, "1")) return .stdout;
    if (std.mem.eql(u8, value, "stderr") or std.mem.eql(u8, value, "2")) return .stderr;
    if (std.mem.eql(u8, value, "null") or std.mem.eql(u8, value, "0")) return .none;
    return error.InvalidArgument;
}

fn emitId(target: PrintTarget, id: []const u8) !void {
    switch (target) {
        .stdout => try runtime.stdoutWriter().print("{s}\n", .{id}),
        .stderr => try runtime.stderrWriter().print("{s}\n", .{id}),
        .none => {},
    }
}

fn cmdCat(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var refs: std.ArrayList([]const u8) = .empty;
    var filters: std.ArrayList(AttrFilter) = .empty;
    var reverse = false;
    try parseRefsAndFilters(allocator, raw_args, &refs, &filters, &reverse, true);

    const stdout = runtime.stdoutWriter();
    if (filters.items.len > 0) {
        if (refs.items.len > 0) return error.InvalidArgument;
        const items = try store.visibleList(allocator);
        var i: usize = 0;
        while (i < items.items.len) : (i += 1) {
            const idx = if (reverse) i else items.items.len - 1 - i;
            if (matchesFilters(&items.items[idx], filters.items)) try store.catId(allocator, items.items[idx].id, stdout);
        }
    } else if (refs.items.len == 0) {
        const id = try store.resolve(allocator, "");
        try store.catId(allocator, id, stdout);
    } else {
        var i: usize = 0;
        while (i < refs.items.len) : (i += 1) {
            const idx = if (reverse) refs.items.len - 1 - i else i;
            const id = try store.resolve(allocator, refs.items[idx]);
            try store.catId(allocator, id, stdout);
        }
    }
    return 0;
}

fn parseRefsAndFilters(
    allocator: Allocator,
    raw_args: []const [:0]const u8,
    refs: *std.ArrayList([]const u8),
    filters: *std.ArrayList(AttrFilter),
    reverse: *bool,
    allow_reverse: bool,
) !void {
    var i: usize = 0;
    while (i < raw_args.len) : (i += 1) {
        const arg = raw_args[i];
        if (allow_reverse and (std.mem.eql(u8, arg, "-r") or std.mem.eql(u8, arg, "--reverse"))) {
            reverse.* = true;
        } else if (std.mem.eql(u8, arg, "-a") or std.mem.eql(u8, arg, "--attr")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try appendFilter(allocator, filters, raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--attr=")) {
            try appendFilter(allocator, filters, arg["--attr=".len..]);
        } else if (std.mem.eql(u8, arg, "--pocket")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try filters.append(allocator, .{ .key = types.pocket_attr, .value = raw_args[i] });
        } else if (std.mem.startsWith(u8, arg, "--pocket=")) {
            try filters.append(allocator, .{ .key = types.pocket_attr, .value = arg["--pocket=".len..] });
        } else {
            try refs.append(allocator, arg);
        }
    }
}

fn appendFilter(allocator: Allocator, filters: *std.ArrayList(AttrFilter), value: []const u8) !void {
    if (value.len == 0 or std.mem.indexOfScalar(u8, value, ',') != null) return error.InvalidArgument;
    if (std.mem.indexOfScalar(u8, value, '=')) |pos| {
        try filters.append(allocator, .{ .key = value[0..pos], .value = value[pos + 1 ..] });
    } else {
        try filters.append(allocator, .{ .key = value, .value = null });
    }
}

fn matchesFilters(meta: *const Meta, filters: []const AttrFilter) bool {
    for (filters) |filter| {
        const value = meta.attr(filter.key) orelse return false;
        if (filter.value) |wanted| {
            if (!std.mem.eql(u8, value, wanted)) return false;
        }
    }
    return true;
}

fn cmdPath(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var want_attr = false;
    var want_dir = false;
    var ref: ?[]const u8 = null;
    for (raw_args) |raw| {
        const arg = raw;
        if (std.mem.eql(u8, arg, "-a") or std.mem.eql(u8, arg, "--attr")) want_attr = true else if (std.mem.eql(u8, arg, "-d") or std.mem.eql(u8, arg, "--dir")) want_dir = true else ref = arg;
    }
    const p = try store.basePaths(allocator);
    const out = runtime.stdoutWriter();
    if (ref) |r| {
        const id = try store.resolve(allocator, r);
        if (want_dir) {
            try out.print("{s}\n", .{p.base});
        } else if (want_attr) {
            try out.print("{s}\n", .{try store.attrPath(allocator, id)});
        } else {
            try out.print("{s}\n", .{try store.dataPath(allocator, id)});
        }
    } else {
        if (want_dir) try out.print("{s}\n", .{p.base}) else if (want_attr) try out.print("{s}\n", .{p.attr}) else try out.print("{s}\n", .{p.data});
    }
    return 0;
}

fn cmdAttr(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    if (raw_args.len == 0) return error.InvalidArgument;
    const id = try store.resolve(allocator, raw_args[0]);
    var meta = try store.getMeta(allocator, id);
    var json = false;
    var preview = false;
    var separator: []const u8 = "\t";
    var unset: std.ArrayList([]const u8) = .empty;
    var items: std.ArrayList([]const u8) = .empty;

    var i: usize = 1;
    while (i < raw_args.len) : (i += 1) {
        const arg = raw_args[i];
        if (std.mem.eql(u8, arg, "--json")) json = true else if (std.mem.eql(u8, arg, "-p") or std.mem.eql(u8, arg, "--preview")) preview = true else if (std.mem.eql(u8, arg, "--separator")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            separator = raw_args[i];
        } else if (std.mem.startsWith(u8, arg, "--separator=")) {
            separator = arg["--separator=".len..];
        } else if (std.mem.eql(u8, arg, "--unset")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try unset.append(allocator, raw_args[i]);
        } else {
            try items.append(allocator, arg);
        }
    }

    if (unset.items.len > 0) {
        if (items.items.len > 0) return error.InvalidArgument;
        for (unset.items) |key| {
            if (!display.writableAttrKey(key)) return error.ReadOnlyAttr;
            meta.unsetAttr(key);
        }
        try store.writeMeta(allocator, id, &meta);
        return 0;
    }

    var has_write = false;
    var has_read = false;
    for (items.items) |item| {
        if (std.mem.indexOfScalar(u8, item, '=') != null) {
            has_write = true;
        } else {
            has_read = true;
        }
    }
    if (has_write and has_read) return error.InvalidArgument;
    if (has_write) {
        for (items.items) |pair| {
            const pos = std.mem.indexOfScalar(u8, pair, '=') orelse return error.InvalidArgument;
            if (!display.writableAttrKey(pair[0..pos])) return error.ReadOnlyAttr;
            try meta.setAttr(allocator, pair[0..pos], pair[pos + 1 ..]);
        }
        try store.writeMeta(allocator, id, &meta);
        return 0;
    }

    const out = runtime.stdoutWriter();
    if (json) {
        try display.printAttrJson(out, &meta, items.items, preview);
        return 0;
    }
    if (items.items.len == 1) {
        const value = display.attrValue(&meta, items.items[0], preview) orelse return error.NotFound;
        try display.printEscapedDisplay(out, value);
        try out.writeByte('\n');
        return 0;
    }
    if (items.items.len > 0) {
        for (items.items) |key| {
            const value = display.attrValue(&meta, key, preview) orelse return error.NotFound;
            try out.print("{s}{s}", .{ key, separator });
            try display.printEscapedDisplay(out, value);
            try out.writeByte('\n');
        }
        return 0;
    }
    try out.print("id{s}{s}\nts{s}{s}\nsize{s}{}\n", .{ separator, meta.id, separator, meta.ts, separator, meta.size });
    for (meta.attrs.items) |item| {
        try out.print("{s}{s}", .{ item.key, separator });
        try display.printEscapedDisplay(out, item.value);
        try out.writeByte('\n');
    }
    if (preview and meta.preview.len > 0) {
        try out.print("preview{s}", .{separator});
        try display.printEscapedDisplay(out, meta.preview);
        try out.writeByte('\n');
    }
    return 0;
}

fn cmdAttrs(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var key: ?[]const u8 = null;
    var count = false;
    for (raw_args) |raw| {
        const arg = raw;
        if (std.mem.eql(u8, arg, "--count")) count = true else key = arg;
    }
    const items = try store.visibleList(allocator);
    var counts = std.StringHashMap(usize).init(allocator);
    for (items.items) |*meta| {
        if (key) |wanted| {
            if (meta.attr(wanted)) |value| {
                const entry = try counts.getOrPut(value);
                if (!entry.found_existing) entry.value_ptr.* = 0;
                entry.value_ptr.* += 1;
            }
        } else {
            for (meta.attrs.items) |attr| {
                const entry = try counts.getOrPut(attr.key);
                if (!entry.found_existing) entry.value_ptr.* = 0;
                entry.value_ptr.* += 1;
            }
        }
    }
    var names: std.ArrayList([]const u8) = .empty;
    var it = counts.iterator();
    while (it.next()) |entry| try names.append(allocator, entry.key_ptr.*);
    std.mem.sort([]const u8, names.items, {}, ascSlices);
    const out = runtime.stdoutWriter();
    for (names.items) |name| {
        if (count) try out.print("{s}\t{}\n", .{ name, counts.get(name).? }) else try out.print("{s}\n", .{name});
    }
    return 0;
}

fn cmdLs(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var id_mode: IdMode = .short;
    var date_mode: ?DateMode = null;
    var size_mode: ?SizeMode = null;
    var attrs_mode: AttrsMode = .none;
    var show_name = false;
    var show_preview = false;
    var headers = false;
    var json = false;
    var reverse = false;
    var number: usize = 0;
    var chars: usize = 80;
    var color = true;
    var before_ref: ?[]const u8 = null;
    var after_ref: ?[]const u8 = null;
    var selection = MetaSelection{};

    var i: usize = 0;
    while (i < raw_args.len) : (i += 1) {
        const arg = raw_args[i];
        if (std.mem.eql(u8, arg, "-l") or std.mem.eql(u8, arg, "--long")) {
            date_mode = .ls;
            size_mode = .human;
            attrs_mode = .flag;
            show_preview = true;
        } else if (std.mem.eql(u8, arg, "--json")) json = true else if (std.mem.eql(u8, arg, "--headers")) headers = true else if (std.mem.eql(u8, arg, "--name")) show_name = true else if (std.mem.eql(u8, arg, "-p") or std.mem.eql(u8, arg, "--preview")) show_preview = true else if (std.mem.eql(u8, arg, "-r") or std.mem.eql(u8, arg, "--reverse")) reverse = true else if (std.mem.eql(u8, arg, "-A")) {
            if (i + 1 < raw_args.len and !std.mem.startsWith(u8, raw_args[i + 1], "-") and isAttrsMode(raw_args[i + 1])) {
                i += 1;
                attrs_mode = try parseAttrsMode(raw_args[i]);
            } else {
                attrs_mode = .list;
            }
            if (attrs_mode == .list) selection.show_all = true;
        } else if (std.mem.eql(u8, arg, "-a") or std.mem.eql(u8, arg, "--attr")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try parseMetaSelectionArg(allocator, &selection, raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--id=")) id_mode = parseIdMode(arg["--id=".len..]) else if (std.mem.eql(u8, arg, "--id")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            id_mode = parseIdMode(raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--date=")) date_mode = try parseDateMode(arg["--date=".len..]) else if (std.mem.eql(u8, arg, "--date")) {
            if (i + 1 < raw_args.len and !std.mem.startsWith(u8, raw_args[i + 1], "-") and isDateMode(raw_args[i + 1])) {
                i += 1;
                date_mode = try parseDateMode(raw_args[i]);
            } else {
                date_mode = .ls;
            }
        } else if (std.mem.startsWith(u8, arg, "--size=")) size_mode = try parseSizeMode(arg["--size=".len..]) else if (std.mem.eql(u8, arg, "--size")) {
            if (i + 1 < raw_args.len and !std.mem.startsWith(u8, raw_args[i + 1], "-") and isSizeMode(raw_args[i + 1])) {
                i += 1;
                size_mode = try parseSizeMode(raw_args[i]);
            } else {
                size_mode = .human;
            }
        } else if (std.mem.startsWith(u8, arg, "--attrs=")) {
            attrs_mode = try parseAttrsMode(arg["--attrs=".len..]);
            if (attrs_mode == .list) selection.show_all = true;
        } else if (std.mem.eql(u8, arg, "--attrs")) {
            if (i + 1 < raw_args.len and !std.mem.startsWith(u8, raw_args[i + 1], "-") and isAttrsMode(raw_args[i + 1])) {
                i += 1;
                attrs_mode = try parseAttrsMode(raw_args[i]);
            } else {
                attrs_mode = .list;
            }
            if (attrs_mode == .list) selection.show_all = true;
        } else if (std.mem.eql(u8, arg, "-n") or std.mem.eql(u8, arg, "--number")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            number = try std.fmt.parseInt(usize, raw_args[i], 10);
        } else if (std.mem.eql(u8, arg, "--before")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            before_ref = raw_args[i];
        } else if (std.mem.startsWith(u8, arg, "--before=")) {
            before_ref = arg["--before=".len..];
        } else if (std.mem.eql(u8, arg, "--after")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            after_ref = raw_args[i];
        } else if (std.mem.startsWith(u8, arg, "--after=")) {
            after_ref = arg["--after=".len..];
        } else if (std.mem.startsWith(u8, arg, "--chars=")) {
            chars = try std.fmt.parseInt(usize, arg["--chars=".len..], 10);
        } else if (std.mem.eql(u8, arg, "--chars")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            chars = try std.fmt.parseInt(usize, raw_args[i], 10);
        } else if (std.mem.startsWith(u8, arg, "--pocket=")) {
            try selection.filter_values.append(allocator, .{ .key = types.pocket_attr, .value = arg["--pocket=".len..] });
        } else if (std.mem.eql(u8, arg, "--pocket")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try selection.filter_values.append(allocator, .{ .key = types.pocket_attr, .value = raw_args[i] });
        } else if (std.mem.startsWith(u8, arg, "--color=")) {
            color = try parseBoolFlag(arg["--color=".len..]);
        } else if (std.mem.eql(u8, arg, "--color")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            color = try parseBoolFlag(raw_args[i]);
        } else {
            return error.InvalidArgument;
        }
    }
    if (before_ref != null and after_ref != null) return error.InvalidArgument;

    var items = try store.visibleList(allocator);
    if (before_ref) |reference| {
        const id = try store.resolve(allocator, reference);
        keepOlderThan(&items, id);
    } else if (after_ref) |reference| {
        const id = try store.resolve(allocator, reference);
        keepNewerThan(&items, id);
    }
    filterItems(&items, &selection);
    if (reverse) std.mem.reverse(Meta, items.items);
    if (number > 0 and items.items.len > number) items.items.len = number;

    if (json) {
        try display.printLsJson(allocator, runtime.stdoutWriter(), items.items, date_mode orelse .ls, chars);
    } else {
        try display.printLsTable(allocator, runtime.stdoutWriter(), items.items, id_mode, date_mode, size_mode, attrs_mode, show_name, show_preview, headers, chars, color, &selection);
    }
    return 0;
}

fn filterItems(items: *std.ArrayList(Meta), selection: *const MetaSelection) void {
    var write: usize = 0;
    for (items.items) |item| {
        if (matchesMetaSelection(&item, selection)) {
            items.items[write] = item;
            write += 1;
        }
    }
    items.items.len = write;
}

fn keepOlderThan(items: *std.ArrayList(Meta), id: []const u8) void {
    for (items.items, 0..) |item, idx| {
        if (std.mem.eql(u8, item.id, id)) {
            const older = items.items[idx + 1 ..];
            std.mem.copyForwards(Meta, items.items[0..older.len], older);
            items.items.len = older.len;
            return;
        }
    }
    items.items.len = 0;
}

fn keepNewerThan(items: *std.ArrayList(Meta), id: []const u8) void {
    for (items.items, 0..) |item, idx| {
        if (std.mem.eql(u8, item.id, id)) {
            items.items.len = idx;
            return;
        }
    }
    items.items.len = 0;
}

fn matchesMetaSelection(meta: *const Meta, selection: *const MetaSelection) bool {
    for (selection.filter_tags.items) |key| if (meta.attr(key) == null) return false;
    for (selection.filter_values.items) |filter| {
        const value = meta.attr(filter.key) orelse return false;
        if (!std.mem.eql(u8, value, filter.value.?)) return false;
    }
    return true;
}

fn cmdRm(allocator: Allocator, raw_args: []const [:0]const u8) !u8 {
    var refs: std.ArrayList([]const u8) = .empty;
    var filters: std.ArrayList(AttrFilter) = .empty;
    var before_ref: ?[]const u8 = null;
    var after_ref: ?[]const u8 = null;
    var i: usize = 0;
    while (i < raw_args.len) : (i += 1) {
        const arg = raw_args[i];
        if (std.mem.eql(u8, arg, "-f") or std.mem.eql(u8, arg, "--force")) {
            // Confirmation prompts are not implemented yet; accept the flag.
        } else if (std.mem.eql(u8, arg, "-a") or std.mem.eql(u8, arg, "--attr")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            try appendFilter(allocator, &filters, raw_args[i]);
        } else if (std.mem.startsWith(u8, arg, "--attr=")) {
            try appendFilter(allocator, &filters, arg["--attr=".len..]);
        } else if (std.mem.eql(u8, arg, "--before")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            before_ref = raw_args[i];
        } else if (std.mem.startsWith(u8, arg, "--before=")) {
            before_ref = arg["--before=".len..];
        } else if (std.mem.eql(u8, arg, "--after")) {
            i += 1;
            if (i >= raw_args.len) return error.InvalidArgument;
            after_ref = raw_args[i];
        } else if (std.mem.startsWith(u8, arg, "--after=")) {
            after_ref = arg["--after=".len..];
        } else {
            try refs.append(allocator, arg);
        }
    }
    if (before_ref != null and after_ref != null) return error.InvalidArgument;
    if (filters.items.len > 0) {
        if (refs.items.len > 0 or before_ref != null or after_ref != null) return error.InvalidArgument;
        const items = try store.visibleList(allocator);
        for (items.items) |*meta| if (matchesFilters(meta, filters.items)) try store.removeId(allocator, meta.id);
        return 0;
    }
    if (before_ref) |reference| {
        if (refs.items.len > 0) return error.InvalidArgument;
        const id = try store.resolve(allocator, reference);
        var items = try store.visibleList(allocator);
        keepOlderThan(&items, id);
        for (items.items) |meta| try store.removeId(allocator, meta.id);
        return 0;
    }
    if (after_ref) |reference| {
        if (refs.items.len > 0) return error.InvalidArgument;
        const id = try store.resolve(allocator, reference);
        var items = try store.visibleList(allocator);
        keepNewerThan(&items, id);
        for (items.items) |meta| try store.removeId(allocator, meta.id);
        return 0;
    }
    if (refs.items.len == 0) return error.InvalidArgument;
    for (refs.items) |r| try store.removeId(allocator, try store.resolve(allocator, r));
    return 0;
}

fn cmdPop(allocator: Allocator) !u8 {
    const id = try store.resolve(allocator, "");
    try store.catId(allocator, id, runtime.stdoutWriter());
    try store.removeId(allocator, id);
    return 0;
}

fn parseMetaSelectionArg(allocator: Allocator, sel: *MetaSelection, value: []const u8) !void {
    if (std.mem.startsWith(u8, value, "++")) {
        const rest = value[2..];
        if (std.mem.indexOfScalar(u8, rest, '=')) |pos| {
            try sel.display_tags.append(allocator, rest[0..pos]);
            try sel.filter_values.append(allocator, .{ .key = rest[0..pos], .value = rest[pos + 1 ..] });
        } else {
            try sel.display_tags.append(allocator, rest);
            try sel.filter_tags.append(allocator, rest);
        }
    } else if (std.mem.startsWith(u8, value, "+")) {
        try sel.display_tags.append(allocator, value[1..]);
    } else if (std.mem.indexOfScalar(u8, value, '=')) |pos| {
        try sel.filter_values.append(allocator, .{ .key = value[0..pos], .value = value[pos + 1 ..] });
    } else {
        try sel.filter_tags.append(allocator, value);
    }
}

fn parseIdMode(value: []const u8) IdMode {
    if (std.mem.eql(u8, value, "full")) return .full;
    if (std.mem.eql(u8, value, "pos")) return .pos;
    return .short;
}

fn parseDateMode(value: []const u8) !DateMode {
    if (std.mem.eql(u8, value, "iso") or std.mem.eql(u8, value, "absolute")) return .iso;
    if (std.mem.eql(u8, value, "ago") or std.mem.eql(u8, value, "relative")) return .ago;
    if (std.mem.eql(u8, value, "ls")) return .ls;
    return error.InvalidArgument;
}

fn parseSizeMode(value: []const u8) !SizeMode {
    if (std.mem.eql(u8, value, "human")) return .human;
    if (std.mem.eql(u8, value, "bytes")) return .bytes;
    return error.InvalidArgument;
}

fn parseAttrsMode(value: []const u8) !AttrsMode {
    if (std.mem.eql(u8, value, "list")) return .list;
    if (std.mem.eql(u8, value, "count")) return .count;
    if (std.mem.eql(u8, value, "flag")) return .flag;
    return error.InvalidArgument;
}

fn isDateMode(value: []const u8) bool {
    return std.mem.eql(u8, value, "iso") or std.mem.eql(u8, value, "absolute") or
        std.mem.eql(u8, value, "ago") or std.mem.eql(u8, value, "relative") or
        std.mem.eql(u8, value, "ls");
}

fn isSizeMode(value: []const u8) bool {
    return std.mem.eql(u8, value, "human") or std.mem.eql(u8, value, "bytes");
}

fn isAttrsMode(value: []const u8) bool {
    return std.mem.eql(u8, value, "list") or std.mem.eql(u8, value, "count") or std.mem.eql(u8, value, "flag");
}

fn parseBoolFlag(value: []const u8) !bool {
    if (std.mem.eql(u8, value, "true")) return true;
    if (std.mem.eql(u8, value, "false")) return false;
    return error.InvalidArgument;
}

fn ascSlices(_: void, a: []const u8, b: []const u8) bool {
    return std.mem.lessThan(u8, a, b);
}
