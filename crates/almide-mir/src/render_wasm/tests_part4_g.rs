
    #[test]
    fn self_hosted_set_map_fold() {
        // SELF-HOSTED set.map (result re-dedup — f may collapse distinct inputs) + set.fold
        // (2-arity closure, acc-first). s={1,2,3,4}: map(x => x % 2)={1,0} len 2 sum 1 (1,0,1,0
        // dedups to {1,0}); fold(0, (acc,x) => acc + x)=10. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let s = set.from_list([1, 2, 3, 4])\n  \
            let m = set.map(s, (x) => x % 2)\n  println(int.to_string(set.len(m)))\n  \
            let ml = set.to_list(m)\n  println(int.to_string(list.sum(ml)))\n  \
            let total = set.fold(s, 0, (acc, x) => acc + x)\n  println(int.to_string(total)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.map"));
        assert!(prog.functions.iter().any(|f| f.name == "set.fold"));
        if let Some(out) = build_and_run("self_hosted_set_map_fold", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n1\n10");
        }
    }

    #[test]
    fn self_hosted_map_core() {
        // SELF-HOSTED Map[Int,Int] core (the Map machinery: a v1 Map IS a v1 List of PAIRED
        // 16-byte slots [k,v,...]; prim.alloc_map + insertion-order set/remove + len-patch). v0:
        // new→set(1,10)→set(2,20)→set(1,99) [update in place] = {1:99, 2:20} len 2; get_or(2,0)=20,
        // get_or(9,-1)=-1; contains(1)=true; remove(1)={2:20} len 1; keys sum 1+2=3 before remove;
        // values sum 99+20=119. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m0 = map.new()\n  let m1 = map.set(m0, 1, 10)\n  let m2 = map.set(m1, 2, 20)\n  \
            let m = map.set(m2, 1, 99)\n  \
            println(int.to_string(map.len(m)))\n  \
            println(int.to_string(map.get_or(m, 2, 0)))\n  \
            println(int.to_string(map.get_or(m, 9, 0 - 1)))\n  \
            let c1 = map.contains(m, 1)\n  let vc1 = if c1 then 1 else 0\n  println(int.to_string(vc1))\n  \
            let ks = map.keys(m)\n  println(int.to_string(list.sum(ks)))\n  \
            let vs = map.values(m)\n  println(int.to_string(list.sum(vs)))\n  \
            let r = map.remove(m, 1)\n  println(int.to_string(map.len(r)))\n  \
            println(int.to_string(map.get_or(r, 2, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.set"));
        if let Some(out) = build_and_run("self_hosted_map_core", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n20\n-1\n1\n3\n119\n1\n20");
        }
    }

    #[test]
    fn self_hosted_map_get_merge_update() {
        // SELF-HOSTED map.get (materialized Option[Int], match-extractable) + map.merge (b
        // overrides a's values, appends new keys) + map.update (closure on present key). m={1:10,
        // 2:20}: get(2)=Some(20) match→20, get(9)=None match→-1; merge with {2:99,3:30}={1:10,2:99,
        // 3:30} get(2)=99 get(3)=30; update(1, x=>x*5)={1:50,...} get(1)=50. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m1 = map.set(map.new(), 1, 10)\n  let m = map.set(m1, 2, 20)\n  \
            let g2 = map.get(m, 2)\n  \
            match g2 { Some(v) => println(int.to_string(v)), None => println(\"none\"), }\n  \
            let g9 = map.get(m, 9)\n  \
            match g9 { Some(v) => println(int.to_string(v)), None => println(\"none\"), }\n  \
            let b1 = map.set(map.new(), 2, 99)\n  let b = map.set(b1, 3, 30)\n  \
            let mg = map.merge(m, b)\n  \
            println(int.to_string(map.len(mg)))\n  \
            println(int.to_string(map.get_or(mg, 2, 0)))\n  \
            println(int.to_string(map.get_or(mg, 3, 0)))\n  \
            let mu = map.update(m, 1, (x) => x * 5)\n  \
            println(int.to_string(map.get_or(mu, 1, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.get"));
        assert!(prog.functions.iter().any(|f| f.name == "map.merge"));
        if let Some(out) = build_and_run("self_hosted_map_get_merge_update", &render_wasm_program(&prog)) {
            assert_eq!(out, "20\nnone\n3\n99\n30\n50");
        }
    }

    #[test]
    fn self_hosted_map_higher_order() {
        // SELF-HOSTED map.filter/all/any/count over Map[Int,Int] (closures machinery — a 2-arity
        // predicate f(k,v) via CallIndirect). m={1:10,2:20,3:30}: filter(v>15)={2:20,3:30} len 2
        // values sum 50; all(v>0)=true; any(k==2)=true; count(v>=20)=2. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m1 = map.set(map.new(), 1, 10)\n  let m2 = map.set(m1, 2, 20)\n  \
            let m = map.set(m2, 3, 30)\n  \
            let fm = map.filter(m, (k, v) => v > 15)\n  println(int.to_string(map.len(fm)))\n  \
            let fv = map.values(fm)\n  println(int.to_string(list.sum(fv)))\n  \
            let ap = map.all(m, (k, v) => v > 0)\n  let va = if ap then 1 else 0\n  println(int.to_string(va))\n  \
            let an = map.any(m, (k, v) => k == 2)\n  let vn = if an then 1 else 0\n  println(int.to_string(vn))\n  \
            let cn = map.count(m, (k, v) => v >= 20)\n  println(int.to_string(cn)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.filter"));
        if let Some(out) = build_and_run("self_hosted_map_higher_order", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n50\n1\n1\n2");
        }
    }

    #[test]
    fn self_hosted_map_fold_three_arity_closure() {
        // SELF-HOSTED map.fold — the FIRST 3-arity closure f(acc,k,v) via $closure_fn3 (the render
        // auto-generates a closure type per arity, so 3-arity needs no new machinery). m={1:10,
        // 2:20,3:30}: fold(0, (a,k,v) => a + k + v) = (1+10)+(2+20)+(3+30) = 66. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m1 = map.set(map.new(), 1, 10)\n  let m2 = map.set(m1, 2, 20)\n  \
            let m = map.set(m2, 3, 30)\n  \
            let total = map.fold(m, 0, (a, k, v) => a + k + v)\n  \
            println(int.to_string(total)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.fold"));
        if let Some(out) = build_and_run("self_hosted_map_fold_three_arity_closure", &render_wasm_program(&prog)) {
            assert_eq!(out, "66");
        }
    }

    #[test]
    fn self_hosted_value_scalar_constructors() {
        // SELF-HOSTED scalar Value constructors (the dynamic data model: a v1 block whose `len`
        // header field carries the variant TAG, payload at +12). value.int(42)→tag 2 payload 42;
        // value.null()→tag 0; value.bool(true)→tag 1 payload 1; value.float(1.0)→tag 3 payload
        // 0x3FF...=4607182418800017408 (f64 bits). Verified by reading the block via prim.
        let src = "fn main() -> Unit = {\n  \
            let vi = value.int(42)\n  let hi = prim.handle(vi)\n  \
            println(int.to_string(prim.load32(hi + 4)))\n  println(int.to_string(prim.load64(hi + 12)))\n  \
            let vn = value.null()\n  println(int.to_string(prim.load32(prim.handle(vn) + 4)))\n  \
            let vb = value.bool(true)\n  let hb = prim.handle(vb)\n  \
            println(int.to_string(prim.load32(hb + 4)))\n  println(int.to_string(prim.load64(hb + 12)))\n  \
            let vf = value.float(1.0)\n  let hf = prim.handle(vf)\n  \
            println(int.to_string(prim.load32(hf + 4)))\n  println(int.to_string(prim.load64(hf + 12))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "value.int"));
        if let Some(out) = build_and_run("self_hosted_value_scalar_constructors", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n42\n0\n1\n1\n3\n4607182418800017408");
        }
    }

    #[test]
    fn self_hosted_value_scalar_extractors() {
        // SELF-HOSTED value.as_int/as_bool/as_float → Result[T, String] (read the tag, Ok(payload)
        // on a match else Err("expected T"); materialized Result, match-executable). as_int(int 42)=
        // Ok(42); as_int(null)=Err("expected Int"); as_bool(bool true)=Ok(true)→"yes"; as_float(int 7)
        // widens to Ok(7.0)="float-ok"; as_float(null)=Err("expected Float"). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let oi = value.as_int(value.int(42))\n  \
            match oi { Ok(n) => println(int.to_string(n)), Err(e) => println(e), }\n  \
            let oi2 = value.as_int(value.null())\n  \
            match oi2 { Ok(n) => println(int.to_string(n)), Err(e) => println(e), }\n  \
            let ob = value.as_bool(value.bool(true))\n  \
            match ob { Ok(b) => if b then println(\"yes\") else println(\"no\"), Err(e) => println(e), }\n  \
            let of = value.as_float(value.int(7))\n  \
            match of { Ok(f) => println(\"float-ok\"), Err(e) => println(e), }\n  \
            let of2 = value.as_float(value.null())\n  \
            match of2 { Ok(f) => println(\"float-ok\"), Err(e) => println(e), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "value.as_int"));
        if let Some(out) = build_and_run("self_hosted_value_scalar_extractors", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\nexpected Int\nyes\nfloat-ok\nexpected Float");
        }
    }

    #[test]
    fn self_hosted_set_string() {
        // SELF-HOSTED Set[String] (heap-element / nested-ownership set, DynListStr; dedup via byte
        // string equality __str_eq; deep-copied elements). The repr-poly dispatch routes set.from_list
        // /contains/to_list over a Set[String] to the _str variant. from_list(["apple","banana",
        // "apple","cherry","banana"])={apple,banana,cherry} len 3; contains("banana")=true,
        // ("grape")=false; to_list first="apple". Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"apple,banana,apple,cherry,banana\", \",\")\n  \
            let s = set.from_list(xs)\n  println(int.to_string(set.len(s)))\n  \
            let ca = set.contains(s, \"banana\")\n  if ca then println(\"has-banana\") else println(\"no\")\n  \
            let cb = set.contains(s, \"grape\")\n  if cb then println(\"has-grape\") else println(\"no\")\n  \
            let ys = set.to_list(s)\n  println(int.to_string(list.len(ys)))\n  \
            let first = list.get(ys, 0)\n  match first { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.from_list_str"));
        assert!(prog.functions.iter().any(|f| f.name == "set.contains_str"));
        if let Some(out) = build_and_run("self_hosted_set_string", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\nhas-banana\nno\n3\napple");
        }
    }

    #[test]
    fn self_hosted_set_string_algebra() {
        // SELF-HOSTED Set[String] union/intersection/difference/is_subset/is_disjoint (heap-element
        // algebra, deep-copied results, __str_eq membership; repr-poly dispatch to the _str variant).
        // a={a,b,c,d} b={c,d,e,f}: union len 6; intersection={c,d} len 2 first "c"; difference(a,b)=
        // {a,b} len 2; is_subset({c,d},a)=true; is_disjoint(a,b)=false. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a = set.from_list(string.split(\"a,b,c,d\", \",\"))\n  \
            let b = set.from_list(string.split(\"c,d,e,f\", \",\"))\n  \
            let u = set.union(a, b)\n  println(int.to_string(set.len(u)))\n  \
            let inter = set.intersection(a, b)\n  println(int.to_string(set.len(inter)))\n  \
            let il = set.to_list(inter)\n  let f = list.get(il, 0)\n  \
            match f { Some(v) => println(v), None => println(\"none\"), }\n  \
            let diff = set.difference(a, b)\n  println(int.to_string(set.len(diff)))\n  \
            let sub = set.is_subset(inter, a)\n  if sub then println(\"subset\") else println(\"no\")\n  \
            let dj = set.is_disjoint(a, b)\n  if dj then println(\"disjoint\") else println(\"no\") }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.union_str"));
        assert!(prog.functions.iter().any(|f| f.name == "set.is_subset_str"));
        if let Some(out) = build_and_run("self_hosted_set_string_algebra", &render_wasm_program(&prog)) {
            assert_eq!(out, "6\n2\nc\n2\nsubset\nno");
        }
    }

    #[test]
    fn self_hosted_set_string_construction() {
        // SELF-HOSTED Set[String] new/insert/remove/symmetric_difference (heap-element, deep copies).
        // new()→insert "a","b","a"(dup)={a,b} len 2; remove "a"={b} len 1 first "b"; sym_diff(
        // {a,b,c},{b,c,d})={a,d} len 2. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let e = set.new()\n  let s1 = set.insert(e, \"a\")\n  let s2 = set.insert(s1, \"b\")\n  \
            let s = set.insert(s2, \"a\")\n  println(int.to_string(set.len(s)))\n  \
            let r = set.remove(s, \"a\")\n  println(int.to_string(set.len(r)))\n  \
            let rl = set.to_list(r)\n  let f = list.get(rl, 0)\n  \
            match f { Some(v) => println(v), None => println(\"none\"), }\n  \
            let a = set.from_list(string.split(\"a,b,c\", \",\"))\n  \
            let b = set.from_list(string.split(\"b,c,d\", \",\"))\n  \
            let sd = set.symmetric_difference(a, b)\n  println(int.to_string(set.len(sd))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.insert_str"));
        if let Some(out) = build_and_run("self_hosted_set_string_construction", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n1\nb\n2");
        }
    }

    #[test]
    fn self_hosted_set_string_higher_order() {
        // SELF-HOSTED Set[String] filter/all/any/fold (closures over String elements, heap-arg ABI).
        // s={apple,banana,kiwi,fig}: filter(len>3)={apple,banana,kiwi} len 3; all(len>2)=true;
        // any(len<4)=true (fig=3); fold(0, +len)=5+6+4+3=18. Byte-matches v0. Closure bodies use a
        // let-bind (scalar-call-as-operand gap workaround).
        let src = "fn main() -> Unit = {\n  \
            let s = set.from_list(string.split(\"apple,banana,kiwi,fig\", \",\"))\n  \
            let big = set.filter(s, (x) => { let l = string.len(x)\n l > 3 })\n  \
            println(int.to_string(set.len(big)))\n  \
            let al = set.all(s, (x) => { let l = string.len(x)\n l > 2 })\n  \
            if al then println(\"all-long\") else println(\"no\")\n  \
            let an = set.any(s, (x) => { let l = string.len(x)\n l < 4 })\n  \
            if an then println(\"has-short\") else println(\"no\")\n  \
            let total = set.fold(s, 0, (acc, x) => { let l = string.len(x)\n acc + l })\n  \
            println(int.to_string(total)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.filter_str"));
        assert!(prog.functions.iter().any(|f| f.name == "set.fold_str"));
        if let Some(out) = build_and_run("self_hosted_set_string_higher_order", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\nall-long\nhas-short\n18");
        }
    }

    #[test]
    fn self_hosted_list_string_search() {
        // SELF-HOSTED list.contains / list.index_of over a List[String] (byte string equality
        // __str_eq; arg-keyed dispatch). Also exercises set.contains_str in the SAME program so the
        // duplicate __str_eq (in list_str.almd + set_str.almd) must DEDUP by name (no "duplicate
        // func"). xs=[a,b,c,d]: contains("c")=true, contains("z")=false; index_of("c")=Some(2),
        // index_of("z")=None; set.contains over the same elements = true. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"a,b,c,d\", \",\")\n  \
            let c1 = list.contains(xs, \"c\")\n  if c1 then println(\"has-c\") else println(\"no\")\n  \
            let c2 = list.contains(xs, \"z\")\n  if c2 then println(\"has-z\") else println(\"no\")\n  \
            let i1 = list.index_of(xs, \"c\")\n  \
            match i1 { Some(v) => println(int.to_string(v)), None => println(\"none\"), }\n  \
            let i2 = list.index_of(xs, \"z\")\n  \
            match i2 { Some(v) => println(int.to_string(v)), None => println(\"none\"), }\n  \
            let s = set.from_list(xs)\n  let sc = set.contains(s, \"b\")\n  \
            if sc then println(\"set-has-b\") else println(\"no\") }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.contains_str"));
        assert!(prog.functions.iter().any(|f| f.name == "list.index_of_str"));
        if let Some(out) = build_and_run("self_hosted_list_string_search", &render_wasm_program(&prog)) {
            assert_eq!(out, "has-c\nno\n2\nnone\nset-has-b");
        }
    }

    #[test]
    fn self_hosted_list_string_higher_order() {
        // SELF-HOSTED list.all/any/count over a List[String] (predicate over String elements,
        // heap-arg closure; arg-keyed dispatch). xs=[apple,banana,kiwi,fig]: all(len>2)=true;
        // any(len<4)=true (fig); count(len>4)=2 (banana=6,apple=5). Byte-matches v0. Closure body
        // uses a let-bind for the scalar call.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"apple,banana,kiwi,fig\", \",\")\n  \
            let al = list.all(xs, (x) => { let l = string.len(x)\n l > 2 })\n  \
            if al then println(\"all-long\") else println(\"no\")\n  \
            let an = list.any(xs, (x) => { let l = string.len(x)\n l < 4 })\n  \
            if an then println(\"has-short\") else println(\"no\")\n  \
            let cn = list.count(xs, (x) => { let l = string.len(x)\n l > 4 })\n  \
            println(int.to_string(cn)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.count_str"));
        if let Some(out) = build_and_run("self_hosted_list_string_higher_order", &render_wasm_program(&prog)) {
            assert_eq!(out, "all-long\nhas-short\n2");
        }
    }

    #[test]
    fn self_hosted_list_string_fold() {
        // SELF-HOSTED list.fold over a List[String] — f(acc, x) 2-arity closure, String element 2nd
        // arg (heap-widened). xs=[ab,cde,f]: fold(0, acc + len(x)) = 2+3+1 = 6. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"ab,cde,f\", \",\")\n  \
            let total = list.fold(xs, 0, (acc, x) => { let l = string.len(x)\n acc + l })\n  \
            println(int.to_string(total)) }\n";
        let prog = lower_source(src);
        // The BLOCK-bodied lambda now DEFUNCTIONALIZES (see self_hosted_list_filter_str).
        if let Some(out) = build_and_run("self_hosted_list_string_fold", &render_wasm_program(&prog)) {
            assert_eq!(out, "6");
        }
    }

    #[test]
    fn self_hosted_list_string_unique() {
        // SELF-HOSTED list.unique over a List[String] — keep first occurrence, drop all later dups
        // (membership vs the result via __str_eq; kept elements deep-copied). xs=[a,b,a,c,b,a]=
        // [a,b,c] len 3, first "a", sum-of-lens 3. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"a,b,a,c,b,a\", \",\")\n  \
            let u = list.unique(xs)\n  println(int.to_string(list.len(u)))\n  \
            let f = list.get(u, 0)\n  match f { Some(v) => println(v), None => println(\"none\"), }\n  \
            let last = list.get(u, 2)\n  match last { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.unique_str"));
        if let Some(out) = build_and_run("self_hosted_list_string_unique", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\na\nc");
        }
    }

    #[test]
    fn self_hosted_list_string_unique_loop_reclaims() {
        // SOUNDNESS for list.unique_str: a bounded loop building + dropping a fresh deduped
        // List[String] each iteration must reclaim every kept element + the block — no leak/double-
        // free. 3000 iters, prints the last result's len (2).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 3000 {\n    \
              let u = list.unique(string.split(\"xx,yy,xx,yy\", \",\"))\n    \
              last = list.len(u)\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_list_string_unique_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "2");
        }
    }

    #[test]
    fn self_hosted_list_string_dedup() {
        // SELF-HOSTED list.dedup over a List[String] — drop CONSECUTIVE duplicates (cur vs source
        // i-1 via __str_eq; kept elements deep-copied). xs=[a,a,b,b,b,a,c]=[a,b,a,c] len 4, first
        // "a", elem 2 "a", last "c". Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"a,a,b,b,b,a,c\", \",\")\n  \
            let d = list.dedup(xs)\n  println(int.to_string(list.len(d)))\n  \
            let e0 = list.get(d, 0)\n  match e0 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let e2 = list.get(d, 2)\n  match e2 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let e3 = list.get(d, 3)\n  match e3 { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.dedup_str"));
        if let Some(out) = build_and_run("self_hosted_list_string_dedup", &render_wasm_program(&prog)) {
            assert_eq!(out, "4\na\na\nc");
        }
    }

    #[test]
    fn scalar_tuple_var_field_construct() {
        // TUPLE machinery: a `(a, b)` of NON-LITERAL scalar fields (vars / computed exprs)
        // constructs dynamically (alloc + Prim::Store each field), then destructures precisely.
        // a=3,b=7: (a,b)→(3,7); (a+1, b*2)→(4,14). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a = 3\n  let b = 7\n  \
            let t = (a, b)\n  let (x, y) = t\n  \
            println(int.to_string(x))\n  println(int.to_string(y))\n  \
            let u = (a + 1, b * 2)\n  let (p, q) = u\n  \
            println(int.to_string(p))\n  println(int.to_string(q)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("scalar_tuple_var_field_construct", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n7\n4\n14");
        }
    }

    #[test]
    fn scalar_tuple_construct_and_destructure() {
        // TUPLE machinery (scalar-field slice): a `(3, 7)` literal materializes a 2-slot block
        // (Init::IntList, like a List[Int] literal); a `let (a, b) = t` destructure LOADS each
        // field at its slot (precise extraction, not the container-grain alias). Computes a=3, b=7,
        // a+b via the bound scalars. Byte-matches v0. The foundation for list.zip/enumerate/etc.
        let src = "fn main() -> Unit = {\n  \
            let t = (3, 7)\n  let (a, b) = t\n  \
            println(int.to_string(a))\n  println(int.to_string(b))\n  \
            let s = a + b\n  println(int.to_string(s)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("scalar_tuple_construct_and_destructure", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n7\n10");
        }
    }

    #[test]
    fn self_hosted_list_string_intersperse() {
        // SELF-HOSTED list.intersperse over a List[String] — insert sep between elements (deep
        // copies). xs=[a,b,c] sep="-" → [a,-,b,-,c] len 5, idx0 "a", idx1 "-", idx4 "c". v0-match.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"a,b,c\", \",\")\n  \
            let r = list.intersperse(xs, \"-\")\n  println(int.to_string(list.len(r)))\n  \
            let e0 = list.get(r, 0)\n  match e0 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let e1 = list.get(r, 1)\n  match e1 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let e4 = list.get(r, 4)\n  match e4 { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.intersperse_str"));
        if let Some(out) = build_and_run("self_hosted_list_string_intersperse", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\na\n-\nc");
        }
    }

    #[test]
    fn self_hosted_list_string_find() {
        // SELF-HOSTED list.find over a List[String] → Option[String] (predicate higher-order; the
        // matched element returned as Some(deep copy), else None; materialized Option, match-exec).
        // xs=[apple,banana,kiwi,fig]: find(len==6)=Some("banana"); find(len==99)=None. v0-match.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"apple,banana,kiwi,fig\", \",\")\n  \
            let f1 = list.find(xs, (x) => { let l = string.len(x)\n l == 6 })\n  \
            match f1 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let f2 = list.find(xs, (x) => { let l = string.len(x)\n l == 99 })\n  \
            match f2 { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        // C1: the inline closure is defunctionalized — `list.find` is inlined as an
        // early-exit loop (try_lower_defunc_find), NOT auto-linked as a combinator.
        assert!(
            !prog.functions.iter().any(|f| f.name == "list.find_str"),
            "list.find is inlined, NOT auto-linked as a combinator"
        );
        if let Some(out) = build_and_run("self_hosted_list_string_find", &render_wasm_program(&prog)) {
            assert_eq!(out, "banana\nnone");
        }
    }

    #[test]
    fn self_hosted_map_string_string() {
        // SELF-HOSTED Map[String,String] (uniform-heap DynListStr, len=2*entries, DropListStr frees
        // all keys+values). Keys built via string.split (List[String] literal gap). new→set("a","x")
        // →set("b","y")→set("a","z")[update in place]={a:z, b:y} len 2; get("a")="z", get("b")="y",
        // get("zz")=None; contains("b")=true. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m0 = map.new()\n  \
            let m1 = map.set(m0, \"a\", \"x\")\n  let m2 = map.set(m1, \"b\", \"y\")\n  \
            let m = map.set(m2, \"a\", \"z\")\n  \
            println(int.to_string(map.len(m)))\n  \
            let ga = map.get(m, \"a\")\n  match ga { Some(v) => println(v), None => println(\"none\"), }\n  \
            let gb = map.get(m, \"b\")\n  match gb { Some(v) => println(v), None => println(\"none\"), }\n  \
            let gz = map.get(m, \"zz\")\n  match gz { Some(v) => println(v), None => println(\"none\"), }\n  \
            let c = map.contains(m, \"b\")\n  let vc = if c then 1 else 0\n  println(int.to_string(vc)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.set_str"));
        assert!(prog.functions.iter().any(|f| f.name == "map.get_str"));
        if let Some(out) = build_and_run("self_hosted_map_string_string", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\nz\ny\nnone\n1");
        }
    }

