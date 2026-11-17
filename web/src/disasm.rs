//! A javap-style disassembler for the bytecode viewer in the playground.

use crate::vm::{parse_class, Cp};

const NAMES: &[&str] = &[
    "nop",
    "aconst_null",
    "iconst_m1",
    "iconst_0",
    "iconst_1",
    "iconst_2",
    "iconst_3",
    "iconst_4",
    "iconst_5",
    "lconst_0",
    "lconst_1",
    "fconst_0",
    "fconst_1",
    "fconst_2",
    "dconst_0",
    "dconst_1",
    "bipush",
    "sipush",
    "ldc",
    "ldc_w",
    "ldc2_w",
    "iload",
    "lload",
    "fload",
    "dload",
    "aload",
    "iload_0",
    "iload_1",
    "iload_2",
    "iload_3",
    "lload_0",
    "lload_1",
    "lload_2",
    "lload_3",
    "fload_0",
    "fload_1",
    "fload_2",
    "fload_3",
    "dload_0",
    "dload_1",
    "dload_2",
    "dload_3",
    "aload_0",
    "aload_1",
    "aload_2",
    "aload_3",
    "iaload",
    "laload",
    "faload",
    "daload",
    "aaload",
    "baload",
    "caload",
    "saload",
    "istore",
    "lstore",
    "fstore",
    "dstore",
    "astore",
    "istore_0",
    "istore_1",
    "istore_2",
    "istore_3",
    "lstore_0",
    "lstore_1",
    "lstore_2",
    "lstore_3",
    "fstore_0",
    "fstore_1",
    "fstore_2",
    "fstore_3",
    "dstore_0",
    "dstore_1",
    "dstore_2",
    "dstore_3",
    "astore_0",
    "astore_1",
    "astore_2",
    "astore_3",
    "iastore",
    "lastore",
    "fastore",
    "dastore",
    "aastore",
    "bastore",
    "castore",
    "sastore",
    "pop",
    "pop2",
    "dup",
    "dup_x1",
    "dup_x2",
    "dup2",
    "dup2_x1",
    "dup2_x2",
    "swap",
    "iadd",
    "ladd",
    "fadd",
    "dadd",
    "isub",
    "lsub",
    "fsub",
    "dsub",
    "imul",
    "lmul",
    "fmul",
    "dmul",
    "idiv",
    "ldiv",
    "fdiv",
    "ddiv",
    "irem",
    "lrem",
    "frem",
    "drem",
    "ineg",
    "lneg",
    "fneg",
    "dneg",
    "ishl",
    "lshl",
    "ishr",
    "lshr",
    "iushr",
    "lushr",
    "iand",
    "land",
    "ior",
    "lor",
    "ixor",
    "lxor",
    "iinc",
    "i2l",
    "i2f",
    "i2d",
    "l2i",
    "l2f",
    "l2d",
    "f2i",
    "f2l",
    "f2d",
    "d2i",
    "d2l",
    "d2f",
    "i2b",
    "i2c",
    "i2s",
    "lcmp",
    "fcmpl",
    "fcmpg",
    "dcmpl",
    "dcmpg",
    "ifeq",
    "ifne",
    "iflt",
    "ifge",
    "ifgt",
    "ifle",
    "if_icmpeq",
    "if_icmpne",
    "if_icmplt",
    "if_icmpge",
    "if_icmpgt",
    "if_icmple",
    "if_acmpeq",
    "if_acmpne",
    "goto",
    "jsr",
    "ret",
    "tableswitch",
    "lookupswitch",
    "ireturn",
    "lreturn",
    "freturn",
    "dreturn",
    "areturn",
    "return",
    "getstatic",
    "putstatic",
    "getfield",
    "putfield",
    "invokevirtual",
    "invokespecial",
    "invokestatic",
    "invokeinterface",
    "invokedynamic",
    "new",
    "newarray",
    "anewarray",
    "arraylength",
    "athrow",
    "checkcast",
    "instanceof",
    "monitorenter",
    "monitorexit",
    "wide",
    "multianewarray",
    "ifnull",
    "ifnonnull",
    "goto_w",
    "jsr_w",
];

