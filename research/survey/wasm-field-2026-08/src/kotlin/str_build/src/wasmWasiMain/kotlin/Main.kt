fun main() {
    var i = 0L
    var n = 0L
    while (i < 3000000) {
        val s = "ab" + (i % 100).toString()
        n += s.length
        i += 1
    }
    println(n)
}
