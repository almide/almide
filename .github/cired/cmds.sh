set +e
D=.github/cired/src
python3 .github/cired/split.py $D/oracle_strs.almd
for k in $(python3 .github/cired/split.py $D/oracle_strs.almd | grep "^LEAK" | awk '{print $2}' | head -3); do
  CIRED_SRCS=/tmp/split/b$k.almd cargo test --release --test zz_cired_render -- --nocapture 2>&1 | sed -n '/^fn main()/,/^}/p' > /tmp/cired-out-leak$k.txt
  cp /tmp/split/b$k.almd /tmp/cired-out-leak$k.almd
done
echo "=== accum"
cargo test --release --test accumulate_in_place_test 2>&1 | grep -E "^test |test result|panicked|took" | head
CIRED_SRCS=$D/accum.almd cargo test --release --test zz_cired_render -- --nocapture 2>&1 | grep -A8 "^fn almd_interp"
echo "=== push"
cargo test --release --test string_push_native_append_test 2>&1 | grep -E "^test |test result" | head
