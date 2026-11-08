set +e
bash scripts/check-clippy-warnings.sh 2>&1 | tail -5
