const std = @import("std");

const Allocator = std.mem.Allocator;

pub const short_id_len = 8;
pub const min_id_len = 6;
pub const pocket_attr = "pocket";
pub const pocket_env = "STASH_POCKET";

pub const Attr = struct {
    key: []u8,
    value: []u8,
};

pub const Meta = struct {
    id: []u8,
    ts: []u8,
    size: i64,
    preview: []u8,
    attrs: std.ArrayList(Attr),

    pub fn init() Meta {
        return .{
            .id = &.{},
            .ts = &.{},
            .size = 0,
            .preview = &.{},
            .attrs = .empty,
        };
    }

    pub fn attr(self: *const Meta, key: []const u8) ?[]const u8 {
        for (self.attrs.items) |item| {
            if (std.mem.eql(u8, item.key, key)) return item.value;
        }
        return null;
    }

    pub fn setAttr(self: *Meta, allocator: Allocator, key: []const u8, value: []const u8) !void {
        for (self.attrs.items) |*item| {
            if (std.mem.eql(u8, item.key, key)) {
                item.value = try allocator.dupe(u8, value);
                return;
            }
        }
        try self.attrs.append(allocator, .{
            .key = try allocator.dupe(u8, key),
            .value = try allocator.dupe(u8, value),
        });
        sortAttrs(self.attrs.items);
    }

    pub fn unsetAttr(self: *Meta, key: []const u8) void {
        var i: usize = 0;
        while (i < self.attrs.items.len) {
            if (std.mem.eql(u8, self.attrs.items[i].key, key)) {
                _ = self.attrs.orderedRemove(i);
                return;
            }
            i += 1;
        }
    }

    pub fn shortId(self: *const Meta) []const u8 {
        if (self.id.len <= short_id_len) return self.id;
        return self.id[self.id.len - short_id_len ..];
    }
};

pub const PrintTarget = enum { stdout, stderr, none };
pub const IdMode = enum { short, full, pos };
pub const DateMode = enum { iso, ago, ls };
pub const AttrsMode = enum { none, list, count, flag };

pub const AttrFilter = struct {
    key: []const u8,
    value: ?[]const u8,
};

pub const MetaSelection = struct {
    show_all: bool = false,
    display_tags: std.ArrayList([]const u8) = .empty,
    filter_tags: std.ArrayList([]const u8) = .empty,
    filter_values: std.ArrayList(AttrFilter) = .empty,
};

pub const Paths = struct {
    base: []u8,
    data: []u8,
    attr: []u8,
    tmp: []u8,
    cache: []u8,
};

pub fn sortAttrs(attrs: []Attr) void {
    std.mem.sort(Attr, attrs, {}, struct {
        fn less(_: void, a: Attr, b: Attr) bool {
            return std.mem.lessThan(u8, a.key, b.key);
        }
    }.less);
}
