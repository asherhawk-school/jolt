//! Native front end for jolt-vm, used to test the browser JVM against the
//! real one: `jolt-vm File.java [args...] < input`.

use std::io::{Read, Write};
use std::time::Instant;

struct Native {
    start: Instant,
}

impl jolt_web::Host for Native {
    fn write(&mut self, fd: u32, s: &str) {
        match fd {
            1 => {
                let _ = std::io::stdout().write_all(s.as_bytes());
            }
            2 | 3 => {
                let _ = std::io::stdout().flush();
                let _ = std::io::stderr().write_all(s.as_bytes());
            }
            5 => {
                let _ = std::io::stdout().write_all(s.as_bytes());
            }
            _ => {
                if std::env::var_os("JOLT_VM_STATUS").is_some() {
                    let _ = std::io::stderr().write_all(s.as_bytes());
                }
            }
        }
    }
    fn now_ms(&mut self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }
    fn epoch_ms(&mut self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }
    fn random(&mut self) -> f64 {
        // xorshift seeded from the clock; Math.random() needs no stronger guarantees
        let mut x = (self.epoch_ms() * 1000.0) as u64 | 1;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
    fn sleep_ms(&mut self, ms: f64) {
        std::thread::sleep(std::time::Duration::from_secs_f64(ms / 1000.0));
    }
    fn read_stdin(&mut self) -> String {
        let mut input = String::new();
        let _ = std::io::stdin().read_to_string(&mut input);
        input
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let disasm = args.first().map_or(false, |a| a == "--disasm");
    let args: Vec<String> = if disasm { args[1..].to_vec() } else { args };
    let Some(path) = args.first() else {
        eprintln!("usage: jolt-vm [--disasm] File.java [args...] < input");
        std::process::exit(2);
    };
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("jolt-vm: cannot read {}: {}", path, e);
            std::process::exit(2);
        }
    };
    let host = Box::new(Native { start: Instant::now() });
    let code = jolt_web::compile_and_run(host, &source, None, &args[1..], !disasm, disasm);
    let _ = std::io::stdout().flush();
    std::process::exit(code);
}
