tailrec fun go(n: Long, acc: Long): Long =
    if (n <= 0) acc else go(n - 1, (acc + n) % 999983)

fun main() {
    var r = 0L
    var acc = 0L
    while (r < 30) {
        acc = (acc + go(1000000, 0)) % 999983
        r += 1
    }
    println(acc)
}
