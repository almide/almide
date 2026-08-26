package main

import "fmt"

func main() {
	out := []int64{}
	var i int64 = 0
	for i < 2000 {
		out = append(out, i)
		i++
	}
	var acc int64 = 0
	var r int64 = 0
	for r < 2000 {
		m := make([]int64, 0, len(out))
		for _, x := range out {
			m = append(m, x*3+1)
		}
		f := []int64{}
		for _, x := range m {
			if x%2 == 0 {
				f = append(f, x)
			}
		}
		var v int64 = 0
		for _, x := range f {
			v = (v + x) % 999983
		}
		acc = (acc + v) % 999983
		r++
	}
	fmt.Println(acc)
}
