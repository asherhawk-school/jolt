# jolt ⚡

**A Java compiler that finishes before `javac` has even started its JVM.**

jolt is a from-scratch compiler for a practical subset of Java, written in Rust with zero
dependencies. It turns `.java` source into real JVM `.class` files that run on any
modern Java runtime, and it does so **25–100× faster than `javac`**.

### ▶ [Try it in your browser](https://asherhawk-school.github.io/jolt/)

The [playground](https://asherhawk-school.github.io/jolt/) compiles and runs Java
entirely on your device, with no server involved. The jolt compiler and **jolt-vm**, a
tiny JVM written for this project, are both compiled to WebAssembly. You get errors
as you type, live program output, a `javap`-style bytecode view, stdin for `Scanner`
programs, and shareable links.

```console
$ jolt run Hello.java
Hello, world!

$ jolt -d out --time Algorithms.java
  read                   0.064 ms
  lex + parse            0.387 ms
  declare                0.207 ms
  check                  0.296 ms
  codegen + write        0.504 ms
  total                  1.505 ms
```

## Benchmarks

Wall-clock time to compile, median of repeated runs, measured on an Apple M1 against
OpenJDK 17 (`javac -nowarn`). Every row also runs the program compiled by each
compiler and checks that the outputs are **byte-for-byte identical**.

| Input | jolt | javac | Speed-up |
|---|---:|---:|---:|
| Hello world | 3.9 ms | 370 ms | **95×** |
| Algorithms (sorting, DP, N-queens, …) | 6.3 ms | 529 ms | **83×** |
| Strings (StringBuilder, printf, …) | 5.2 ms | 494 ms | **96×** |
| All test programs at once | 11.1 ms | 726 ms | **65×** |
| Generated project, 6k lines | 17.5 ms | 1,154 ms | **66×** |
| Generated project, 118k lines | 86 ms | 4,422 ms | **52×** |
| Generated project, 588k lines | 409 ms | 10,633 ms | **26×** |

On large inputs jolt sustains about **1.4 million lines per second**, while javac manages
about 55 thousand. Reproduce with:

```sh
python3 bench/bench.py          # or --quick
```

Compile-and-run is fast too: `jolt run Algorithms.java` finishes in about 75 ms,
while Java's own source launcher (`java Algorithms.java`) takes about 520 ms.

> **Fair warning:** javac is a complete, production compiler for all of Java, and part
> of its time is JVM startup. jolt is fast partly *because* it does less (see
> [What's supported](#whats-supported)). For programs inside its subset, though, the
> output is the same and you get it 25–100× sooner.

## In the browser: jolt-vm

Browsers don't ship a JVM, so the playground brings its own. `web/` contains
**jolt-vm**, an interpreter for exactly the bytecode jolt emits. Every JDK method
jolt knows about is implemented natively, including `String`, `StringBuilder`,
`Math`, `Arrays`, `Scanner`, `Random` (bit-for-bit Java's algorithm), `String.format`/`printf`
(with Java's HALF_UP rounding), `Double.toString` (Java's digit rules), regex-based
`split`/`replaceAll`/`matches`, exceptions with real stack traces, and class initialization
order. The test suite checks that jolt-vm's output is **identical to the real JVM's**
for every test program, both natively and in the WebAssembly build.

| | |
|---|---|
| WebAssembly module | ~970 KB, no wasm-bindgen, no JS dependencies |
| Compile in the browser | about 1–2 ms for typical programs |
| Interpreter speed | about 50–85 million bytecode instructions per second |

Build it yourself with `web/build.sh` (needs `rustup target add wasm32-unknown-unknown`),
then serve `web/site/` with any static file server. You can also run programs on
jolt-vm natively: `cargo run --release -p jolt-web --bin jolt-vm -- Hello.java`.

## Install

```sh
git clone https://github.com/asherhawk-school/jolt
cd jolt
cargo build --release      # binary lands in target/release/jolt
```

You need a JVM (`java`) to *run* the compiled classes, but not to compile them.

## Usage

```sh
jolt Hello.java                   # writes Hello.class to the current directory
jolt -d out src/*.java            # compile several files into out/
jolt run Main.java arg1 arg2      # compile to a temp dir and run with `java`
jolt --time Big.java              # show per-phase timings
```

Errors are reported javac-style:

```text
TypeMismatch.java:4:17: error: incompatible types: possible lossy conversion from double to int
        int x = 3.5;
                ^
1 error
```

## What's supported

jolt targets the kind of Java found in coursework, algorithms practice and small tools.

**Language**
- Classes, abstract classes, interfaces, inheritance, method overriding and
  overloading (javac's most-specific-method rules), constructors with
  `this(...)`/`super(...)`, `super.method()` and `super.field`
- Static nested classes, multiple top-level classes per file, packages
- Fields, static and instance initializers, `static final` constants (inlined and
  folded at compile time), varargs methods
- Types: `int`, `long`, `double`, `char`, `boolean`, `String`, classes, and arrays of
  any depth
- All operators, including compound assignment, `++`/`--`, shifts, bitwise ops, ternary,
  `instanceof` and casts, all with Java's exact overflow, NaN and division semantics
- String concatenation with every type (`+` and `+=`)
- `if`, `while`, `do`, `for`, for-each over arrays, labeled `break`/`continue`, and
  `switch` on `int`/`char`/`String` (both `case X:` and `case X ->` forms)
- `throw`, `try`/`catch` (including multi-catch), and custom exception classes
- `var` for locals; javac-compatible checks for missing returns, unreachable code
  and type errors

**Library** (a built-in model of the commonly used JDK API)
- `String`, `StringBuilder`, `Math`, `Integer`/`Long`/`Double`/`Character`/`Boolean`
  helpers, `System`, `PrintStream` (`println`, `printf`), `String.format`,
  `java.util.Arrays`, `Scanner`, `Random`, `Thread.sleep`, and the standard
  exception classes

**Not supported (yet)**
- Generics and the collections framework (`ArrayList`, `HashMap`, …)
- Lambdas, method references, streams, anonymous and inner (non-static) classes
- Enums, records, switch *expressions*, pattern matching
- `finally` and try-with-resources
- `float`, `byte` and `short`; boxing beyond `Object` targets and varargs
- Checked-exception analysis (jolt accepts code that javac would reject for an
  unreported checked exception)

jolt tells you clearly when you hit one of these, rather than miscompiling.

## How it's so fast

1. **No JVM.** jolt is a native binary that starts in about 1 ms. javac needs a JVM
   that has to boot, load hundreds of classes and warm up its JIT before it compiles
   anything.
2. **A built-in model of the JDK.** Instead of opening and indexing the JDK's module
   image to resolve `String.length()`, jolt ships a compact description of the
   commonly used API, parsed in microseconds.
3. **Hand-written, single-pass front end.** A byte-level lexer and a
   recursive-descent parser with precedence climbing, with no parser generator and
   no token copying.
4. **Straight to bytecode.** One typed IR sits between checking and code generation,
   with constant folding and direct branch-on-condition lowering (`a < b && c` becomes
   compare-and-jump instructions, not boolean temporaries).
5. **Class-file version 49.** That version needs no `StackMapTable` frames, so jolt
   skips that whole analysis. The JVM verifies the code by type inference, and every
   modern JVM still loads version 49 classes.
6. **Parallel by default.** Big inputs are lexed, parsed, type-checked, compiled and
   written across all cores. The class table is built once and then shared read-only.
7. **Mechanical sympathy.** A per-thread size-class allocator, Fx hashing in place of
   SipHash, shared `Arc<str>` class names so cloning a type never allocates, and no
   teardown of trees that the OS is about to reclaim anyway.

## Correctness

Testing is differential: [`tests/run.sh`](tests/run.sh) compiles every program in
[`tests/programs`](tests/programs) with both jolt and javac, runs both on the JVM, and
requires identical output. It then runs each program on jolt-vm and requires the same
output again. [`web/test/wasm.sh`](web/test/wasm.sh) repeats that check for the
WebAssembly build, and [`tests/errors`](tests/errors) checks that invalid programs are
rejected with the right diagnostic. The benchmark also cross-checks a generated
half-million-line project.

```sh
tests/run.sh       # or: cargo test
web/test/wasm.sh   # needs node and the wasm32 target
```

## Project layout

| File | Purpose |
|---|---|
| `src/lexer.rs` | Byte-level tokenizer |
| `src/parser.rs` | Recursive-descent parser → `ast.rs` |
| `src/classes.rs` | Class table and the built-in JDK model |
| `src/declare.rs` | Registers classes, supertypes and members |
| `src/check.rs` | Name resolution, type checking, overloads, folding, reachability → `tir.rs` |
| `src/codegen.rs` | Typed IR → JVM bytecode |
| `src/classfile.rs` | Constant pool and class-file writer |
| `src/alloc.rs`, `src/fxhash.rs` | Performance plumbing |
| `src/lib.rs`, `src/main.rs` | Library API, CLI driver and parallel scheduling |
| `web/src/vm.rs` | jolt-vm: class loader and bytecode interpreter |
| `web/src/natives.rs` | jolt-vm's implementation of the JDK classes |
| `web/src/jfmt.rs`, `web/src/regex.rs` | Java-exact number formatting, a small regex engine |
| `web/src/wasm.rs`, `web/site/` | WebAssembly bindings and the playground page |

## License

MIT
