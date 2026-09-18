#!/usr/bin/env bash
# Visible suite for edit 02 (the model may read and run this). cwd = the program tree.
pass=0; fail=0
check() { # name, stdin (printf %b), expected stdout
  local got; got="$(printf '%b' "$2" | ./app 2>&1)"
  if [ "$got" = "$(printf '%b' "$3")" ]; then pass=$((pass+1)); else fail=$((fail+1)); echo "FAIL $1: expected [$(printf '%b' "$3")] got [$got]"; fi
}
check "print after add" 'add 5\nprint\n'            '5'
check "print after sub" 'add 5\nsub 7\nprint\n'     '-2'
check "print twice"     'add 1\nprint\nadd 1\nprint\n' '1\n2'
echo "PASSED: $pass"
echo "FAILED: $fail"
[ "$fail" -eq 0 ]
