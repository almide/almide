// fan floor probe: usage  floor <seq|par> <sweep|nest|busy> <iters> <offers>
// zig 0.13.0: std.Thread.Pool + WaitGroup, the caller helping (waitAndWork).
const std = @import("std");

const P: i64 = 1000003;
var pool: std.Thread.Pool = undefined;
var par: bool = false;

fn work(s: i64, i: i64, iters: i64) i64 {
    var acc = s + i;
    var j: i64 = 0;
    while (j < iters) : (j += 1) acc = @mod(acc * 31 + j, P);
    return acc;
}

fn elem(out: *i64, s: i64, i: i64, iters: i64) void {
    out.* = work(s, i, iters);
}

fn step(s: i64, iters: i64) i64 {
    var out: [10]i64 = undefined;
    if (par) {
        var wg: std.Thread.WaitGroup = .{};
        for (0..10) |i| pool.spawnWg(&wg, elem, .{ &out[i], s, @as(i64, @intCast(i)), iters });
        pool.waitAndWork(&wg);
    } else {
        for (0..10) |i| out[i] = work(s, @intCast(i), iters);
    }
    var a: i64 = 0;
    for (out) |x| a += x;
    return a;
}

fn innerSum(s: i64, iters: i64) i64 {
    // the nest probe folds mod P; step's plain sum differs, so recompute here
    var out: [10]i64 = undefined;
    if (par) {
        var wg: std.Thread.WaitGroup = .{};
        for (0..10) |i| pool.spawnWg(&wg, elem, .{ &out[i], s, @as(i64, @intCast(i)), iters });
        pool.waitAndWork(&wg);
    } else {
        for (0..10) |i| out[i] = work(s, @intCast(i), iters);
    }
    var a: i64 = 0;
    for (out) |x| a = @mod(a + x, P);
    return a;
}

fn outer(out: *i64, s: i64, iters: i64) void {
    out.* = innerSum(s, iters);
}

fn lane(out: *i64, seed: i64, offers: i64, iters: i64) void {
    var s = seed;
    var k: i64 = 0;
    while (k < offers) : (k += 1) s = @mod(s + step(s, iters), P);
    out.* = s;
}

pub fn main() !void {
    const alloc = std.heap.page_allocator;
    const args = try std.process.argsAlloc(alloc);
    par = std.mem.eql(u8, args[1], "par");
    const probe = args[2];
    const iters = try std.fmt.parseInt(i64, args[3], 10);
    const offers = try std.fmt.parseInt(i64, args[4], 10);
    try pool.init(.{ .allocator = alloc });
    defer pool.deinit();
    const stdout = std.io.getStdOut().writer();
    if (std.mem.eql(u8, probe, "sweep")) {
        var s: i64 = 0;
        lane(&s, 1, offers, iters);
        try stdout.print("{d}\n", .{s});
    } else if (std.mem.eql(u8, probe, "nest")) {
        var s: i64 = 1;
        var k: i64 = 0;
        while (k < offers) : (k += 1) {
            var ys: [4]i64 = undefined;
            if (par) {
                var wg: std.Thread.WaitGroup = .{};
                for (0..4) |q| pool.spawnWg(&wg, outer, .{ &ys[q], s + @as(i64, @intCast(q)), iters });
                pool.waitAndWork(&wg);
            } else {
                for (0..4) |q| ys[q] = innerSum(s + @as(i64, @intCast(q)), iters);
            }
            var a: i64 = 0;
            for (ys) |y| a += y;
            s = @mod(s + a, P);
        }
        try stdout.print("{d}\n", .{s});
    } else {
        var a: i64 = 0;
        var b: i64 = 0;
        if (par) {
            const t1 = try std.Thread.spawn(.{}, lane, .{ &a, 1, offers, iters });
            const t2 = try std.Thread.spawn(.{}, lane, .{ &b, 2, offers, iters });
            t1.join();
            t2.join();
        } else {
            lane(&a, 1, offers, iters);
            lane(&b, 2, offers, iters);
        }
        try stdout.print("{d} {d}\n", .{ a, b });
    }
}
