fun main() {
    var i = 0L
    var acc = 0L
    while (i < 30000000) {
        acc = (acc + i * 7) % 999983
        i += 1
    }
    println(acc)
}
