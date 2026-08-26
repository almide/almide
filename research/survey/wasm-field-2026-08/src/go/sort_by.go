package main

import (
	"cmp"
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
		slices.SortStableFunc(s, func(a, b int64) int {
			return cmp.Compare(-a, -b)
		})
		acc = acc + s[0] + s[1999]
		r++
	}
	fmt.Println(acc)
}
