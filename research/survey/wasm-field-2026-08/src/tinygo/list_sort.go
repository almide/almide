// identical source is compiled by both the go (mainline) and tinygo lanes
package main

import (
	"fmt"
	"slices"
)

func mk(n int64) []int64 {
	out := []int64{}
	var i int64 = 0
	for i < n {
		out = append(out, (i*7919)%10007)
		i++
	}
	return out
}

func main() {
	xs := mk(2000)
	var acc int64 = 0
	var r int64 = 0
	for r < 300 {
		s := slices.Clone(xs)
		slices.Sort(s)
		acc = acc + s[0] + s[1999]
		r++
	}
	fmt.Println(acc)
}
