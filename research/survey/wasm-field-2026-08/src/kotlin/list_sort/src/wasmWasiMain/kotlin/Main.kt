fun mk(n: Long): MutableList<Long> {
    val out = mutableListOf<Long>()
    var i = 0L
    while (i < n) {
        out.add(i * 7919 % 10007)
        i += 1
    }
    return out
}

fun main() {
    val xs = mk(2000)
    var acc = 0L
    var r = 0L
    while (r < 300) {
        val s = xs.sorted()
        acc = acc + s[0] + s[1999]
        r += 1
    }
    println(acc)
}
