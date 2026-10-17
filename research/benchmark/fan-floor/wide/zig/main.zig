// fan floor, wide probes: usage  wide <seq|std|hand> <size|skew|heap|fail> <n> <iters> <reps>
// zig 0.13.0, linked against libc (c_allocator for the heap probe).
//   std:  std.Thread.Pool, one spawnWg per chunk of n / (4W), caller helping
//   hand: W-1 resident threads + the caller, woken by a futex generation,
//         drawing chunks of n / (4W) from an atomic counter
// Every implementation runs every element and keeps the lowest-index error.
const std = @import("std");

const P: i64 = 1000003;
const Res = struct { v: i64, ok: bool };

var kind: u8 = 's';
var s0: i64 = 1;
var iters: i64 = 0;
var n: usize = 0;
var chunk: usize = 1;
var out: []Res = undefined;

fn work(s: i64, i: i64, it: i64) i64 {
    var acc = s + i;
    var j: i64 = 0;
    while (j < it) : (j += 1) acc = @mod(acc * 31 + j, P);
    return acc;
}

fn elem(i: i64) Res {
    switch (kind) {
        's' => return .{ .v = work(s0, i, iters), .ok = true },
        'k' => return .{ .v = work(s0, i, if (@mod(i, 10) == 0) iters * 20 else iters), .ok = true },
        'h' => {
            const a = std.heap.c_allocator;
            var parts: [20][]u8 = undefined;
            for (0..20) |j| parts[j] = std.fmt.allocPrint(a, "{d}", .{@mod((s0 + i) * @as(i64, @intCast(j)), 1000)}) catch unreachable;
            const joined = std.mem.join(a, ",", &parts) catch unreachable;
            const len: i64 = @intCast(joined.len);
            a.free(joined);
            for (parts) |p| a.free(p);
            return .{ .v = len, .ok = true };
        },
        else => {
            if (@mod(i, 97) == 13) {
                var buf: [32]u8 = undefined;
                const m = std.fmt.bufPrint(&buf, "bad {d}", .{i}) catch unreachable;
                return .{ .v = @intCast(m.len), .ok = false };
            }
            return .{ .v = work(s0, i, iters), .ok = true };
        },
    }
}

fn range(lo: usize, hi: usize) void {
    var i = lo;
    while (i < hi) : (i += 1) out[i] = elem(@intCast(i));
}

// hand-written resident pool
var gen = std.atomic.Value(u32).init(0);
var next = std.atomic.Value(usize).init(0);
var pending = std.atomic.Value(u32).init(0);
var quit = std.atomic.Value(bool).init(false);

fn drain() void {
    while (true) {
        const lo = next.fetchAdd(chunk, .monotonic);
        if (lo >= n) return;
        range(lo, @min(lo + chunk, n));
    }
}

fn worker() void {
    var seen: u32 = 0;
    while (true) {
        var spins: u32 = 0;
        while (gen.load(.acquire) == seen) {
            if (spins < 2000) {
                std.atomic.spinLoopHint();
                spins += 1;
            } else std.Thread.Futex.wait(&gen, seen);
        }
        seen = gen.load(.acquire);
        if (quit.load(.acquire)) return;
        drain();
        if (pending.fetchSub(1, .acq_rel) == 1) std.Thread.Futex.wake(&pending, 1);
    }
}

pub fn main() !void {
    const alloc = std.heap.c_allocator;
    const args = try std.process.argsAlloc(alloc);
    const imp = args[1];
    kind = switch (args[2][0]) {
        's' => if (args[2][1] == 'i') 's' else 'k',
        'h' => 'h',
        else => 'f',
    };
    n = try std.fmt.parseInt(usize, args[3], 10);
    iters = try std.fmt.parseInt(i64, args[4], 10);
    const reps = try std.fmt.parseInt(usize, args[5], 10);
    const W = try std.Thread.getCpuCount();
    chunk = @max(1, n / (4 * W));
    out = try alloc.alloc(Res, n);
    var pool: std.Thread.Pool = undefined;
    const isStd = std.mem.eql(u8, imp, "std");
    const isHand = std.mem.eql(u8, imp, "hand");
    if (isStd) try pool.init(.{ .allocator = alloc });
    var threads: [256]std.Thread = undefined;
    if (isHand) for (0..W - 1) |k| {
        threads[k] = try std.Thread.spawn(.{}, worker, .{});
    };
    var s: i64 = 1;
    for (0..reps) |_| {
        s0 = s;
        if (isStd) {
            var wg: std.Thread.WaitGroup = .{};
            var lo: usize = 0;
            while (lo < n) : (lo += chunk) pool.spawnWg(&wg, range, .{ lo, @min(lo + chunk, n) });
            pool.waitAndWork(&wg);
        } else if (isHand) {
            next.store(0, .monotonic);
            pending.store(@intCast(W - 1), .release);
            _ = gen.fetchAdd(1, .release);
            std.Thread.Futex.wake(&gen, std.math.maxInt(u32));
            drain();
            while (true) {
                const p = pending.load(.acquire);
                if (p == 0) break;
                std.Thread.Futex.wait(&pending, p);
            }
        } else range(0, n);
        var sum: i64 = 0;
        var failed = false;
        for (out) |x| {
            if (!x.ok) {
                s = @mod(s0 + x.v, P);
                failed = true;
                break;
            }
            sum = @mod(sum + x.v, P);
        }
        if (!failed) s = @mod(s0 + sum, P);
    }
    if (isHand) {
        quit.store(true, .release);
        _ = gen.fetchAdd(1, .release);
        std.Thread.Futex.wake(&gen, std.math.maxInt(u32));
        for (0..W - 1) |k| threads[k].join();
    }
    if (isStd) pool.deinit();
    try std.io.getStdOut().writer().print("{d}\n", .{s});
}
