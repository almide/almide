set +e
almide run tools/almide-gates/src/main.almd -- impl-count . --update 2>&1 | tail -5
git diff > /tmp/cired-out-impl.patch; git diff --stat
almide run tools/almide-gates/src/main.almd -- impl-count . 2>&1 | tail -3
