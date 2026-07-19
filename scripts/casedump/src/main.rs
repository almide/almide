fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    if mode == "classes" {
        // char CLASS ranges from Rust itself (the native oracle): contiguous
        // codepoint ranges for is_alphabetic / is_uppercase / is_lowercase —
        // consumed by gen-case-tables.py into stdlib/string_class.almd.
        for (tag, pred) in [
            ("A", (|c: char| c.is_alphabetic()) as fn(char) -> bool),
            ("U", |c: char| c.is_uppercase()),
            ("L", |c: char| c.is_lowercase()),
            ("N", |c: char| c.is_numeric()),
        ] {
            let mut start: Option<u32> = None;
            let mut prev = 0u32;
            for cp in 0u32..0x110000 {
                let inside = char::from_u32(cp).map(pred).unwrap_or(false);
                match (inside, start) {
                    (true, None) => start = Some(cp),
                    (false, Some(s)) => {
                        println!("{} {:X} {:X}", tag, s, prev);
                        start = None;
                    }
                    _ => {}
                }
                if inside {
                    prev = cp;
                }
            }
            if let Some(s) = start {
                println!("{} {:X} {:X}", tag, s, prev);
            }
        }
        return;
    }
    if mode == "props" {
        // black-box Cased / Case_Ignorable from str::to_lowercase's Final_Sigma:
        // lower("Σ"+cp)[0] == σ  ⟺  cp is Cased (a cased char follows Σ)
        // else lower("Σ"+cp+"A")[0] == σ  ⟺  cp is Case_Ignorable (skipped, A cased)
        for cp in 0u32..0x110000 {
            if let Some(c) = char::from_u32(cp) {
                let s1: String = format!("A\u{3A3}{}", c).to_lowercase();
                let second1 = s1.chars().nth(1).unwrap();
                if second1 == '\u{3C3}' {
                    println!("C {:X}", cp); // cased (non-final sigma)
                } else {
                    let s2: String = format!("A\u{3A3}{}B", c).to_lowercase();
                    if s2.chars().nth(1).unwrap() == '\u{3C3}' {
                        println!("I {:X}", cp); // case-ignorable (skipped)
                    }
                }
            }
        }
        return;
    }
    for cp in 0u32..0x110000 {
        if let Some(c) = char::from_u32(cp) {
            let up: Vec<u32> = c.to_uppercase().map(|x| x as u32).collect();
            if up != vec![cp] {
                println!("U {:X} {}", cp, up.iter().map(|x| format!("{:X}", x)).collect::<Vec<_>>().join(" "));
            }
            let lo: Vec<u32> = c.to_lowercase().map(|x| x as u32).collect();
            if lo != vec![cp] {
                println!("L {:X} {}", cp, lo.iter().map(|x| format!("{:X}", x)).collect::<Vec<_>>().join(" "));
            }
        }
    }
}
