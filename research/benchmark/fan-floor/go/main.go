// fan floor probe: usage  floor <seq|par> <sweep|nest|busy> <iters> <offers>
package main

import (
	"fmt"
	"os"
	"strconv"
	"sync"
)

const P = 1000003

func work(s, i, iters int64) int64 {
	acc := s + i
	for j := int64(0); j < iters; j++ {
		acc = (acc*31 + j) % P
	}
	return acc
}

var par bool

// one offer: n elements, each its own goroutine (the idiomatic Go fan-out)
func fanMap(n int, f func(i int64) int64) []int64 {
	out := make([]int64, n)
	if !par {
		for i := 0; i < n; i++ {
			out[i] = f(int64(i))
		}
		return out
	}
	var wg sync.WaitGroup
	wg.Add(n)
	for i := 0; i < n; i++ {
		go func(i int) { out[i] = f(int64(i)); wg.Done() }(i)
	}
	wg.Wait()
	return out
}

func sum(xs []int64) int64 {
	var a int64
	for _, x := range xs {
		a += x
	}
	return a
}

func step(s, iters int64) []int64 { return fanMap(10, func(i int64) int64 { return work(s, i, iters) }) }

func lane(seed, offers, iters int64) int64 {
	s := seed
	for k := int64(0); k < offers; k++ {
		s = (s + sum(step(s, iters))) % P
	}
	return s
}

func main() {
	par = os.Args[1] == "par"
	probe := os.Args[2]
	iters, _ := strconv.ParseInt(os.Args[3], 10, 64)
	offers, _ := strconv.ParseInt(os.Args[4], 10, 64)
	switch probe {
	case "sweep":
		fmt.Println(lane(1, offers, iters))
	case "nest":
		s := int64(1)
		for k := int64(0); k < offers; k++ {
			ys := fanMap(4, func(q int64) int64 {
				var a int64
				for _, x := range step(s+q, iters) {
					a = (a + x) % P
				}
				return a
			})
			s = (s + sum(ys)) % P
		}
		fmt.Println(s)
	case "busy":
		var a, b int64
		if par {
			var wg sync.WaitGroup
			wg.Add(2)
			go func() { a = lane(1, offers, iters); wg.Done() }()
			go func() { b = lane(2, offers, iters); wg.Done() }()
			wg.Wait()
		} else {
			a, b = lane(1, offers, iters), lane(2, offers, iters)
		}
		fmt.Println(a, b)
	}
}
