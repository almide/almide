package main

import "fmt"

func main() {
	var i int64 = 0
	x := 1.5
	for i < 20000000 {
		x = x*1.0000001 + 0.0000003
		i++
	}
	fmt.Println(x)
}
