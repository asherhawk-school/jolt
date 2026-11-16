//! jolt: a tiny, very fast compiler for a subset of Java.

mod alloc;
mod ast;
mod check;
mod classes;
mod classfile;
mod codegen;
mod declare;
mod diag;
mod fxhash;
mod lexer;
mod parser;
mod tir;
mod types;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use diag::Diag;

#[global_allocator]
static GLOBAL: alloc::JoltAlloc = alloc::JoltAlloc;

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

/// Inputs bigger than this are processed on all cores.
const PARALLEL_THRESHOLD_BYTES: usize = 256 * 1024;

fn par_map<T: Sync, R: Send>(items: &[T], parallel: bool, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if !parallel || items.len() < 2 {
        return items.iter().map(f).collect();
    }
    let n = threads().min(items.len());
    let chunk = items.len().div_ceil(n);
    let f = &f;
    std::thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|c| s.spawn(move || c.iter().map(f).collect::<Vec<R>>()))
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    })
}

/// Worker threads: all cores by default, or `JOLT_THREADS=n`.
fn threads() -> usize {
    std::env::var("JOLT_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4))
}

struct Source {
    path: String,
    text: String,
}

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
    diags.sort_by_key(|d| (d.file, d.pos.line, d.pos.col));
    let mut out = String::new();
    for d in diags.iter().take(100) {
        let s = &sources[d.file];
        out.push_str(&diag::render(d, &s.path, &s.text));
    }
    let n = diags.len();
    out.push_str(&format!("{} error{}\n", n, if n == 1 { "" } else { "s" }));
    eprint!("{}", out);
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
    let parallel = total_bytes >= PARALLEL_THRESHOLD_BYTES && threads() > 1;
    timer.lap("read");

    let indexed: Vec<(usize, &Source)> = sources.iter().enumerate().collect();
    let parsed = par_map(&indexed, parallel, |(i, s)| {
        lexer::lex(&s.text, *i).and_then(|toks| parser::parse(&toks, *i))
    });
    let mut diags = Vec::new();
    let mut units = Vec::new();
    for r in parsed {
        match r {
            Ok(u) => units.push(u),
            Err(d) => diags.push(d),
        }
    }
    if !diags.is_empty() {
        report(&mut diags, &sources);
        return Err(());
    }
    timer.lap("lex + parse");

    let mut table = classes::ClassTable::with_jdk();
    let mut diags = declare::declare(&units, &mut table);
    if !diags.is_empty() {
        report(&mut diags, &sources);
        return Err(());
    }
    check::compute_constants(&units, &mut table);
    timer.lap("declare");

    let decls = declare::walk(&units);
    // Type-check everything first so that nothing is written if there are errors.
    let checked = par_map(&decls, parallel, |(d, name, file)| {
        let sf = Path::new(&sources[*file].path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        check::check_class(&table, d, name, *file, &sf).map(|tc| (tc, *file, d.pos))
    });
    let mut classes = Vec::with_capacity(checked.len());
    for r in checked {
        match r {
            Ok(c) => classes.push(c),
            Err(mut ds) => diags.append(&mut ds),
        }
    }
    if !diags.is_empty() {
        report(&mut diags, &sources);
        return Err(());
    }
    timer.lap("check");

    // Create output directories once, then generate + write class files in parallel.
    let mut dirs: Vec<PathBuf> = Vec::new();
    for (tc, _, _) in &classes {
        let dir = match tc.name.rsplit_once('/') {
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
    let written = par_map(&classes, parallel, |(tc, file, pos)| {
        let bytes = codegen::gen_class(tc).map_err(|msg| Diag {
            file: *file,
            pos: *pos,
            msg,
        })?;
        let path = out_dir.join(format!("{}.class", tc.name));
        std::fs::write(&path, &bytes).map_err(|e| Diag {
            file: *file,
            pos: *pos,
            msg: format!("cannot write {}: {}", path.display(), e),
        })
    });
    let mut diags: Vec<Diag> = written.into_iter().filter_map(|r| r.err()).collect();
    if !diags.is_empty() {
        report(&mut diags, &sources);
        return Err(());
    }
    timer.lap("codegen + write");
    timer.total();

    // Find a main class: the first class with `static void main(String[])`.
    let main_ty = types::Type::array_of(types::Type::string());
    let main = decls.iter().find_map(|(_, name, _)| {
        let ci = table.get(name)?;
        ci.methods
            .iter()
            .any(|m| m.name == "main" && m.is_static && m.params == [main_ty.clone()] && m.ret == types::Type::Void)
            .then(|| name.replace('/', "."))
    });

    // The process is about to exit: skip running destructors for the big trees.
    std::mem::forget(decls);
    std::mem::forget(classes);
    std::mem::forget(units);
    std::mem::forget(table);
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