fn flags_str(f: u16, method: bool) -> String {
    let mut v = Vec::new();
    if f & 0x0001 != 0 {
        v.push("public");
    }
    if f & 0x0002 != 0 {
        v.push("private");
    }
    if f & 0x0004 != 0 {
        v.push("protected");
    }
    if f & 0x0008 != 0 {
        v.push("static");
    }
    if f & 0x0010 != 0 {
        v.push("final");
    }
    if method && f & 0x0400 != 0 {
        v.push("abstract");
    }
    let mut s = v.join(" ");
    if !s.is_empty() {
        s.push(' ');
    }
    s
}

/// Java source spelling of one field descriptor starting at `*i`.
fn java_type(d: &[u8], i: &mut usize) -> String {
    let c = d.get(*i).copied().unwrap_or(b'V');
    *i += 1;
    match c {
        b'Z' => "boolean".into(),
        b'C' => "char".into(),
        b'B' => "byte".into(),
        b'S' => "short".into(),
        b'I' => "int".into(),
        b'J' => "long".into(),
        b'F' => "float".into(),
        b'D' => "double".into(),
        b'V' => "void".into(),
        b'[' => format!("{}[]", java_type(d, i)),
        b'L' => {
            let start = *i;
            while *i < d.len() && d[*i] != b';' {
                *i += 1;
            }
            let name = String::from_utf8_lossy(&d[start..*i]).replace('/', ".");
            *i += 1;
            name
        }
        _ => "?".into(),
    }
}

fn field_sig(desc: &str) -> String {
    java_type(desc.as_bytes(), &mut 0)
}

/// `public static void main(java.lang.String[])` style signature.
fn method_sig(class: &str, name: &str, desc: &str) -> String {
    let d = desc.as_bytes();
    let mut i = 1;
    let mut params = Vec::new();
    while i < d.len() && d[i] != b')' {
        params.push(java_type(d, &mut i));
    }
    i += 1;
    let ret = java_type(d, &mut i);
    match name {
        "<init>" => format!("{}({})", class.replace('/', "."), params.join(", ")),
        "<clinit>" => "static {}".to_string(),
        _ => format!("{} {}({})", ret, name, params.join(", ")),
    }
}

