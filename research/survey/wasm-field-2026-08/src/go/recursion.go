package main

import "fmt"

func run(n int64, acc int64) int64 {
	if n <= 0 {
		return acc
	}
	return run(n-1, (acc+n)%999983)
}

func main() {
	var r int64 = 0
	var acc int64 = 0
	for r < 30 {
		acc = (acc + run(1000000, 0)) % 999983
		r++
	}
	fmt.Println(acc)
}
