#!/usr/bin/env bash
# Hidden oracle for edit 01 — never copied into the model's tree. cwd = the program tree.
pass=0; fail=0
check() { # name, stdin (printf %b), expected stdout
  local got; got="$(printf '%b' "$2" | ./app 2>&1)"
  if [ "$got" = "$(printf '%b' "$3")" ]; then pass=$((pass+1)); else fail=$((fail+1)); echo "FAIL $1: expected [$(printf '%b' "$3")] got [$got]"; fi
}
check "sub after add"      'add 5\nsub 2\nshow\n'            '3'
check "sub goes negative"  'sub 5\nshow\n'                   '-5'
check "negative then back" 'add 1\nsub 3\nadd 2\nshow\n'     '0'
check "no clamping"        'sub 1\nsub 1\nshow\nadd 5\nshow\n' '-2\n3'
check "sub bad amount"     'sub y\nshow\n'                   'error: bad amount y\n0'
check "unknown untouched"  'foo\nshow\n'                     'error: unknown command foo\n0'
echo "PASSED: $pass"
echo "FAILED: $fail"
[ "$fail" -eq 0 ]
