// identical source is compiled by both the go (mainline) and tinygo lanes
package main

import (
	"fmt"
	"strconv"
)

func main() {
	var i int64 = 0
	var n int64 = 0
	for i < 3000000 {
		s := "ab" + strconv.FormatInt(i%100, 10)
		n += int64(len(s))
		i++
	}
	fmt.Println(n)
}
