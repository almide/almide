package main

import "fmt"

func main() {
	var i int64 = 0
	var acc int64 = 0
	for i < 30000000 {
		acc = (acc + i*7) % 999983
		i++
	}
	fmt.Println(acc)
}