pub fn disassemble(bytes: &[u8]) -> String {
    let cf = match parse_class(bytes) {
        Ok(c) => c,
        Err(e) => return format!("// cannot parse class: {}\n", e),
    };
    let utf = |i: u16| -> String {
        match cf.cp.get(i as usize) {
            Some(Cp::Utf8(s)) => s.to_string(),
            _ => "?".into(),
        }
    };
    let cname = |i: u16| -> String {
        match cf.cp.get(i as usize) {
            Some(Cp::Class(u)) => utf(*u),
            _ => "?".into(),
        }
    };
    let describe = |i: u16| -> String {
        match cf.cp.get(i as usize) {
            Some(Cp::Int(v)) => format!("int {}", v),
            Some(Cp::Long(v)) => format!("long {}l", v),
            Some(Cp::Double(v)) => format!("double {}d", crate::jfmt::double_to_string(*v)),
            Some(Cp::Str(u)) => {
                let s = utf(*u);
                let s: String = s
                    .chars()
                    .flat_map(|c| if c == '\n' { vec!['\\', 'n'] } else { vec![c] })
                    .collect();
                format!("String {}", s)
            }
            Some(Cp::Class(_)) => format!("class {}", cname(i)),
            Some(Cp::Member(c, nt)) => match cf.cp.get(*nt as usize) {
                Some(Cp::NameType(n, d)) => format!("{}.{}:{}", cname(*c), utf(*n), utf(*d)),
                _ => "?".into(),
            },
            _ => "?".into(),
        }
    };
    let mut out = String::new();
    let kind = if cf.flags & 0x0200 != 0 { "interface" } else { "class" };
    out.push_str(&format!(
        "// Compiled from \"{}\" (class file version 49.0)\n",
        cf.source
    ));
    out.push_str(&format!(
        "{}{} {}",
        flags_str(cf.flags & 0x0011, false),
        kind,
        cf.name.replace('/', ".")
    ));
    if let Some(s) = &cf.super_name {
        if &**s != "java/lang/Object" {
            out.push_str(&format!(" extends {}", s.replace('/', ".")));
        }
    }
    if !cf.interfaces.is_empty() {
        let names: Vec<String> = cf.interfaces.iter().map(|i| i.replace('/', ".")).collect();
        out.push_str(&format!(" implements {}", names.join(", ")));
    }
    out.push_str(" {\n");
    for (f, n, d) in &cf.fields {
        out.push_str(&format!("  {}{} {};\n", flags_str(*f, false), field_sig(d), n));
    }
    if !cf.fields.is_empty() {
        out.push('\n');
    }
    for m in &cf.methods {
        let flags = if &*m.name == "<clinit>" {
            String::new()
        } else {
            flags_str(m.flags, true)
        };
        out.push_str(&format!("  {}{};\n", flags, method_sig(&cf.name, &m.name, &m.desc)));
        let Some(code) = &m.code else {
            out.push('\n');
            continue;
        };
        out.push_str(&format!(
            "    Code: locals={}, length={}\n",
            code.max_locals,
            code.code.len()
        ));
        let c = &code.code;
        let mut pc = 0;
        while pc < c.len() {
            let op = c[pc] as usize;
            let name = NAMES.get(op).copied().unwrap_or("???");
            let u2 = |p: usize| ((c[p] as u16) << 8) | c[p + 1] as u16;
            let s2 = |p: usize| u2(p) as i16 as i64;
            let rd4 = |p: usize| i32::from_be_bytes([c[p], c[p + 1], c[p + 2], c[p + 3]]);
            let (args, len): (String, usize) = match op {
                16 => (format!("{}", c[pc + 1] as i8), 2),
                17 => (format!("{}", s2(pc + 1)), 3),
                18 => (format!("#{:<14}// {}", c[pc + 1], describe(c[pc + 1] as u16)), 2),
                19 | 20 | 178..=184 | 187 | 189 | 192 | 193 => {
                    (format!("#{:<14}// {}", u2(pc + 1), describe(u2(pc + 1))), 3)
                }
                185 => (format!("#{:<14}// {}", u2(pc + 1), describe(u2(pc + 1))), 5),
                197 => (
                    format!("#{}, {:<10}// {}", u2(pc + 1), c[pc + 3], describe(u2(pc + 1))),
                    4,
                ),
                21..=25 | 54..=58 => (format!("{}", c[pc + 1]), 2),
                132 => (format!("{}, {}", c[pc + 1], c[pc + 2] as i8), 3),
                153..=168 | 198 | 199 => (format!("{}", pc as i64 + s2(pc + 1)), 3),
                188 => (
                    match c[pc + 1] {
                        4 => "boolean",
                        5 => "char",
                        7 => "double",
                        10 => "int",
                        11 => "long",
                        _ => "?",
                    }
                    .to_string(),
                    2,
                ),
                170 | 171 => {
                    let mut p = (pc + 4) & !3;
                    let default = rd4(p);
                    p += 4;
                    let mut cases = Vec::new();
                    if op == 170 {
                        let (lo, hi) = (rd4(p), rd4(p + 4));
                        p += 8;
                        for k in lo..=hi {
                            cases.push(format!("{}: {}", k, pc as i64 + rd4(p) as i64));
                            p += 4;
                        }
                    } else {
                        let n = rd4(p);
                        p += 4;
                        for _ in 0..n {
                            cases.push(format!("{}: {}", rd4(p), pc as i64 + rd4(p + 4) as i64));
                            p += 8;
                        }
                    }
                    cases.push(format!("default: {}", pc as i64 + default as i64));
                    (format!("{{ {} }}", cases.join(", ")), p - pc)
                }
                196 => {
                    let wop = c[pc + 1] as usize;
                    let wn = NAMES.get(wop).copied().unwrap_or("?");
                    if wop == 132 {
                        (format!("{} {}, {}", wn, u2(pc + 2), s2(pc + 4)), 6)
                    } else {
                        (format!("{} {}", wn, u2(pc + 2)), 4)
                    }
                }
                _ => (String::new(), 1),
            };
            out.push_str(&format!("      {:>4}: {:<14}{}\n", pc, name, args).trim_end());
            out.push('\n');
            pc += len;
        }
        if !code.exc.is_empty() {
            out.push_str("    Exception table:\n       from    to  target type\n");
            for e in &code.exc {
                let t = if e[3] == 0 { "any".into() } else { cname(e[3]) };
                out.push_str(&format!("      {:>5} {:>5} {:>7}   {}\n", e[0], e[1], e[2], t));
            }
        }
        out.push('\n');
    }
    out.push_str("}\n\n");
    out
}
