set +e
echo "=== clippy workspace"
bash scripts/check-clippy-warnings.sh 2>&1 | tail -60
echo "=== parity goldens"
ORACLE=target/release/almide bash scripts/check-parity-goldens.sh > /tmp/pg.txt 2>&1; tail -5 /tmp/pg.txt
git diff > /tmp/cired-out-goldens.patch; git diff --stat
