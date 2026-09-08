#!/usr/bin/env bash
# Hidden oracle for edit 02 — never copied into the model's tree. cwd = the program tree.
pass=0; fail=0
check() { # name, stdin (printf %b), expected stdout
  local got; got="$(printf '%b' "$2" | ./app 2>&1)"
  if [ "$got" = "$(printf '%b' "$3")" ]; then pass=$((pass+1)); else fail=$((fail+1)); echo "FAIL $1: expected [$(printf '%b' "$3")] got [$got]"; fi
}
check "print after add"     'add 5\nprint\n'                 '5'
check "show is gone"        'add 5\nshow\nprint\n'           'error: unknown command show\n5'
check "negative preserved"  'sub 3\nprint\n'                 '-3'
check "bad amount preserved" 'add q\nprint\n'                'error: bad amount q\n0'
check "unknown preserved"   'foo\nprint\n'                   'error: unknown command foo\n0'
echo "PASSED: $pass"
echo "FAILED: $fail"
[ "$fail" -eq 0 ]
