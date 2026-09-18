#!/usr/bin/env bash
# Visible suite for edit 01 (the model may read and run this). cwd = the program tree.
pass=0; fail=0
check() { # name, stdin (printf %b), expected stdout
  local got; got="$(printf '%b' "$2" | ./app 2>&1)"
  if [ "$got" = "$(printf '%b' "$3")" ]; then pass=$((pass+1)); else fail=$((fail+1)); echo "FAIL $1: expected [$(printf '%b' "$3")] got [$got]"; fi
}
check "sub after add"   'add 5\nsub 2\nshow\n'          '3'
check "sub to zero"     'add 4\nsub 4\nshow\n'          '0'
check "add still works" 'add 1\nadd 2\nshow\n'          '3'
check "sub bad amount"  'add 1\nsub x\nshow\n'          'error: bad amount x\n1'
echo "PASSED: $pass"
echo "FAILED: $fail"
[ "$fail" -eq 0 ]
