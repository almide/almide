// Parallel runtime dependencies are closed under the actual fan header.
// Keep this separate from the ordinary floor: a renderer taking these items
// must also emit fan_glue and its trap/output routing before every item.
// This does not select a parallel source call or admit a source ABI.
pub(crate) fn parallel_runtime_body(symbol: &str) -> Result<(Vec<String>, String, Vec<&'static str>), String> {
    if !almide_codegen::generated::runtime_fn_modes::crosses_threads(symbol) {
        return Err(format!("`{symbol}` is not a thread-crossing runtime call"));
    }
    runtime_body_with_fan(symbol)
}

/// The generated Send-free twin preserves the declared host operation.
pub(crate) fn send_free_runtime_body(symbol: &str) -> Option<(Vec<String>, String, Vec<&'static str>)> {
    let twin = almide_codegen::generated::runtime_fn_modes::send_free_twin(symbol)?;
    runtime_body_with_fan(twin).ok()
}

fn runtime_body_with_fan(symbol: &str) -> Result<(Vec<String>, String, Vec<&'static str>), String> {
    let rt = runtime_index();
    let text = signature_text(rt, symbol).ok_or("no runtime item")?;
    let (params, ret) = rust_signature(text).ok_or("no runtime signature")?;
    let glue = crate::render_native::fan_glue();
    let mut supplied = defined_names(&glue);
    supplied.extend(thread_local_names(&glue));
    let items = closure_using(symbol, rt, |text| references_with_supplied(rt, text, &supplied))?;
    Ok((params.iter().map(|p| squash(p)).collect(), squash(&ret), items))
}

#[cfg(test)]
mod parallel_runtime_tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn parallel_mapper_dependencies_compile_and_run_both_schedules() {
        assert!(parallel_runtime_body("almide_rt_string_len").is_err());
        // The normal floor must not assume output/thread hooks are ambient.
        assert!(runtime_body("almide_rt_fan_map_par").is_none());
        let (params, ret, items) = parallel_runtime_body("almide_rt_fan_map_par").expect("the runtime source defines almide_rt_fan_map_par");
        assert_eq!(params, ["Vec<A>", "F"]);
        assert_eq!(ret, "Result<Vec<B>,AlmideString>");
        let mut src = String::from("#![allow(dead_code, unused_imports, unused_mut, unused_variables)]\n");
        src.push_str(&crate::render_native::fan_glue());
        src.push_str(&crate::render_native_str::almide_string_alias());
        src.push_str(floor_header().strip_suffix(FLOOR_IO_HEADER).expect("the floor header ends with its IO header"));
        src.push_str(almide_base::native_stack_guard::NATIVE_STACK_GUARD);
        src.push_str("#[cfg(any(target_os = \"linux\", target_os = \"macos\"))] fn almide_stack_guard_stdout() {}\n");
        for item in items { src.push_str(item); src.push('\n'); }
        src.push_str(r#"
fn main() {
    if std::env::args().nth(1).as_deref() == Some("trap") {
        let _: Result<Vec<i64>, AlmideString> = almide_rt_fan_map_par((0..64).collect(), |i| {
            println!("element {i}");
            if i == 3 || i == 49 { almide_abort(format!("trap {i}")); }
            Ok(i)
        });
        panic!("the trap did not run");
    }
    let caller = std::thread::current().id();
    let count = std::sync::atomic::AtomicUsize::new(0); // text of a test program rustc builds, never on the playground compile path
    let worker = std::sync::atomic::AtomicBool::new(false);
    let parallel = !almide_rt_list_par_sequential() && almide_rt_list_par_workers(64) > 1;
    let first_pair = std::sync::Barrier::new(2);
    let result: Result<Vec<i64>, AlmideString> = almide_rt_fan_map_par((0..64).collect(), |i| {
        // Otherwise the caller can drain this tiny queue before a worker
        // is scheduled, making the thread-observation assertion flaky.
        let ordinal = count.fetch_add(1, std::sync::atomic::Ordering::SeqCst); // the same test program text, off the playground compile path
        if parallel && ordinal < 2 { first_pair.wait(); }
        if std::thread::current().id() != caller { worker.store(true, std::sync::atomic::Ordering::SeqCst); }
        println!("element {i}");
        if i == 3 || i == 49 { Err(AlmideString::from(format!("error {i}"))) } else { Ok(i * 2) }
    });
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 64);
    assert_eq!(result.unwrap_err().as_str(), "error 3");
    assert_eq!(worker.load(std::sync::atomic::Ordering::SeqCst), parallel);
    let values = almide_rt_fan_map_par(vec![2i64, 1, 3], |i| Ok(i * 7)).unwrap();
    assert_eq!(values, vec![14, 7, 21]);
    assert_eq!(almide_rt_fan_map_par(Vec::<i64>::new(), |i| Ok(i)).unwrap(), Vec::<i64>::new());
    let words: Vec<AlmideString> = (0..64).map(|i| format!("word {i}").into()).collect();
    let saved = words.clone();
    let prefix = AlmideString::from("shared capture:");
    prefix.almide_share();
    let mapped = almide_rt_fan_map_par(words, |s| Ok(AlmideString::from(format!("{prefix}{s}")))).unwrap();
    for i in 0..64 {
        assert_eq!(saved[i].as_str(), format!("word {i}"));
        assert_eq!(mapped[i].as_str(), format!("shared capture:word {i}"));
    }
    println!("all elements settled; lowest error; values in input order");
}
"#);
        let src = crate::render_native::route_exits(&src);
        let dir = std::env::temp_dir().join(format!("almide-native-parallel-runtime-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the scratch dir");
        let file = dir.join("runtime.rs");
        let bin = dir.join("runtime");
        std::fs::write(&file, src).expect("write the runtime source");
        let build = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
            .args(["--edition=2021", "-Awarnings"]).arg(&file).arg("-o").arg(&bin).output().expect("spawn rustc");
        assert!(build.status.success(), "{}\nsource: {}", String::from_utf8_lossy(&build.stderr), file.display());
        let mut expected = (0..64).map(|i| format!("element {i}\n")).collect::<String>();
        expected.push_str("all elements settled; lowest error; values in input order\n");
        for sequential in ["0", "1"] {
            let run = Command::new(&bin).env("ALMIDE_FAN_SEQUENTIAL", sequential)
                .env("ALMIDE_FAN_COST_OFF", "1").env("ALMIDE_FAN_THREADS", "4").output().expect("run the built runtime");
            assert!(run.status.success(), "schedule {sequential}: {}", String::from_utf8_lossy(&run.stderr));
            assert_eq!(run.stdout, expected.as_bytes(), "schedule {sequential}");
            assert!(run.stderr.is_empty());
            let trap = Command::new(&bin).arg("trap").env("ALMIDE_FAN_SEQUENTIAL", sequential)
                .env("ALMIDE_FAN_COST_OFF", "1").env("ALMIDE_FAN_THREADS", "4").output().expect("run the built runtime's trap case");
            assert_eq!(trap.status.code(), Some(1));
            assert_eq!(trap.stdout, b"element 0\nelement 1\nelement 2\nelement 3\n");
            assert_eq!(trap.stderr, b"Error: trap 3\n");
        }
        std::fs::remove_dir_all(dir).expect("remove the scratch dir");
    }
}
