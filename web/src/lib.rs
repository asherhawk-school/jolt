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

/// Where a program's System.in comes from.
pub enum Input<'a> {
    /// All of the input, up front (like piping a file in).
    Given(&'a str),
    /// Read from the host the first time the program asks (`Host::read_stdin`).
    FromHost,
    /// Typed while the program runs: the program pauses when it needs more, and
    /// `Session::resume` continues it (like typing into a terminal).
    Interactive,
}

/// A program that has paused, waiting for input.
pub struct Session {
    pub vm: vm::Vm,
    run_ms: f64,
}

pub enum Started {
    Done(i32),
    Waiting(Session),
}

impl Session {
    /// Continue with more input, or with `None` to signal end of input (Ctrl+D).
    pub fn resume(mut self, input: Option<&str>) -> Started {
        match input {
            Some(text) => self.vm.feed_input(text),
            None => self.vm.end_input(),
        }
        let t = self.vm.host.now_ms();
        let outcome = self.vm.resume();
        self.run_ms += self.vm.host.now_ms() - t;
        self.settle(outcome)
    }

    fn settle(mut self, outcome: vm::RunOutcome) -> Started {
        match outcome {
            vm::RunOutcome::NeedInput => {
                self.vm.host.write(fd::STATUS, "{\"event\":\"waiting\"}\n");
                Started::Waiting(self)
            }
            vm::RunOutcome::Exited(code) => {
                let steps = self.vm.steps;
                self.vm.host.write(
                    fd::STATUS,
                    &format!(
                        "{{\"event\":\"exited\",\"code\":{},\"ms\":{:.3},\"steps\":{}}}\n",
                        code, self.run_ms, steps
                    ),
                );
                Started::Done(code)
            }
        }
    }
}

/// Compile one Java source file and optionally run it on jolt-vm.
/// Everything is reported through `host`.
pub fn start(
    mut host: Box<dyn Host>,
    source: &str,
    input: Input,
    args: &[String],
    run: bool,
    disassemble: bool,
) -> Started {
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
            return Started::Done(1);
        }
    };
    let classes = match program.generate_all() {
        Ok(c) => c,
        Err(mut diags) => {
            host.write(fd::DIAG, &jolt::render_diags(&mut diags, &sources));
            host.write(fd::STATUS, "{\"event\":\"compile-error\",\"ms\":0}\n");
            return Started::Done(1);
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
        return Started::Done(0);
    }
    let Some(main) = program.main_class.clone() else {
        host.write(
            fd::DIAG,
            "error: no class has a `public static void main(String[] args)` method\n",
        );
        host.write(fd::STATUS, "{\"event\":\"exited\",\"code\":1,\"ms\":0,\"steps\":0}\n");
        return Started::Done(1);
    };
    let mut vm = match input {
        Input::Given(text) => vm::Vm::new(host, Some(text)),
        Input::FromHost => vm::Vm::new(host, None),
        Input::Interactive => {
            let mut vm = vm::Vm::new(host, Some(""));
            vm.set_interactive();
            vm
        }
    };
    for (_, b) in &classes {
        if let Err(e) = vm.load(b) {
            vm.host.write(fd::DIAG, &format!("jolt-vm: cannot load class: {}\n", e));
            return Started::Done(1);
        }
    }
    let t1 = vm.host.now_ms();
    let outcome = vm.start_main(&main, args);
    let run_ms = vm.host.now_ms() - t1;
    Session { vm, run_ms }.settle(outcome)
}

/// Compile and run to completion with non-interactive input; returns the exit code.
pub fn compile_and_run(
    host: Box<dyn Host>,
    source: &str,
    stdin: Option<&str>,
    args: &[String],
    run: bool,
    disassemble: bool,
) -> i32 {
    let input = match stdin {
        Some(text) => Input::Given(text),
        None => Input::FromHost,
    };
    match start(host, source, input, args, run, disassemble) {
        Started::Done(code) => code,
        Started::Waiting(_) => unreachable!("non-interactive programs never pause"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct TestHost(Rc<RefCell<String>>);

    impl Host for TestHost {
        fn write(&mut self, fd: u32, s: &str) {
            if fd == fd::STDOUT || fd == fd::STDERR || fd == fd::DIAG {
                self.0.borrow_mut().push_str(s);
            }
        }
        fn now_ms(&mut self) -> f64 {
            0.0
        }
        fn epoch_ms(&mut self) -> f64 {
            0.0
        }
        fn random(&mut self) -> f64 {
            0.5
        }
        fn sleep_ms(&mut self, _: f64) {}
    }

    const ASK: &str = r#"
import java.util.Scanner;
public class Ask {
    static Scanner in = new Scanner(System.in);
    public static void main(String[] args) {
        System.out.print("Name? ");
        String name = in.nextLine();
        System.out.print("Age? ");
        int age = in.nextInt();
        System.out.println(name + " is " + age);
        while (in.hasNext()) System.out.println("extra " + in.next());
        System.out.println("done");
    }
}
"#;

    fn waiting(s: Started) -> Session {
        match s {
            Started::Waiting(s) => s,
            Started::Done(c) => panic!("expected the program to wait for input, but it exited with {}", c),
        }
    }

    #[test]
    fn interactive_input_pauses_and_resumes() {
        let out = Rc::new(RefCell::new(String::new()));
        let host = Box::new(TestHost(out.clone()));
        let s = waiting(start(host, ASK, Input::Interactive, &[], true, false));
        assert_eq!(*out.borrow(), "Name? ");
        let s = waiting(s.resume(Some("Ada Lovelace\n")));
        assert_eq!(*out.borrow(), "Name? Age? ");
        // A number typed in two pieces must not be read early as "3".
        let s = waiting(s.resume(Some("3")));
        let s = waiting(s.resume(Some("6\n")));
        assert_eq!(*out.borrow(), "Name? Age? Ada Lovelace is 36\n");
        let s = waiting(s.resume(Some("x y\n")));
        assert!(out.borrow().ends_with("extra x\nextra y\n"));
        match s.resume(None) {
            Started::Done(code) => assert_eq!(code, 0),
            Started::Waiting(_) => panic!("should finish at end of input"),
        }
        assert!(out.borrow().ends_with("done\n"));
    }

    #[test]
    fn given_input_runs_to_completion() {
        let out = Rc::new(RefCell::new(String::new()));
        let host = Box::new(TestHost(out.clone()));
        let code = compile_and_run(host, ASK, Some("Grace\n85 a\n"), &[], true, false);
        assert_eq!(code, 0);
        assert_eq!(*out.borrow(), "Name? Age? Grace is 85\nextra a\ndone\n");
    }

    #[test]
    fn empty_given_input_behaves_like_java() {
        let out = Rc::new(RefCell::new(String::new()));
        let host = Box::new(TestHost(out.clone()));
        let code = compile_and_run(host, ASK, Some(""), &[], true, false);
        assert_eq!(code, 1);
        assert!(out.borrow().contains("java.util.NoSuchElementException: No line found"));
    }
}
