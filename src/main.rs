//! jolt: a tiny, very fast compiler for a subset of Java.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use jolt::{Diag, Source};

#[global_allocator]
static GLOBAL: jolt::alloc::JoltAlloc = jolt::alloc::JoltAlloc;

const USAGE: &str = "\
jolt - a very fast compiler for a subset of Java

USAGE:
    jolt [-d <dir>] [--time] <File.java>...      compile to .class files
    jolt run [--time] <File.java>... [args...]   compile and run with `java`

OPTIONS:
    -d <dir>     where to put .class files (default: current directory)
    --time       print how long each compiler phase took
    -h, --help   show this help
    -V, --version
";

struct Timer {
    on: bool,
    start: Instant,
    last: Instant,
}

impl Timer {
    fn lap(&mut self, what: &str) {
        if self.on {
            let now = Instant::now();
            eprintln!("  {:<18} {:>9.3} ms", what, (now - self.last).as_secs_f64() * 1000.0);
            self.last = now;
        }
    }
    fn total(&self) {
        if self.on {
            eprintln!(
                "  {:<18} {:>9.3} ms",
                "total",
                self.start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}

fn report(diags: &mut [Diag], sources: &[Source]) {
    eprint!("{}", jolt::render_diags(diags, sources));
}

/// Compile sources into `out_dir`. Returns the main class (if any) on success.
fn compile(paths: &[String], out_dir: &Path, time: bool) -> Result<Option<String>, ()> {
    let mut timer = Timer {
        on: time,
        start: Instant::now(),
        last: Instant::now(),
    };

    let mut sources = Vec::with_capacity(paths.len());
    for p in paths {
        match std::fs::read(p) {
            Ok(bytes) => {
                let text = match String::from_utf8(bytes) {
                    Ok(s) => s,
                    Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
                };
                sources.push(Source { path: p.clone(), text });
            }
            Err(e) => {
                eprintln!("error: cannot read {}: {}", p, e);
                return Err(());
            }
        }
    }
    let total_bytes: usize = sources.iter().map(|s| s.text.len()).sum();
    let parallel = total_bytes >= jolt::PARALLEL_THRESHOLD_BYTES && jolt::threads() > 1;
    timer.lap("read");

    let program = match jolt::check(&sources, parallel, true, &mut |p| timer.lap(p)) {
        Ok(p) => p,
        Err(mut diags) => {
            report(&mut diags, &sources);
            return Err(());
        }
    };

    // Create output directories once, then generate + write class files in parallel.
    let mut dirs: Vec<PathBuf> = Vec::new();
    for c in &program.classes {
        let dir = match c.class.name.rsplit_once('/') {
            Some((pkg, _)) => out_dir.join(pkg),
            None => out_dir.to_path_buf(),
        };
        if !dirs.contains(&dir) {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("error: cannot create {}: {}", dir.display(), e);
                return Err(());
            }
            dirs.push(dir);
        }
    }
    let written = program.generate(parallel, &|name, bytes| {
        let path = out_dir.join(format!("{}.class", name));
        std::fs::write(&path, bytes).map_err(|e| format!("cannot write {}: {}", path.display(), e))
    });
    if let Err(mut diags) = written {
        report(&mut diags, &sources);
        return Err(());
    }
    timer.lap("codegen + write");
    timer.total();

    let main = program.main_class.clone();
    // The process is about to exit: skip running destructors for the big trees.
    std::mem::forget(program);
    std::mem::forget(sources);
    Ok(main)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{}", USAGE);
        return if args.is_empty() {
            ExitCode::from(2)
        } else {
            ExitCode::SUCCESS
        };
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("jolt {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    if args[0] == "run" {
        let mut time = false;
        let mut files = Vec::new();
        let mut prog_args = Vec::new();
        let mut it = args[1..].iter();
        for a in it.by_ref() {
            if a == "--time" {
                time = true;
            } else if a.ends_with(".java") {
                files.push(a.clone());
            } else if a == "--" {
                break;
            } else {
                prog_args.push(a.clone());
                break;
            }
        }
        prog_args.extend(it.cloned());
        if files.is_empty() {
            eprintln!("error: no .java files given");
            return ExitCode::from(2);
        }
        let dir = std::env::temp_dir().join(format!("jolt-run-{}", std::process::id()));
        let result = compile(&files, &dir, time);
        let code = match result {
            Err(()) => ExitCode::from(1),
            Ok(None) => {
                eprintln!("error: no class with `public static void main(String[] args)` found");
                ExitCode::from(1)
            }
            Ok(Some(main)) => {
                let java = std::env::var_os("JAVA_HOME")
                    .map(|h| PathBuf::from(h).join("bin").join("java"))
                    .filter(|p| p.exists())
                    .unwrap_or_else(|| PathBuf::from("java"));
                match std::process::Command::new(java)
                    .arg("-cp")
                    .arg(&dir)
                    .arg(&main)
                    .args(&prog_args)
                    .status()
                {
                    Ok(st) => ExitCode::from(st.code().unwrap_or(1).clamp(0, 255) as u8),
                    Err(e) => {
                        eprintln!("error: could not start `java`: {}", e);
                        ExitCode::from(1)
                    }
                }
            }
        };
        let _ = std::fs::remove_dir_all(&dir);
        return code;
    }

    let mut out_dir = PathBuf::from(".");
    let mut time = false;
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-d" => {
                i += 1;
                match args.get(i) {
                    Some(d) => out_dir = PathBuf::from(d),
                    None => {
                        eprintln!("error: -d needs a directory");
                        return ExitCode::from(2);
                    }
                }
            }
            "--time" => time = true,
            a if a.starts_with('-') => {
                eprintln!("error: unknown option {}", a);
                return ExitCode::from(2);
            }
            a => files.push(a.to_string()),
        }
        i += 1;
    }
    if files.is_empty() {
        eprintln!("error: no source files");
        return ExitCode::from(2);
    }
    match compile(&files, &out_dir, time) {
        Ok(_) => ExitCode::SUCCESS,
        Err(()) => ExitCode::from(1),
    }
}
