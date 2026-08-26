fun main() {
    var i = 0L
    var x = 1.5
    while (i < 20000000) {
        x = x * 1.0000001 + 0.0000003
        i += 1
    }
    println(x)
}
