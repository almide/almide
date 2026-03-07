#!/usr/bin/env bash
set -e

PASS_COUNT=0
FAIL_COUNT=0

fail() {
  echo "FAIL: $1"
  FAIL_COUNT=$((FAIL_COUNT+1))
}

pass() {
  echo "PASS: $1"
  PASS_COUNT=$((PASS_COUNT+1))
}

cleanup() {
  cd "$(dirname "$0")"
  rm -rf testrepo
}

# Build if needed
cd "$(dirname "$0")"

if [ -f Makefile ] || [ -f makefile ]; then
  make -s 2>/dev/null || true
fi
if [ -f build.sh ]; then
  bash build.sh 2>/dev/null || true
fi
chmod +x minigit 2>/dev/null || true

######################################
# Setup
######################################

cleanup
mkdir testrepo
cd testrepo

######################################
# Test 1: init creates directory
######################################

if ../minigit init && [ -d .minigit ] && [ -d .minigit/objects ] && [ -d .minigit/commits ]; then
  pass "init creates .minigit directory"
else
  fail "init creates .minigit directory"
fi

######################################
# Test 2: init duplicate
######################################

if ../minigit init 2>&1 | grep -q "Repository already initialized"; then
  pass "init duplicate prints message"
else
  fail "init duplicate prints message"
fi

######################################
# Test 3: add stages file
######################################

echo "hello" > a.txt

if ../minigit add a.txt && grep -q "a.txt" .minigit/index && [ "$(ls .minigit/objects/ | wc -l)" -ge 1 ]; then
  pass "add stages file and creates blob"
else
  fail "add stages file and creates blob"
fi

######################################
# Test 4: add nonexistent file
######################################

if ../minigit add nonexistent.txt 2>&1 | grep -q "File not found"; then
  # Also verify it exits non-zero
  if ! ../minigit add nonexistent.txt >/dev/null 2>&1; then
    pass "add nonexistent file fails with message"
  else
    fail "add nonexistent file fails with message"
  fi
else
  fail "add nonexistent file fails with message"
fi

######################################
# Test 5: commit works
######################################

# Re-add to ensure index is populated (add from test 3 should suffice, but be safe)
../minigit add a.txt 2>/dev/null || true

if ../minigit commit -m "first" && [ -s .minigit/HEAD ] && [ "$(ls .minigit/commits/ | wc -l)" -ge 1 ]; then
  pass "commit creates commit and updates HEAD"
else
  fail "commit creates commit and updates HEAD"
fi

COMMIT1=$(cat .minigit/HEAD)

######################################
# Test 6: commit with empty index
######################################

if ../minigit commit -m "empty" 2>&1 | grep -q "Nothing to commit"; then
  if ! ../minigit commit -m "empty" >/dev/null 2>&1; then
    pass "commit with empty index fails"
  else
    fail "commit with empty index fails"
  fi
else
  fail "commit with empty index fails"
fi

######################################
# Prepare second commit for log tests
######################################

echo "world" > b.txt
../minigit add b.txt 2>/dev/null
../minigit commit -m "second" >/dev/null 2>&1
COMMIT2=$(cat .minigit/HEAD)

######################################
# Test 7: log shows commits
######################################

if ../minigit log | grep -q "$COMMIT1" && ../minigit log | grep -q "$COMMIT2"; then
  pass "log contains both commits"
else
  fail "log contains both commits"
fi

######################################
# Test 8: log order (most recent first)
######################################

FIRST_IN_LOG=$(../minigit log | grep "^commit " | head -1 | awk '{print $2}')

if [ "$FIRST_IN_LOG" = "$COMMIT2" ]; then
  pass "log shows most recent commit first"
else
  fail "log shows most recent commit first"
fi

######################################
# Test 9: log with no commits
######################################

mkdir -p ../emptyrepo && cd ../emptyrepo
if ../minigit init >/dev/null 2>&1 && ../minigit log 2>&1 | grep -q "No commits"; then
  pass "log with no commits"
else
  fail "log with no commits"
fi
cd ../testrepo
rm -rf ../emptyrepo

######################################
# Test 10: hash consistency
######################################

OBJ_COUNT_BEFORE=$(ls .minigit/objects/ | wc -l)
echo "deterministic" > d1.txt
../minigit add d1.txt >/dev/null 2>&1
OBJ_COUNT_AFTER1=$(ls .minigit/objects/ | wc -l)

echo "deterministic" > d2.txt
../minigit add d2.txt >/dev/null 2>&1
OBJ_COUNT_AFTER2=$(ls .minigit/objects/ | wc -l)

if [ "$OBJ_COUNT_AFTER1" -gt "$OBJ_COUNT_BEFORE" ] && [ "$OBJ_COUNT_AFTER2" = "$OBJ_COUNT_AFTER1" ]; then
  pass "hash consistency: same content produces same blob"
else
  fail "hash consistency: same content produces same blob"
fi

######################################
# Test 11: hash accuracy (MiniHash)
######################################

echo "test hash" > hashtest.txt
../minigit add hashtest.txt >/dev/null 2>&1

EXPECTED_HASH=$(python3 -c "
data = open('hashtest.txt','rb').read()
h = 1469598103934665603
for b in data:
    h ^= b
    h = (h * 1099511628211) % (2**64)
print(format(h, '016x'))
")

if [ -f ".minigit/objects/$EXPECTED_HASH" ]; then
  pass "hash accuracy: MiniHash matches reference"
else
  fail "hash accuracy: MiniHash matches reference (expected $EXPECTED_HASH, got: $(ls .minigit/objects/))"
fi

######################################
# Cleanup & Summary
######################################

cd ..
rm -rf testrepo

echo ""
echo "========================"
echo "PASSED: $PASS_COUNT"
echo "FAILED: $FAIL_COUNT"
echo "TOTAL:  $((PASS_COUNT + FAIL_COUNT))"
echo "========================"

if [ "$FAIL_COUNT" -eq 0 ]; then
  echo "ALL TESTS PASSED"
  exit 0
else
  exit 1
fi
