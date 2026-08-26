fun main() {
    val out = mutableListOf<Long>()
    var i = 0L
    while (i < 2000) {
        out.add(i)
        i += 1
    }
    var acc = 0L
    var r = 0L
    while (r < 2000) {
        val m = out.map { it * 3 + 1 }
        val f = m.filter { it % 2 == 0L }
        val v = f.fold(0L) { a, x -> (a + x) % 999983 }
        acc = (acc + v) % 999983
        r += 1
    }
    println(acc)
}
