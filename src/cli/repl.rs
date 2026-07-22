use std::io::{self, Write, BufRead};
use std::path::PathBuf;
use crate::{out, out_no_nl, err};

pub fn run_repl() {
    out(&format!("Almide REPL v{} — type expressions to evaluate, :q to quit",
             env!("CARGO_PKG_VERSION")));
    out("");

    let mut session = Session::new();
    let stdin = io::stdin();

    loop {
        out_no_nl(&format!(">>> "));
        io::stdout().flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let input = line.trim();
        if input.is_empty() { continue; }

        if input.starts_with(':') {
            match input {
                ":q" | ":quit" => break,
                ":h" | ":help" => print_help(),
                ":history" => session.print_history(),
                ":clear" => session.clear(),
                _ => out(&format!("Unknown command: {}. Type :h for help.", input)),
            }
            continue;
        }

        session.eval(input);
    }
}

enum Kind { TopLevel, Body, Expr }

fn classify(input: &str) -> Kind {
    let t = input.trim_start();
    if t.starts_with("fn ") || t.starts_with("effect fn ")
        || t.starts_with("type ") || t.starts_with("mod type ")
        || t.starts_with("import ")
    {
        return Kind::TopLevel;
    }
    if t.starts_with("let ") || t.starts_with("var ") || t.starts_with("for ") {
        return Kind::Body;
    }
    // Assignment: `ident = expr` (not `==` or `=>`)
    let bytes = t.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.') {
        i += 1;
    }
    if i > 0 {
        while i < bytes.len() && bytes[i] == b' ' { i += 1; }
        if i < bytes.len() && bytes[i] == b'='
            && bytes.get(i + 1) != Some(&b'=')
            && bytes.get(i + 1) != Some(&b'>')
        {
            return Kind::Body;
        }
    }
    Kind::Expr
}

struct Session {
    top: Vec<String>,
    body: Vec<String>,
    history: Vec<String>,
    repl_dir: PathBuf,
    build_dir: PathBuf,
}

impl Session {
    fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let repl_dir = PathBuf::from(&home).join(".almide/repl");
        let build_dir = repl_dir.join("build");
        std::fs::create_dir_all(&build_dir).ok();
        Self { top: vec![], body: vec![], history: vec![], repl_dir, build_dir }
    }

    fn eval(&mut self, input: &str) {
        self.history.push(input.to_string());
        match classify(input) {
            Kind::TopLevel => self.eval_top(input),
            Kind::Body => self.eval_body(input),
            Kind::Expr => self.eval_expr(input),
        }
    }

    fn eval_top(&mut self, input: &str) {
        let mut new_top = self.top.clone();
        new_top.push(input.to_string());
        let source = build_program(&new_top, &self.body, None);
        if self.compile_quiet(&source).is_ok() {
            self.top.push(input.to_string());
        }
    }

    fn eval_body(&mut self, input: &str) {
        let mut new_body = self.body.clone();
        new_body.push(input.to_string());
        let source = build_program(&self.top, &new_body, None);
        if self.compile_quiet(&source).is_ok() {
            self.body.push(input.to_string());
        }
    }

    fn eval_expr(&mut self, input: &str) {
        let source = build_program(&self.top, &self.body, Some(input));
        match self.compile_and_run(&source) {
            Ok(result) => {
                let result = result.trim();
                if !result.is_empty() {
                    out(&format!("{}", result));
                }
            }
            Err(_) => {} // errors already printed by compiler / cargo
        }
    }

    fn compile(&self, source: &str) -> Result<String, String> {
        let path = self.source_path();
        std::fs::write(&path, source).map_err(|e| e.to_string())?;
        crate::try_compile(path.to_str().unwrap(), false)
    }

    fn compile_quiet(&self, source: &str) -> Result<String, String> {
        crate::SUPPRESS_WARNINGS.store(true, std::sync::atomic::Ordering::Relaxed);
        let result = self.compile(source);
        crate::SUPPRESS_WARNINGS.store(false, std::sync::atomic::Ordering::Relaxed);
        result
    }

    fn compile_and_run(&self, source: &str) -> Result<String, String> {
        let rust_code = self.compile_quiet(source)?;
        // Use Debug format for the REPL print so List, records etc. work
        let rust_code = rust_code.replace(
            r#"format!("{}\n", __r)"#,
            r#"format!("{:?}\n", __r)"#,
        );
        let bin = super::cargo_build_generated(&rust_code, &self.build_dir, false)?;
        let output = std::process::Command::new(&bin)
            .output()
            .map_err(|e| format!("execution failed: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                err(&format!("{}", stderr.trim()));
            }
            return Err("runtime error".into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn source_path(&self) -> PathBuf {
        self.repl_dir.join("repl.almd")
    }

    fn print_history(&self) {
        for (i, h) in self.history.iter().enumerate() {
            out(&format!("{:>3}  {}", i + 1, h));
        }
    }

    fn clear(&mut self) {
        self.top.clear();
        self.body.clear();
        out(&format!("Session cleared."));
    }
}

fn build_program(top: &[String], body: &[String], expr: Option<&str>) -> String {
    let mut s = String::new();
    s.push_str("import io\n");
    for decl in top {
        s.push_str(decl);
        s.push('\n');
    }
    s.push_str("\neffect fn main() -> Unit = {\n");
    for line in body {
        s.push_str("  ");
        s.push_str(line);
        s.push('\n');
    }
    if let Some(e) = expr {
        s.push_str("  let __r = ");
        s.push_str(e);
        s.push('\n');
        s.push_str("  io.print(\"${__r}\\n\")\n");
    }
    s.push_str("  io.print(\"\")\n");
    s.push_str("}\n");
    s
}

fn print_help() {
    out(&format!("Commands:"));
    out(&format!("  :q, :quit    Exit"));
    out(&format!("  :h, :help    Show this help"));
    out(&format!("  :history     Show evaluation history"));
    out(&format!("  :clear       Clear session state"));
    out("");
    out(&format!("Examples:"));
    out(&format!("  >>> 1 + 2"));
    out(&format!("  3"));
    out(&format!("  >>> let name = \"world\""));
    out(&format!("  >>> \"Hello, \" + name"));
    out(&format!("  Hello, world"));
}
