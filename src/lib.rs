//! jolt as a library: compile Java source text to JVM class files in memory.
//!
//! The command-line tool (`src/main.rs`) and the in-browser playground (`web/`)
//! both drive the compiler through this API.

pub mod alloc;
pub mod ast;
pub mod check;
pub mod classes;
pub mod classfile;
pub mod codegen;
pub mod declare;
pub mod diag;
pub mod fxhash;
pub mod lexer;
pub mod parser;
pub mod tir;
pub mod types;

pub use diag::Diag;

/// Inputs bigger than this are processed on all cores (when threads exist).
pub const PARALLEL_THRESHOLD_BYTES: usize = 256 * 1024;

pub struct Source {
    pub path: String,
    pub text: String,
}

/// A type-checked program, ready for code generation.
pub struct Program {
    pub classes: Vec<Checked>,
    /// Dotted name of the first class with `public static void main(String[])`.
    pub main_class: Option<String>,
}

pub struct Checked {
    pub class: tir::TClass,
    file: usize,
    pos: diag::Pos,
}

/// Map `f` over `items`, on all cores when `parallel` is set.
pub fn par_map<T: Sync, R: Send>(items: &[T], parallel: bool, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
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
pub fn threads() -> usize {
    std::env::var("JOLT_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// Lex, parse, declare and type-check `sources`. `phase` is called after each
/// phase with its name (handy for timing). With `leak`, the syntax trees and
/// class table are intentionally not freed, which saves time in a short-lived process.
pub fn check(
    sources: &[Source],
    parallel: bool,
    leak: bool,
    phase: &mut dyn FnMut(&'static str),
) -> Result<Program, Vec<Diag>> {
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
        return Err(diags);
    }
    phase("lex + parse");

    let mut table = classes::ClassTable::with_jdk();
    let diags = declare::declare(&units, &mut table);
    if !diags.is_empty() {
        return Err(diags);
    }
    check::compute_constants(&units, &mut table);
    phase("declare");

    let decls = declare::walk(&units);
    let checked = par_map(&decls, parallel, |(d, name, file)| {
        let sf = file_name(&sources[*file].path);
        check::check_class(&table, d, name, *file, &sf).map(|class| Checked {
            class,
            file: *file,
            pos: d.pos,
        })
    });
    let mut classes = Vec::with_capacity(checked.len());
    let mut diags = Vec::new();
    for r in checked {
        match r {
            Ok(c) => classes.push(c),
            Err(mut ds) => diags.append(&mut ds),
        }
    }
    if !diags.is_empty() {
        return Err(diags);
    }

    let main_ty = types::Type::array_of(types::Type::string());
    let main_class = decls.iter().find_map(|(_, name, _)| {
        let ci = table.get(name)?;
        ci.methods
            .iter()
            .any(|m| m.name == "main" && m.is_static && m.params == [main_ty.clone()] && m.ret == types::Type::Void)
            .then(|| name.replace('/', "."))
    });
    phase("check");

    if leak {
        std::mem::forget(decls);
        std::mem::forget(units);
        std::mem::forget(table);
    }
    Ok(Program { classes, main_class })
}

impl Program {
    /// Generate every class file and hand it to `sink(internal_name, bytes)`.
    pub fn generate(
        &self,
        parallel: bool,
        sink: &(dyn Fn(&str, Vec<u8>) -> Result<(), String> + Sync),
    ) -> Result<(), Vec<Diag>> {
        let results = par_map(&self.classes, parallel, |c| {
            let err = |msg| Diag {
                file: c.file,
                pos: c.pos,
                msg,
            };
            let bytes = codegen::gen_class(&c.class).map_err(err)?;
            sink(&c.class.name, bytes).map_err(err)
        });
        let diags: Vec<Diag> = results.into_iter().filter_map(|r| r.err()).collect();
        if diags.is_empty() {
            Ok(())
        } else {
            Err(diags)
        }
    }

    /// Generate all class files into memory: (internal name, bytes).
    pub fn generate_all(&self) -> Result<Vec<(String, Vec<u8>)>, Vec<Diag>> {
        let out = std::sync::Mutex::new(Vec::new());
        self.generate(false, &|name, bytes| {
            out.lock().unwrap().push((name.to_string(), bytes));
            Ok(())
        })?;
        Ok(out.into_inner().unwrap())
    }
}

/// Render diagnostics javac-style, sorted by position, followed by a count.
pub fn render_diags(diags: &mut [Diag], sources: &[Source]) -> String {
    diags.sort_by_key(|d| (d.file, d.pos.line, d.pos.col));
    let mut out = String::new();
    for d in diags.iter().take(100) {
        let s = &sources[d.file];
        out.push_str(&diag::render(d, &s.path, &s.text));
    }
    let n = diags.len();
    out.push_str(&format!("{} error{}\n", n, if n == 1 { "" } else { "s" }));
    out
}
