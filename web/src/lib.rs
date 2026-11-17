//! jolt in the browser: the jolt compiler plus a tiny JVM ("jolt-vm") that
//! runs the class files it produces. Compiled to WebAssembly for the playground,
//! and to a native binary (`jolt-vm`) for testing against the real JVM.

pub mod disasm;
pub mod jfmt;
pub mod natives;
pub mod regex;
pub mod vm;

#[cfg(target_arch = "wasm32")]
mod wasm;

pub use vm::Host;

/// Output channels used with `Host::write`.
pub mod fd {
    pub const STDOUT: u32 = 1;
    pub const STDERR: u32 = 2;
    /// Compiler diagnostics (javac-style text).
    pub const DIAG: u32 = 3;
    /// One JSON object per line describing progress (compiled, exited, ...).
    pub const STATUS: u32 = 4;
    /// javap-style disassembly of the generated classes.
    pub const DISASM: u32 = 5;
}

/// Guess the file name javac would require: `public class X` lives in X.java.
pub fn file_name_for(source: &str) -> String {
    let mut words = source
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|w| !w.is_empty());
    while let Some(w) = words.next() {
        if w == "public" {
            let mut next = words.next();
            while matches!(next, Some("final") | Some("abstract") | Some("static")) {
                next = words.next();
            }
            if matches!(next, Some("class") | Some("interface")) {
                if let Some(name) = words.next() {
                    return format!("{}.java", name);
                }
            }
        }
    }
    "Main.java".into()
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Compile one Java source file and optionally run it on jolt-vm.
/// Everything is reported through `host`; returns the exit code.
/// `stdin` is the program's input, or `None` to read it from the host on demand.
pub fn compile_and_run(
    mut host: Box<dyn Host>,
    source: &str,
    stdin: Option<&str>,
    args: &[String],
    run: bool,
    disassemble: bool,
) -> i32 {
    let file = file_name_for(source);
    let t0 = host.now_ms();
    let sources = [jolt::Source {
        path: file.clone(),
        text: source.to_string(),
    }];
    let program = match jolt::check(&sources, false, false, &mut |_| {}) {
        Ok(p) => p,
        Err(mut diags) => {
            let ms = host.now_ms() - t0;
            host.write(fd::DIAG, &jolt::render_diags(&mut diags, &sources));
            host.write(
                fd::STATUS,
                &format!("{{\"event\":\"compile-error\",\"ms\":{:.3}}}\n", ms),
            );
            return 1;
        }
    };
    let classes = match program.generate_all() {
        Ok(c) => c,
        Err(mut diags) => {
            host.write(fd::DIAG, &jolt::render_diags(&mut diags, &sources));
            host.write(fd::STATUS, "{\"event\":\"compile-error\",\"ms\":0}\n");
            return 1;
        }
    };
    let ms = host.now_ms() - t0;
    let names: Vec<String> = classes.iter().map(|(n, _)| json_str(&n.replace('/', "."))).collect();
    let bytes: usize = classes.iter().map(|(_, b)| b.len()).sum();
    host.write(
        fd::STATUS,
        &format!(
            "{{\"event\":\"compiled\",\"ms\":{:.3},\"file\":{},\"classes\":[{}],\"bytes\":{},\"main\":{}}}\n",
            ms,
            json_str(&file),
            names.join(","),
            bytes,
            program
                .main_class
                .as_deref()
                .map(json_str)
                .unwrap_or_else(|| "null".into())
        ),
    );
    if disassemble {
        for (_, b) in &classes {
            host.write(fd::DISASM, &disasm::disassemble(b));
        }
    }
    if !run {
        return 0;
    }
    let Some(main) = program.main_class.clone() else {
        host.write(
            fd::DIAG,
            "error: no class has a `public static void main(String[] args)` method\n",
        );
        host.write(fd::STATUS, "{\"event\":\"exited\",\"code\":1,\"ms\":0,\"steps\":0}\n");
        return 1;
    };
    let mut vm = vm::Vm::new(host, stdin);
    for (_, b) in &classes {
        if let Err(e) = vm.load(b) {
            vm.host.write(fd::DIAG, &format!("jolt-vm: cannot load class: {}\n", e));
            return 1;
        }
    }
    let t1 = vm.host.now_ms();
    let code = vm.run_main(&main, args);
    let ms = vm.host.now_ms() - t1;
    let steps = vm.steps;
    vm.host.write(
        fd::STATUS,
        &format!(
            "{{\"event\":\"exited\",\"code\":{},\"ms\":{:.3},\"steps\":{}}}\n",
            code, ms, steps
        ),
    );
    code
}
