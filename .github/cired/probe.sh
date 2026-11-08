# usage: probe.sh <almide-bin>; prints one verdict line per program
B=$1; P=.github/cired/progs; T=$(mktemp -d)
for f in $P/*.almd; do
  n=$(basename $f .almd); cp $f $T/$n.almd
  nat=$(cd $T && timeout 300 $B run $n.almd 2>$T/$n.err); ne=$?
  was=$(cd $T && timeout 300 $B run $n.almd --target wasm 2>/dev/null); we=$?
  if [ $ne -ne 0 ]; then v="NATIVE-FAIL($(grep -m1 -o 'error\[E[0-9]*\][^`]*`[^`]*`' $T/$n.err))"; elif [ "$nat" != "$was" ]; then v="DIFF"; else v="same"; fi
  echo "VERDICT $n: $v"
  [ "$v" = same ] || { echo "  native: $(echo "$nat" | tail -2 | tr '\n' '|')"; echo "  wasm:   $(echo "$was" | tail -2 | tr '\n' '|')"; }
done
