//! Native implementations of the slice of the JDK that jolt knows about.

use std::cell::RefCell;
use std::rc::Rc;

use crate::jfmt;
use crate::regex::Regex;
use crate::vm::*;

pub type Nf = fn(&mut Vm, &[V]) -> R;

// ---------------------------------------------------------------- helpers

pub struct Scanner {
    pub buf: Vec<u16>,
    pub pos: usize,
    /// No more input will ever arrive. Always true, except for System.in in
    /// interactive mode, where the user may still type more.
    pub eof: bool,
}

/// The answer depends on input that has not been typed yet.
pub struct NeedMore;

impl Scanner {
    pub fn new(s: &str) -> Scanner {
        Scanner {
            buf: s.encode_utf16().collect(),
            pos: 0,
            eof: true,
        }
    }
    fn is_ws(c: u16) -> bool {
        matches!(c, 9..=13 | 28..=32) || c == 0x85 || c == 0xA0 || c == 0x2028 || c == 0x2029
    }
    fn skip_ws(&self) -> usize {
        let mut i = self.pos;
        while i < self.buf.len() && Self::is_ws(self.buf[i]) {
            i += 1;
        }
        i
    }
    /// The next complete token, or `None` when the input has ended.
    fn token(&self, eof: bool) -> Result<Option<(usize, usize)>, NeedMore> {
        let start = self.skip_ws();
        if start >= self.buf.len() {
            return if eof { Ok(None) } else { Err(NeedMore) };
        }
        let mut i = start;
        while i < self.buf.len() && !Self::is_ws(self.buf[i]) {
            i += 1;
        }
        if i == self.buf.len() && !eof {
            return Err(NeedMore); // the token may continue
        }
        Ok(Some((start, i)))
    }
    fn has_token(&self, eof: bool) -> Result<bool, NeedMore> {
        if self.skip_ws() < self.buf.len() {
            Ok(true)
        } else if eof {
            Ok(false)
        } else {
            Err(NeedMore)
        }
    }
    /// The current line: (end of its text, start of the next line).
    fn line(&self, eof: bool) -> Result<Option<(usize, usize)>, NeedMore> {
        let mut i = self.pos;
        while i < self.buf.len() && self.buf[i] != 10 && self.buf[i] != 13 {
            i += 1;
        }
        if i < self.buf.len() {
            let mut next = i + 1;
            if self.buf[i] == 13 && next < self.buf.len() && self.buf[next] == 10 {
                next += 1;
            }
            return Ok(Some((i, next)));
        }
        if !eof {
            return Err(NeedMore);
        }
        Ok((self.pos < self.buf.len()).then_some((i, i)))
    }
    fn has_line(&self, eof: bool) -> Result<bool, NeedMore> {
        if self.pos < self.buf.len() {
            Ok(true)
        } else if eof {
            Ok(false)
        } else {
            Err(NeedMore)
        }
    }
    fn text(&self, a: usize, b: usize) -> String {
        String::from_utf16_lossy(&self.buf[a..b])
    }
}

/// Ask the scanner a question. If the answer needs input that hasn't been typed
/// yet, pause the program until it is (interactive mode), or else treat the
/// input as finished, like a closed stdin.
fn scan<T>(vm: &mut Vm, st: &Rc<RefCell<Scanner>>, f: impl Fn(&Scanner, bool) -> Result<T, NeedMore>) -> Result<T, Ex> {
    vm.flush();
    let eof = st.borrow().eof;
    if let Ok(t) = f(&st.borrow(), eof) {
        return Ok(t);
    }
    if vm.can_suspend() {
        return Err(Ex::NeedInput);
    }
    match f(&st.borrow(), true) {
        Ok(t) => Ok(t),
        Err(NeedMore) => Err(Ex::Fatal("scanner needs input after end of input".into())),
    }
}

fn u16s(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

pub fn rust_string(v: &V) -> Option<String> {
    match v {
        V::R(o) => match &*o.data.borrow() {
            Data::Str(s) => Some(String::from_utf16_lossy(s)),
            _ => None,
        },
        _ => None,
    }
}

fn npe(vm: &mut Vm) -> Ex {
    vm.throw("java/lang/NullPointerException", None)
}

/// Characters of a String or StringBuilder (CharSequence).
fn str_of(vm: &mut Vm, v: &V) -> Result<Rc<[u16]>, Ex> {
    match v {
        V::R(o) => match &*o.data.borrow() {
            Data::Str(s) => Ok(s.clone()),
            Data::Sb(s) => Ok(s.clone().into()),
            _ => Err(Ex::Fatal("expected a string".into())),
        },
        _ => Err(npe(vm)),
    }
}

fn ret_s(vm: &mut Vm, units: Vec<u16>) -> R {
    Ok(Some(vm.str_from(units.into())))
}
fn ret_rs(vm: &mut Vm, s: &str) -> R {
    Ok(Some(vm.jstr(s)))
}
fn ok_i(x: i32) -> R {
    Ok(Some(V::I(x)))
}
fn ok_l(x: i64) -> R {
    Ok(Some(V::L(x)))
}
fn ok_d(x: f64) -> R {
    Ok(Some(V::D(x)))
}
fn ok_z(x: bool) -> R {
    Ok(Some(V::I(x as i32)))
}

fn set_data(v: &V, d: Data) {
    if let V::R(o) = v {
        *o.data.borrow_mut() = d;
    }
}

fn char_array(vm: &mut Vm, v: &V) -> Result<Vec<u16>, Ex> {
    match v {
        V::R(o) => match &*o.data.borrow() {
            Data::ArrC(c) => Ok(c.clone()),
            _ => Err(Ex::Fatal("expected char[]".into())),
        },
        _ => Err(npe(vm)),
    }
}

fn sb<T>(v: &V, f: impl FnOnce(&mut Vec<u16>) -> T) -> T {
    let V::R(o) = v else { panic!("null StringBuilder") };
    let mut d = o.data.borrow_mut();
    match &mut *d {
        Data::Sb(s) => f(s),
        _ => panic!("not a StringBuilder"),
    }
}

fn ch_of(c: u32) -> Option<char> {
    char::from_u32(c)
}

pub fn java_hash(s: &[u16]) -> i32 {
    s.iter().fold(0i32, |h, &c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// `String.valueOf(Object)`: "null", or the object's (virtual) toString().
pub fn to_jstring(vm: &mut Vm, v: &V) -> Result<Rc<[u16]>, Ex> {
    match v {
        V::Null => Ok(u16s("null").into()),
        V::I(x) => Ok(u16s(&x.to_string()).into()),
        V::L(x) => Ok(u16s(&x.to_string()).into()),
        V::D(x) => Ok(u16s(&jfmt::double_to_string(*x)).into()),
        V::R(o) => {
            if let Data::Str(s) = &*o.data.borrow() {
                return Ok(s.clone());
            }
            let r = vm.call_virtual(v.clone(), "toString", "()Ljava/lang/String;", vec![])?;
            match r {
                Some(V::R(s)) => match &*s.data.borrow() {
                    Data::Str(s) => Ok(s.clone()),
                    _ => Ok(u16s("null").into()),
                },
                _ => Ok(u16s("null").into()),
            }
        }
    }
}

fn to_rstring(vm: &mut Vm, v: &V) -> Result<String, Ex> {
    Ok(String::from_utf16_lossy(&to_jstring(vm, v)?))
}

pub fn throwable_to_string(vm: &mut Vm, t: &Ref) -> String {
    to_rstring(vm, &V::R(t.clone())).unwrap_or_else(|_| vm.class_name(t))
}

fn throwable_fields(v: &V) -> Option<(V, V, V)> {
    match v {
        V::R(o) => match &*o.data.borrow() {
            Data::Fields(f) if f.len() >= 3 => Some((f[0].clone(), f[1].clone(), f[2].clone())),
            _ => None,
        },
        _ => None,
    }
}

const BOX_KINDS: &[(u8, &str)] = &[
    (b'I', "java/lang/Integer"),
    (b'J', "java/lang/Long"),
    (b'D', "java/lang/Double"),
    (b'C', "java/lang/Character"),
    (b'Z', "java/lang/Boolean"),
];

fn box_value(vm: &mut Vm, kind: u8, v: V) -> V {
    let small = match (&v, kind) {
        (V::I(x), b'I') if (-128..=127).contains(x) => Some(*x),
        (V::I(x), b'C') if (0..=127).contains(x) => Some(*x),
        (V::I(x), b'Z') => Some(*x),
        (V::L(x), b'J') if (-128..=127).contains(x) => Some(*x as i32),
        _ => None,
    };
    if let Some(k) = small {
        if let Some(r) = vm.boxed_cache.get(&(kind, k)) {
            return V::R(r.clone());
        }
    }
    let cls = BOX_KINDS.iter().find(|b| b.0 == kind).unwrap().1;
    let id = vm.class_id(cls).ok().unwrap();
    let r = vm.new_obj(id, Data::Boxed(v));
    if let Some(k) = small {
        vm.boxed_cache.insert((kind, k), r.clone());
    }
    V::R(r)
}

fn unbox(v: &V) -> Option<V> {
    match v {
        V::R(o) => match &*o.data.borrow() {
            Data::Boxed(b) => Some(b.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn box_kind(vm: &Vm, o: &Ref) -> u8 {
    let name = &vm.classes[o.cls].name;
    BOX_KINDS.iter().find(|b| *b.1 == **name).map(|b| b.0).unwrap_or(0)
}

fn boxed_to_string(vm: &Vm, o: &Ref) -> String {
    let k = box_kind(vm, o);
    let v = unbox(&V::R(o.clone())).unwrap_or(V::Null);
    match (k, v) {
        (b'D', V::D(d)) => jfmt::double_to_string(d),
        (b'C', V::I(c)) => String::from_utf16_lossy(&[c as u16]),
        (b'Z', V::I(z)) => (if z != 0 { "true" } else { "false" }).into(),
        (_, V::I(x)) => x.to_string(),
        (_, V::L(x)) => x.to_string(),
        _ => "?".into(),
    }
}

fn parse_int(vm: &mut Vm, s: &V, radix: u32) -> Result<i64, Ex> {
    let Some(st) = rust_string(s) else {
        return Err(vm.throw(
            "java/lang/NumberFormatException",
            Some("Cannot parse null string: null"),
        ));
    };
    let fail = |vm: &mut Vm| {
        let msg = if radix == 10 {
            format!("For input string: \"{}\"", st)
        } else {
            format!("For input string: \"{}\" under radix {}", st, radix)
        };
        vm.throw("java/lang/NumberFormatException", Some(&msg))
    };
    let body = st.strip_prefix('+').unwrap_or(&st);
    if body.is_empty()
        || body.starts_with('+')
        || body == "-"
        || (body.starts_with('-') && body[1..].starts_with(['+', '-']))
    {
        return Err(fail(vm));
    }
    match i128::from_str_radix(body, radix) {
        Ok(v) if v >= i64::MIN as i128 && v <= i64::MAX as i128 => Ok(v as i64),
        _ => Err(fail(vm)),
    }
}

fn parse_double(vm: &mut Vm, s: &V) -> Result<f64, Ex> {
    let Some(st) = rust_string(s) else { return Err(npe(vm)) };
    let t = st.trim_matches(|c: char| c <= ' ');
    let t2 = t.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(t);
    let parsed = match t2 {
        "NaN" | "+NaN" | "-NaN" => Some(f64::NAN),
        "Infinity" | "+Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ if t2.chars().any(|c| c.is_ascii_alphabetic() && !matches!(c, 'e' | 'E')) => None,
        _ => t2.parse::<f64>().ok(),
    };
    match parsed {
        Some(d) => Ok(d),
        None => {
            let msg = if t.is_empty() {
                "empty String".to_string()
            } else {
                format!("For input string: \"{}\"", st)
            };
            Err(vm.throw("java/lang/NumberFormatException", Some(&msg)))
        }
    }
}

fn arr_len(v: &V) -> Option<usize> {
    match v {
        V::R(o) => Some(match &*o.data.borrow() {
            Data::ArrI(a) => a.len(),
            Data::ArrJ(a) => a.len(),
            Data::ArrD(a) => a.len(),
            Data::ArrZ(a) => a.len(),
            Data::ArrC(a) => a.len(),
            Data::ArrA(a) => a.len(),
            _ => return None,
        }),
        _ => None,
    }
}

fn arr_to_string(vm: &mut Vm, v: &V) -> Result<String, Ex> {
    let V::R(o) = v else { return Ok("null".into()) };
    let parts: Vec<String> = {
        let d = o.data.borrow();
        match &*d {
            Data::ArrI(a) => a.iter().map(|x| x.to_string()).collect(),
            Data::ArrJ(a) => a.iter().map(|x| x.to_string()).collect(),
            Data::ArrD(a) => a.iter().map(|x| jfmt::double_to_string(*x)).collect(),
            Data::ArrZ(a) => a
                .iter()
                .map(|x| (if *x != 0 { "true" } else { "false" }).to_string())
                .collect(),
            Data::ArrC(a) => a.iter().map(|x| String::from_utf16_lossy(&[*x])).collect(),
            Data::ArrA(a) => {
                let items = a.clone();
                drop(d);
                let mut out = Vec::with_capacity(items.len());
                for it in &items {
                    out.push(to_rstring(vm, it)?);
                }
                out
            }
            _ => return Ok("?".into()),
        }
    };
    Ok(format!("[{}]", parts.join(", ")))
}

fn deep_to_string(vm: &mut Vm, v: &V, seen: &mut Vec<usize>) -> Result<String, Ex> {
    let V::R(o) = v else { return Ok("null".into()) };
    let items = match &*o.data.borrow() {
        Data::ArrA(a) => a.clone(),
        _ => return Err(Ex::Fatal("deepToString expects Object[]".into())),
    };
    seen.push(Rc::as_ptr(o) as usize);
    let mut parts = Vec::new();
    for it in &items {
        let is_arr = arr_len(it).is_some();
        let is_obj_arr = matches!(it, V::R(r) if matches!(&*r.data.borrow(), Data::ArrA(_)));
        if is_obj_arr {
            let p = Rc::as_ptr(it.obj().unwrap()) as usize;
            if seen.contains(&p) {
                parts.push("[...]".to_string());
            } else {
                parts.push(deep_to_string(vm, it, seen)?);
            }
        } else if is_arr {
            parts.push(arr_to_string(vm, it)?);
        } else {
            parts.push(to_rstring(vm, it)?);
        }
    }
    seen.pop();
    Ok(format!("[{}]", parts.join(", ")))
}

fn java_double_cmp(a: f64, b: f64) -> std::cmp::Ordering {
    let ka = if a.is_nan() { f64::NAN.abs() } else { a };
    let kb = if b.is_nan() { f64::NAN.abs() } else { b };
    ka.total_cmp(&kb)
}

fn compare_objects(vm: &mut Vm, a: &V, b: &V) -> Result<i32, Ex> {
    if matches!(a, V::Null) || matches!(b, V::Null) {
        return Err(npe(vm));
    }
    if let (Some(x), Some(y)) = (rust_str16(a), rust_str16(b)) {
        return Ok(str_cmp(&x, &y));
    }
    if let (Some(x), Some(y)) = (unbox(a), unbox(b)) {
        return Ok(match (x, y) {
            (V::I(p), V::I(q)) => p.cmp(&q) as i32,
            (V::L(p), V::L(q)) => p.cmp(&q) as i32,
            (V::D(p), V::D(q)) => java_double_cmp(p, q) as i32,
            _ => 0,
        });
    }
    let o = a.obj().unwrap().clone();
    let target = match vm.lookup_virtual(o.cls, "compareTo", "(Ljava/lang/Object;)I") {
        Target::Abstract => {
            // a user compareTo(Foo) without a bridge method
            let mut found = None;
            let mut c = Some(o.cls);
            while let Some(id) = c {
                if let Some(m) = vm.classes[id]
                    .methods
                    .iter()
                    .find(|m| &*m.name == "compareTo" && m.nargs == 1 && !m.is_static && !m.is_abstract)
                {
                    found = Some(m.clone());
                    break;
                }
                c = vm.classes[id].sup;
            }
            match found {
                Some(m) => Target::Code(m),
                None => {
                    let msg = format!(
                        "class {} cannot be cast to class java.lang.Comparable ({} is in unnamed module of loader 'app'; java.lang.Comparable is in module java.base of loader 'bootstrap')",
                        vm.class_name(&o),
                        vm.class_name(&o)
                    );
                    return Err(vm.throw("java/lang/ClassCastException", Some(&msg)));
                }
            }
        }
        t => t,
    };
    let r = match target {
        Target::Code(m) => vm.invoke(m, vec![a.clone(), b.clone()])?,
        Target::Native(n) => {
            let f = vm.natives[n].1;
            f(vm, &[a.clone(), b.clone()])?
        }
        Target::Abstract => None,
    };
    Ok(r.map(|v| v.i()).unwrap_or(0))
}

fn rust_str16(v: &V) -> Option<Rc<[u16]>> {
    match v {
        V::R(o) => match &*o.data.borrow() {
            Data::Str(s) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn str_cmp(a: &[u16], b: &[u16]) -> i32 {
    for (x, y) in a.iter().zip(b) {
        if x != y {
            return *x as i32 - *y as i32;
        }
    }
    a.len() as i32 - b.len() as i32
}

fn upper16(c: u16) -> u16 {
    ch_of(c as u32)
        .map(|ch| {
            let mut u = ch.to_uppercase();
            match (u.next(), u.next()) {
                (Some(x), None) if (x as u32) <= 0xFFFF => x as u32 as u16,
                _ => c,
            }
        })
        .unwrap_or(c)
}

fn lower16(c: u16) -> u16 {
    ch_of(c as u32)
        .map(|ch| {
            let mut u = ch.to_lowercase();
            match (u.next(), u.next()) {
                (Some(x), None) if (x as u32) <= 0xFFFF => x as u32 as u16,
                _ => c,
            }
        })
        .unwrap_or(c)
}

fn java_whitespace(c: u32) -> bool {
    match c {
        9..=13 | 28..=31 => true,
        0xA0 | 0x2007 | 0x202F => false,
        _ => ch_of(c).map_or(false, |ch| ch.is_whitespace()),
    }
}

fn index_of(hay: &[u16], needle: &[u16], from: i32) -> i32 {
    let from = from.max(0) as usize;
    if needle.is_empty() {
        return from.min(hay.len()) as i32;
    }
    if needle.len() > hay.len() {
        return -1;
    }
    (from..=hay.len() - needle.len())
        .find(|&i| hay[i..i + needle.len()] == *needle)
        .map_or(-1, |i| i as i32)
}

fn last_index_of(hay: &[u16], needle: &[u16]) -> i32 {
    if needle.len() > hay.len() {
        return -1;
    }
    (0..=hay.len() - needle.len())
        .rev()
        .find(|&i| hay[i..i + needle.len()] == *needle)
        .map_or(-1, |i| i as i32)
}

fn code_point_units(c: i32) -> Vec<u16> {
    match ch_of(c as u32) {
        Some(ch) => {
            let mut b = [0u16; 2];
            ch.encode_utf16(&mut b).to_vec()
        }
        None => vec![c as u16],
    }
}

fn regex(vm: &mut Vm, v: &V) -> Result<Regex, Ex> {
    let p = str_of(vm, v)?;
    Regex::new(&p).map_err(|e| {
        let msg = format!("{}\n{}", e, String::from_utf16_lossy(&p));
        vm.throw("java/util/regex/PatternSyntaxException", Some(&msg))
    })
}

fn sioobe(vm: &mut Vm, msg: String) -> Ex {
    vm.throw("java/lang/StringIndexOutOfBoundsException", Some(&msg))
}

fn print_to(vm: &mut Vm, stream: &V, s: &str) -> Result<(), Ex> {
    let fd = match stream {
        V::R(o) => match &*o.data.borrow() {
            Data::Stream(fd) => *fd,
            _ => 1,
        },
        _ => return Err(npe(vm)),
    };
    vm.emit(fd, s);
    Ok(())
}

fn bool_str(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

// ---------------------------------------------------------------- formatter

fn fmt_err(vm: &mut Vm, cls: &str, msg: String) -> Ex {
    vm.throw(cls, Some(&msg))
}

/// `java.util.Formatter` for the common conversions.
pub fn java_format(vm: &mut Vm, fmt: &[u16], args: &[V]) -> Result<String, Ex> {
    let f: Vec<char> = String::from_utf16_lossy(fmt).chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut next_arg = 0usize;
    let mut last_arg: Option<usize> = None;
    while i < f.len() {
        if f[i] != '%' {
            out.push(f[i]);
            i += 1;
            continue;
        }
        let spec_start = i;
        i += 1;
        // argument index or flags/width
        let mut explicit: Option<usize> = None;
        let mut relative = false;
        let save = i;
        let mut num = String::new();
        while i < f.len() && f[i].is_ascii_digit() {
            num.push(f[i]);
            i += 1;
        }
        if i < f.len() && f[i] == '$' && !num.is_empty() {
            explicit = num.parse::<usize>().ok().map(|n| n.saturating_sub(1));
            i += 1;
        } else {
            i = save;
        }
        let mut flags = String::new();
        while i < f.len() && "-#+ 0,(<".contains(f[i]) {
            if f[i] == '<' {
                relative = true;
            } else {
                flags.push(f[i]);
            }
            i += 1;
        }
        let mut width: Option<usize> = None;
        let mut w = String::new();
        while i < f.len() && f[i].is_ascii_digit() {
            w.push(f[i]);
            i += 1;
        }
        if !w.is_empty() {
            width = w.parse().ok();
        }
        let mut prec: Option<usize> = None;
        if i < f.len() && f[i] == '.' {
            i += 1;
            let mut p = String::new();
            while i < f.len() && f[i].is_ascii_digit() {
                p.push(f[i]);
                i += 1;
            }
            prec = p.parse().ok();
        }
        if i >= f.len() {
            let s: String = f[spec_start..].iter().collect();
            return Err(fmt_err(
                vm,
                "java/util/UnknownFormatConversionException",
                format!("Conversion = '{}'", s),
            ));
        }
        let conv = f[i];
        i += 1;
        let spec: String = f[spec_start..i].iter().collect();
        if conv == 'n' {
            out.push('\n');
            continue;
        }
        if conv == '%' {
            let s = "%".to_string();
            out.push_str(&pad(&s, width, flags.contains('-'), false));
            continue;
        }
        if !"bBhHsScCdoxXeEfgG".contains(conv) {
            return Err(fmt_err(
                vm,
                "java/util/UnknownFormatConversionException",
                format!("Conversion = '{}'", conv),
            ));
        }
        let idx = if relative {
            match last_arg {
                Some(l) => l,
                None => {
                    return Err(fmt_err(
                        vm,
                        "java/util/MissingFormatArgumentException",
                        format!("Format specifier '{}'", spec),
                    ))
                }
            }
        } else if let Some(e) = explicit {
            e
        } else {
            next_arg += 1;
            next_arg - 1
        };
        last_arg = Some(idx);
        let Some(arg) = args.get(idx).cloned() else {
            return Err(fmt_err(
                vm,
                "java/util/MissingFormatArgumentException",
                format!("Format specifier '{}'", spec),
            ));
        };
        let left = flags.contains('-');
        let upper = conv.is_ascii_uppercase();
        let body: String = match conv.to_ascii_lowercase() {
            'b' => {
                let b = match &arg {
                    V::Null => false,
                    _ => match unbox(&arg) {
                        Some(V::I(z)) if box_kind(vm, arg.obj().unwrap()) == b'Z' => z != 0,
                        _ => true,
                    },
                };
                let s = bool_str(b).to_string();
                truncate(s, prec)
            }
            'h' => match &arg {
                V::Null => "null".into(),
                _ => {
                    let h = vm
                        .call_virtual(arg.clone(), "hashCode", "()I", vec![])?
                        .map(|v| v.i())
                        .unwrap_or(0);
                    format!("{:x}", h as u32)
                }
            },
            's' => truncate(to_rstring(vm, &arg)?, prec),
            'c' => match &arg {
                V::Null => "null".into(),
                _ => match unbox(&arg) {
                    Some(V::I(c)) => String::from_utf16_lossy(&code_point_units(c)),
                    _ => {
                        let n = vm.class_name(arg.obj().unwrap());
                        return Err(fmt_err(
                            vm,
                            "java/util/IllegalFormatConversionException",
                            format!("c != {}", n),
                        ));
                    }
                },
            },
            'd' | 'o' | 'x' => {
                let (v, bits): (i64, u32) = match unbox(&arg) {
                    Some(V::I(x)) if box_kind(vm, arg.obj().unwrap()) == b'I' => (x as i64, 32),
                    Some(V::L(x)) => (x, 64),
                    _ => {
                        if matches!(arg, V::Null) {
                            out.push_str(&pad("null", width, left, false));
                            continue;
                        }
                        let n = vm.class_name(arg.obj().unwrap());
                        return Err(fmt_err(
                            vm,
                            "java/util/IllegalFormatConversionException",
                            format!("{} != {}", conv, n),
                        ));
                    }
                };
                if conv == 'd' {
                    let neg = v < 0;
                    let mut digits = v.unsigned_abs().to_string();
                    if flags.contains(',') {
                        digits = jfmt::group(&digits);
                    }
                    signed(digits, neg, &flags, width)
                } else {
                    let u = if bits == 32 { v as u32 as u64 } else { v as u64 };
                    let r = if conv.to_ascii_lowercase() == 'x' { 16 } else { 8 };
                    let mut s = jfmt::radix(u, r);
                    if flags.contains('#') {
                        s = format!("{}{}", if r == 16 { "0x" } else { "0" }, s);
                    }
                    if flags.contains('0') {
                        if let Some(w) = width {
                            while s.len() < w {
                                s.insert(0, '0');
                            }
                        }
                    }
                    s
                }
            }
            'e' | 'f' | 'g' => {
                let d = match unbox(&arg) {
                    Some(V::D(d)) => d,
                    _ => {
                        if matches!(arg, V::Null) {
                            out.push_str(&pad("null", width, left, false));
                            continue;
                        }
                        let n = vm.class_name(arg.obj().unwrap());
                        return Err(fmt_err(
                            vm,
                            "java/util/IllegalFormatConversionException",
                            format!("{} != {}", conv, n),
                        ));
                    }
                };
                if d.is_nan() || d.is_infinite() {
                    let s = if d.is_nan() {
                        "NaN".to_string()
                    } else if d > 0.0 {
                        if flags.contains('+') {
                            "+Infinity".into()
                        } else {
                            "Infinity".into()
                        }
                    } else if flags.contains('(') {
                        "(Infinity)".into()
                    } else {
                        "-Infinity".into()
                    };
                    out.push_str(&pad(&s, width, left, false));
                    continue;
                }
                let p = prec.unwrap_or(6);
                let a = d.abs();
                let mut digits = match conv.to_ascii_lowercase() {
                    'e' => jfmt::sci(a, p, upper),
                    'f' => jfmt::fixed(a, p),
                    _ => jfmt::general(a, p, upper),
                };
                if flags.contains(',') && conv.to_ascii_lowercase() != 'e' {
                    let (int, frac) = match digits.split_once('.') {
                        Some((i, f)) => (i.to_string(), format!(".{}", f)),
                        None => (digits.clone(), String::new()),
                    };
                    digits = format!("{}{}", jfmt::group(&int), frac);
                }
                signed(
                    digits,
                    d.is_sign_negative() && d != 0.0 || (d == 0.0 && d.is_sign_negative()),
                    &flags,
                    width,
                )
            }
            _ => unreachable!(),
        };
        let body = if upper { body.to_uppercase() } else { body };
        out.push_str(&pad(&body, width, left, false));
    }
    Ok(out)
}

fn truncate(s: String, prec: Option<usize>) -> String {
    match prec {
        Some(p) => s.chars().take(p).collect(),
        None => s,
    }
}

fn signed(digits: String, neg: bool, flags: &str, width: Option<usize>) -> String {
    let (pre, post) = if neg {
        if flags.contains('(') {
            ("(", ")")
        } else {
            ("-", "")
        }
    } else if flags.contains('+') {
        ("+", "")
    } else if flags.contains(' ') {
        (" ", "")
    } else {
        ("", "")
    };
    let mut d = digits;
    if flags.contains('0') {
        if let Some(w) = width {
            while pre.len() + d.chars().count() + post.len() < w {
                d.insert(0, '0');
            }
        }
    }
    format!("{}{}{}", pre, d, post)
}

fn pad(s: &str, width: Option<usize>, left: bool, _zero: bool) -> String {
    let n = s.chars().count();
    match width {
        Some(w) if w > n => {
            let fill = " ".repeat(w - n);
            if left {
                format!("{}{}", s, fill)
            } else {
                format!("{}{}", fill, s)
            }
        }
        _ => s.to_string(),
    }
}

// ---------------------------------------------------------------- Random (java.util.Random's exact LCG)

const MULT: i64 = 0x5DEECE66D;
const MASK: i64 = (1 << 48) - 1;

fn rnd_next(v: &V, bits: u32) -> i32 {
    let V::R(o) = v else { return 0 };
    let mut d = o.data.borrow_mut();
    let Data::Random { seed, .. } = &mut *d else { return 0 };
    *seed = (seed.wrapping_mul(MULT).wrapping_add(0xB)) & MASK;
    (*seed >> (48 - bits)) as i32
}

fn rnd_next_int(v: &V) -> i32 {
    rnd_next(v, 32)
}

fn rnd_bounded(v: &V, bound: i32) -> i32 {
    let mut r = rnd_next(v, 31);
    let m = bound - 1;
    if bound & m == 0 {
        ((bound as i64 * r as i64) >> 31) as i32
    } else {
        let mut u = r;
        loop {
            r = u % bound;
            if u.wrapping_sub(r).wrapping_add(m) >= 0 {
                break;
            }
            u = rnd_next(v, 31);
        }
        r
    }
}

fn rnd_double(v: &V) -> f64 {
    let hi = rnd_next(v, 26) as i64;
    let lo = rnd_next(v, 27) as i64;
    ((hi << 27) + lo) as f64 * (1.0 / (1u64 << 53) as f64)
}

// ---------------------------------------------------------------- registration

macro_rules! natives {
    ($vm:expr, { $($k:expr => $f:expr),* $(,)? }) => {{
        $( { let f: Nf = $f; $vm.natives.push(($k.to_string(), f)); } )*
    }};
}

pub fn register(vm: &mut Vm) {
    natives!(vm, {
        // ---- Object
        "java/lang/Object.<init>()V" => |_, _| Ok(None),
        "java/lang/Object.equals(Ljava/lang/Object;)Z" => |_, a| ok_z(a[0].same(&a[1])),
        "java/lang/Object.hashCode()I" => |vm, a| { let o = a[0].obj().unwrap().clone(); ok_i(vm.identity_hash(&o)) },
        "java/lang/Object.toString()Ljava/lang/String;" => |vm, a| {
            let h = vm.call_virtual(a[0].clone(), "hashCode", "()I", vec![])?.map(|v| v.i()).unwrap_or(0);
            let n = vm.class_name(a[0].obj().unwrap());
            ret_rs(vm, &format!("{}@{:x}", n, h as u32))
        },

        // ---- String
        "java/lang/String.<init>()V" => |_, a| { set_data(&a[0], Data::Str(Rc::from(vec![]))); Ok(None) },
        "java/lang/String.<init>(Ljava/lang/String;)V" => |vm, a| { let s = str_of(vm, &a[1])?; set_data(&a[0], Data::Str(s)); Ok(None) },
        "java/lang/String.<init>([C)V" => |vm, a| { let c = char_array(vm, &a[1])?; set_data(&a[0], Data::Str(c.into())); Ok(None) },
        "java/lang/String.<init>([CII)V" => |vm, a| {
            let c = char_array(vm, &a[1])?;
            let (off, n) = (a[2].i(), a[3].i());
            if off < 0 || n < 0 || off as i64 + n as i64 > c.len() as i64 {
                return Err(sioobe(vm, format!("offset {}, count {}, length {}", off, n, c.len())));
            }
            set_data(&a[0], Data::Str(c[off as usize..(off + n) as usize].to_vec().into()));
            Ok(None)
        },
        "java/lang/String.length()I" => |vm, a| ok_i(str_of(vm, &a[0])?.len() as i32),
        "java/lang/String.isEmpty()Z" => |vm, a| ok_z(str_of(vm, &a[0])?.is_empty()),
        "java/lang/String.isBlank()Z" => |vm, a| ok_z(str_of(vm, &a[0])?.iter().all(|&c| java_whitespace(c as u32))),
        "java/lang/String.charAt(I)C" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let i = a[1].i();
            if i < 0 || i as usize >= s.len() {
                return Err(sioobe(vm, format!("String index out of range: {}", i)));
            }
            ok_i(s[i as usize] as i32)
        },
        "java/lang/String.substring(I)Ljava/lang/String;" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let b = a[1].i();
            if b < 0 || b as usize > s.len() {
                return Err(sioobe(vm, format!("begin {}, end {}, length {}", b, s.len(), s.len())));
            }
            ret_s(vm, s[b as usize..].to_vec())
        },
        "java/lang/String.substring(II)Ljava/lang/String;" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let (b, e) = (a[1].i(), a[2].i());
            if b < 0 || e as i64 > s.len() as i64 || b > e {
                return Err(sioobe(vm, format!("begin {}, end {}, length {}", b, e, s.len())));
            }
            ret_s(vm, s[b as usize..e as usize].to_vec())
        },
        "java/lang/String.indexOf(Ljava/lang/String;)I" => |vm, a| { let (s, n) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?); ok_i(index_of(&s, &n, 0)) },
        "java/lang/String.indexOf(Ljava/lang/String;I)I" => |vm, a| { let (s, n) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?); ok_i(index_of(&s, &n, a[2].i())) },
        "java/lang/String.indexOf(I)I" => |vm, a| { let s = str_of(vm, &a[0])?; ok_i(index_of(&s, &code_point_units(a[1].i()), 0)) },
        "java/lang/String.indexOf(II)I" => |vm, a| { let s = str_of(vm, &a[0])?; ok_i(index_of(&s, &code_point_units(a[1].i()), a[2].i())) },
        "java/lang/String.lastIndexOf(Ljava/lang/String;)I" => |vm, a| { let (s, n) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?); ok_i(last_index_of(&s, &n)) },
        "java/lang/String.lastIndexOf(I)I" => |vm, a| { let s = str_of(vm, &a[0])?; ok_i(last_index_of(&s, &code_point_units(a[1].i()))) },
        "java/lang/String.contains(Ljava/lang/CharSequence;)Z" => |vm, a| { let (s, n) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?); ok_z(index_of(&s, &n, 0) >= 0) },
        "java/lang/String.startsWith(Ljava/lang/String;)Z" => |vm, a| { let (s, n) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?); ok_z(s.starts_with(&n)) },
        "java/lang/String.endsWith(Ljava/lang/String;)Z" => |vm, a| { let (s, n) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?); ok_z(s.ends_with(&n)) },
        "java/lang/String.equals(Ljava/lang/Object;)Z" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            ok_z(rust_str16(&a[1]).map_or(false, |o| *o == *s))
        },
        "java/lang/String.equalsIgnoreCase(Ljava/lang/String;)Z" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let Some(o) = rust_str16(&a[1]) else { return ok_z(false) };
            ok_z(s.len() == o.len() && s.iter().zip(o.iter()).all(|(&x, &y)| x == y || upper16(x) == upper16(y) || lower16(upper16(x)) == lower16(upper16(y))))
        },
        "java/lang/String.compareTo(Ljava/lang/String;)I" => |vm, a| { let (s, o) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?); ok_i(str_cmp(&s, &o)) },
        "java/lang/String.compareTo(Ljava/lang/Object;)I" => |vm, a| { let (s, o) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?); ok_i(str_cmp(&s, &o)) },
        "java/lang/String.compareToIgnoreCase(Ljava/lang/String;)I" => |vm, a| {
            let (s, o) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?);
            for (&x, &y) in s.iter().zip(o.iter()) {
                if x != y {
                    let (ux, uy) = (upper16(x), upper16(y));
                    if ux != uy {
                        let (lx, ly) = (lower16(ux), lower16(uy));
                        if lx != ly {
                            return ok_i(lx as i32 - ly as i32);
                        }
                    }
                }
            }
            ok_i(s.len() as i32 - o.len() as i32)
        },
        "java/lang/String.hashCode()I" => |vm, a| { let s = str_of(vm, &a[0])?; ok_i(java_hash(&s)) },
        "java/lang/String.toString()Ljava/lang/String;" => |_, a| Ok(Some(a[0].clone())),
        "java/lang/String.intern()Ljava/lang/String;" => |vm, a| { let s = str_of(vm, &a[0])?; Ok(Some(vm.intern_units(s))) },
        "java/lang/String.toUpperCase()Ljava/lang/String;" => |vm, a| { let s = str_of(vm, &a[0])?; let t = String::from_utf16_lossy(&s).to_uppercase(); ret_rs(vm, &t) },
        "java/lang/String.toLowerCase()Ljava/lang/String;" => |vm, a| { let s = str_of(vm, &a[0])?; let t = String::from_utf16_lossy(&s).to_lowercase(); ret_rs(vm, &t) },
        "java/lang/String.trim()Ljava/lang/String;" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let st = s.iter().position(|&c| c > 32).unwrap_or(s.len());
            let en = s.iter().rposition(|&c| c > 32).map_or(st, |e| e + 1);
            ret_s(vm, s[st..en.max(st)].to_vec())
        },
        "java/lang/String.strip()Ljava/lang/String;" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let st = s.iter().position(|&c| !java_whitespace(c as u32)).unwrap_or(s.len());
            let en = s.iter().rposition(|&c| !java_whitespace(c as u32)).map_or(st, |e| e + 1);
            ret_s(vm, s[st..en.max(st)].to_vec())
        },
        "java/lang/String.concat(Ljava/lang/String;)Ljava/lang/String;" => |vm, a| {
            let (s, o) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?);
            let mut v = s.to_vec();
            v.extend_from_slice(&o);
            ret_s(vm, v)
        },
        "java/lang/String.replace(CC)Ljava/lang/String;" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let (from, to) = (a[1].i() as u16, a[2].i() as u16);
            ret_s(vm, s.iter().map(|&c| if c == from { to } else { c }).collect())
        },
        "java/lang/String.replace(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Ljava/lang/String;" => |vm, a| {
            let (s, t, r) = (str_of(vm, &a[0])?, str_of(vm, &a[1])?, str_of(vm, &a[2])?);
            let mut out = Vec::with_capacity(s.len());
            if t.is_empty() {
                for &c in s.iter() {
                    out.extend_from_slice(&r);
                    out.push(c);
                }
                out.extend_from_slice(&r);
            } else {
                let mut i = 0;
                while i < s.len() {
                    if s[i..].starts_with(&t) {
                        out.extend_from_slice(&r);
                        i += t.len();
                    } else {
                        out.push(s[i]);
                        i += 1;
                    }
                }
            }
            ret_s(vm, out)
        },
        "java/lang/String.replaceAll(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let re = regex(vm, &a[1])?;
            let rep = str_of(vm, &a[2])?;
            match re.replace_all(&s, &rep) {
                Ok(v) => ret_s(vm, v),
                Err(e) => Err(vm.throw("java/lang/IllegalArgumentException", Some(&e))),
            }
        },
        "java/lang/String.matches(Ljava/lang/String;)Z" => |vm, a| { let s = str_of(vm, &a[0])?; let re = regex(vm, &a[1])?; ok_z(re.matches(&s)) },
        "java/lang/String.split(Ljava/lang/String;)[Ljava/lang/String;" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let re = regex(vm, &a[1])?;
            let parts = re.split(&s);
            let items: Vec<V> = parts.into_iter().map(|p| vm.str_from(p.into())).collect();
            let arr = vm.new_array("[Ljava/lang/String;", 0)?;
            set_data(&arr, Data::ArrA(items));
            Ok(Some(arr))
        },
        "java/lang/String.toCharArray()[C" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let arr = vm.new_array("[C", 0)?;
            set_data(&arr, Data::ArrC(s.to_vec()));
            Ok(Some(arr))
        },
        "java/lang/String.repeat(I)Ljava/lang/String;" => |vm, a| {
            let s = str_of(vm, &a[0])?;
            let n = a[1].i();
            if n < 0 {
                return Err(vm.throw("java/lang/IllegalArgumentException", Some(&format!("count is negative: {}", n))));
            }
            ret_s(vm, s.repeat(n as usize))
        },
        "java/lang/String.valueOf(I)Ljava/lang/String;" => |vm, a| ret_rs(vm, &a[0].i().to_string()),
        "java/lang/String.valueOf(J)Ljava/lang/String;" => |vm, a| ret_rs(vm, &a[0].l().to_string()),
        "java/lang/String.valueOf(D)Ljava/lang/String;" => |vm, a| ret_rs(vm, &jfmt::double_to_string(a[0].d())),
        "java/lang/String.valueOf(Z)Ljava/lang/String;" => |vm, a| ret_rs(vm, bool_str(a[0].i() != 0)),
        "java/lang/String.valueOf(C)Ljava/lang/String;" => |vm, a| ret_s(vm, vec![a[0].i() as u16]),
        "java/lang/String.valueOf([C)Ljava/lang/String;" => |vm, a| { let c = char_array(vm, &a[0])?; ret_s(vm, c) },
        "java/lang/String.valueOf(Ljava/lang/Object;)Ljava/lang/String;" => |vm, a| { let s = to_jstring(vm, &a[0])?; Ok(Some(vm.str_from(s))) },
        "java/lang/String.format(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;" => |vm, a| {
            let f = str_of(vm, &a[0])?;
            let args = match &a[1] { V::R(o) => match &*o.data.borrow() { Data::ArrA(v) => v.clone(), _ => vec![] }, _ => vec![] };
            let s = java_format(vm, &f, &args)?;
            ret_rs(vm, &s)
        },
        "java/lang/String.join(Ljava/lang/CharSequence;[Ljava/lang/CharSequence;)Ljava/lang/String;" => |vm, a| {
            let sep = str_of(vm, &a[0])?;
            let items = match &a[1] { V::R(o) => match &*o.data.borrow() { Data::ArrA(v) => v.clone(), _ => vec![] }, _ => return Err(npe(vm)) };
            let mut out = Vec::new();
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(&sep);
                }
                match it {
                    V::Null => out.extend(u16s("null")),
                    _ => out.extend_from_slice(&str_of(vm, it)?),
                }
            }
            ret_s(vm, out)
        },

        // ---- StringBuilder
        "java/lang/StringBuilder.<init>()V" => |_, a| { set_data(&a[0], Data::Sb(Vec::new())); Ok(None) },
        "java/lang/StringBuilder.<init>(I)V" => |vm, a| {
            if a[1].i() < 0 {
                return Err(vm.throw("java/lang/NegativeArraySizeException", Some(&a[1].i().to_string())));
            }
            set_data(&a[0], Data::Sb(Vec::with_capacity(a[1].i().min(1 << 20) as usize)));
            Ok(None)
        },
        "java/lang/StringBuilder.<init>(Ljava/lang/String;)V" => |vm, a| { let s = str_of(vm, &a[1])?; set_data(&a[0], Data::Sb(s.to_vec())); Ok(None) },
        "java/lang/StringBuilder.append(I)Ljava/lang/StringBuilder;" => |_, a| { sb(&a[0], |s| s.extend(u16s(&a[1].i().to_string()))); Ok(Some(a[0].clone())) },
        "java/lang/StringBuilder.append(J)Ljava/lang/StringBuilder;" => |_, a| { sb(&a[0], |s| s.extend(u16s(&a[1].l().to_string()))); Ok(Some(a[0].clone())) },
        "java/lang/StringBuilder.append(D)Ljava/lang/StringBuilder;" => |_, a| { sb(&a[0], |s| s.extend(u16s(&jfmt::double_to_string(a[1].d())))); Ok(Some(a[0].clone())) },
        "java/lang/StringBuilder.append(Z)Ljava/lang/StringBuilder;" => |_, a| { sb(&a[0], |s| s.extend(u16s(bool_str(a[1].i() != 0)))); Ok(Some(a[0].clone())) },
        "java/lang/StringBuilder.append(C)Ljava/lang/StringBuilder;" => |_, a| { sb(&a[0], |s| s.push(a[1].i() as u16)); Ok(Some(a[0].clone())) },
        "java/lang/StringBuilder.append([C)Ljava/lang/StringBuilder;" => |vm, a| { let c = char_array(vm, &a[1])?; sb(&a[0], |s| s.extend(c)); Ok(Some(a[0].clone())) },
        "java/lang/StringBuilder.append(Ljava/lang/String;)Ljava/lang/StringBuilder;" => |vm, a| { let t = to_jstring(vm, &a[1])?; sb(&a[0], |s| s.extend_from_slice(&t)); Ok(Some(a[0].clone())) },
        "java/lang/StringBuilder.append(Ljava/lang/CharSequence;)Ljava/lang/StringBuilder;" => |vm, a| {
            let t: Rc<[u16]> = match &a[1] { V::Null => u16s("null").into(), v => str_of(vm, v)? };
            sb(&a[0], |s| s.extend_from_slice(&t));
            Ok(Some(a[0].clone()))
        },
        "java/lang/StringBuilder.append(Ljava/lang/Object;)Ljava/lang/StringBuilder;" => |vm, a| { let t = to_jstring(vm, &a[1])?; sb(&a[0], |s| s.extend_from_slice(&t)); Ok(Some(a[0].clone())) },
        "java/lang/StringBuilder.insert(ILjava/lang/String;)Ljava/lang/StringBuilder;" => |vm, a| {
            let t = to_jstring(vm, &a[2])?;
            let off = a[1].i();
            let len = sb(&a[0], |s| s.len());
            if off < 0 || off as usize > len {
                return Err(sioobe(vm, format!("offset {}, length {}", off, len)));
            }
            sb(&a[0], |s| { s.splice(off as usize..off as usize, t.iter().copied()); });
            Ok(Some(a[0].clone()))
        },
        "java/lang/StringBuilder.insert(IC)Ljava/lang/StringBuilder;" => |vm, a| {
            let off = a[1].i();
            let len = sb(&a[0], |s| s.len());
            if off < 0 || off as usize > len {
                return Err(sioobe(vm, format!("offset {}, length {}", off, len)));
            }
            sb(&a[0], |s| s.insert(off as usize, a[2].i() as u16));
            Ok(Some(a[0].clone()))
        },
        "java/lang/StringBuilder.insert(II)Ljava/lang/StringBuilder;" => |vm, a| {
            let t = u16s(&a[2].i().to_string());
            let off = a[1].i();
            let len = sb(&a[0], |s| s.len());
            if off < 0 || off as usize > len {
                return Err(sioobe(vm, format!("offset {}, length {}", off, len)));
            }
            sb(&a[0], |s| { s.splice(off as usize..off as usize, t.iter().copied()); });
            Ok(Some(a[0].clone()))
        },
        "java/lang/StringBuilder.reverse()Ljava/lang/StringBuilder;" => |_, a| {
            sb(&a[0], |s| {
                s.reverse();
                let mut i = 0;
                while i + 1 < s.len() {
                    if (0xDC00..0xE000).contains(&s[i]) && (0xD800..0xDC00).contains(&s[i + 1]) {
                        s.swap(i, i + 1);
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            });
            Ok(Some(a[0].clone()))
        },
        "java/lang/StringBuilder.length()I" => |_, a| ok_i(sb(&a[0], |s| s.len()) as i32),
        "java/lang/StringBuilder.isEmpty()Z" => |_, a| ok_z(sb(&a[0], |s| s.is_empty())),
        "java/lang/StringBuilder.charAt(I)C" => |vm, a| {
            let i = a[1].i();
            let len = sb(&a[0], |s| s.len());
            if i < 0 || i as usize >= len {
                return Err(sioobe(vm, format!("index {},length {}", i, len)));
            }
            ok_i(sb(&a[0], |s| s[i as usize]) as i32)
        },
        "java/lang/StringBuilder.setCharAt(IC)V" => |vm, a| {
            let i = a[1].i();
            let len = sb(&a[0], |s| s.len());
            if i < 0 || i as usize >= len {
                return Err(sioobe(vm, format!("index {},length {}", i, len)));
            }
            sb(&a[0], |s| s[i as usize] = a[2].i() as u16);
            Ok(None)
        },
        "java/lang/StringBuilder.deleteCharAt(I)Ljava/lang/StringBuilder;" => |vm, a| {
            let i = a[1].i();
            let len = sb(&a[0], |s| s.len());
            if i < 0 || i as usize >= len {
                return Err(sioobe(vm, format!("index {},length {}", i, len)));
            }
            sb(&a[0], |s| { s.remove(i as usize); });
            Ok(Some(a[0].clone()))
        },
        "java/lang/StringBuilder.delete(II)Ljava/lang/StringBuilder;" => |vm, a| {
            let len = sb(&a[0], |s| s.len()) as i32;
            let (st, mut en) = (a[1].i(), a[2].i());
            if en > len {
                en = len;
            }
            if st < 0 || st > en {
                return Err(sioobe(vm, format!("start {}, end {}, length {}", st, a[2].i(), len)));
            }
            sb(&a[0], |s| { s.drain(st as usize..en as usize); });
            Ok(Some(a[0].clone()))
        },
        "java/lang/StringBuilder.setLength(I)V" => |vm, a| {
            let n = a[1].i();
            if n < 0 {
                return Err(sioobe(vm, format!("String index out of range: {}", n)));
            }
            sb(&a[0], |s| s.resize(n as usize, 0));
            Ok(None)
        },
        "java/lang/StringBuilder.indexOf(Ljava/lang/String;)I" => |vm, a| { let n = str_of(vm, &a[1])?; let s = sb(&a[0], |s| s.clone()); ok_i(index_of(&s, &n, 0)) },
        "java/lang/StringBuilder.replace(IILjava/lang/String;)Ljava/lang/StringBuilder;" => |vm, a| {
            let t = str_of(vm, &a[3])?;
            let len = sb(&a[0], |s| s.len()) as i32;
            let (st, mut en) = (a[1].i(), a[2].i());
            if en > len {
                en = len;
            }
            if st < 0 || st > len || st > en {
                return Err(sioobe(vm, format!("start {}, end {}, length {}", st, a[2].i(), len)));
            }
            sb(&a[0], |s| { s.splice(st as usize..en as usize, t.iter().copied()); });
            Ok(Some(a[0].clone()))
        },
        "java/lang/StringBuilder.substring(I)Ljava/lang/String;" => |vm, a| {
            let s = sb(&a[0], |s| s.clone());
            let b = a[1].i();
            if b < 0 || b as usize > s.len() {
                return Err(sioobe(vm, format!("start {}, end {}, length {}", b, s.len(), s.len())));
            }
            ret_s(vm, s[b as usize..].to_vec())
        },
        "java/lang/StringBuilder.substring(II)Ljava/lang/String;" => |vm, a| {
            let s = sb(&a[0], |s| s.clone());
            let (b, e) = (a[1].i(), a[2].i());
            if b < 0 || e as i64 > s.len() as i64 || b > e {
                return Err(sioobe(vm, format!("start {}, end {}, length {}", b, e, s.len())));
            }
            ret_s(vm, s[b as usize..e as usize].to_vec())
        },
        "java/lang/StringBuilder.toString()Ljava/lang/String;" => |vm, a| { let s = sb(&a[0], |s| s.clone()); ret_s(vm, s) },

        // ---- Math
        "java/lang/Math.abs(I)I" => |_, a| ok_i(a[0].i().wrapping_abs()),
        "java/lang/Math.abs(J)J" => |_, a| ok_l(a[0].l().wrapping_abs()),
        "java/lang/Math.abs(D)D" => |_, a| ok_d(a[0].d().abs()),
        "java/lang/Math.max(II)I" => |_, a| ok_i(a[0].i().max(a[1].i())),
        "java/lang/Math.max(JJ)J" => |_, a| ok_l(a[0].l().max(a[1].l())),
        "java/lang/Math.max(DD)D" => |_, a| ok_d(jmax(a[0].d(), a[1].d())),
        "java/lang/Math.min(II)I" => |_, a| ok_i(a[0].i().min(a[1].i())),
        "java/lang/Math.min(JJ)J" => |_, a| ok_l(a[0].l().min(a[1].l())),
        "java/lang/Math.min(DD)D" => |_, a| ok_d(jmin(a[0].d(), a[1].d())),
        "java/lang/Math.sqrt(D)D" => |_, a| ok_d(a[0].d().sqrt()),
        "java/lang/Math.cbrt(D)D" => |_, a| ok_d(a[0].d().cbrt()),
        "java/lang/Math.pow(DD)D" => |_, a| ok_d(jpow(a[0].d(), a[1].d())),
        "java/lang/Math.exp(D)D" => |_, a| ok_d(a[0].d().exp()),
        "java/lang/Math.log(D)D" => |_, a| ok_d(a[0].d().ln()),
        "java/lang/Math.log10(D)D" => |_, a| ok_d(a[0].d().log10()),
        "java/lang/Math.floor(D)D" => |_, a| ok_d(a[0].d().floor()),
        "java/lang/Math.ceil(D)D" => |_, a| ok_d(a[0].d().ceil()),
        "java/lang/Math.rint(D)D" => |_, a| ok_d(a[0].d().round_ties_even()),
        "java/lang/Math.round(D)J" => |_, a| {
            let x = a[0].d();
            if x.is_nan() {
                return ok_l(0);
            }
            let f = x.floor();
            ok_l(if x - f >= 0.5 { f + 1.0 } else { f } as i64)
        },
        "java/lang/Math.random()D" => |vm, _| ok_d(vm.host.random()),
        "java/lang/Math.sin(D)D" => |_, a| ok_d(a[0].d().sin()),
        "java/lang/Math.cos(D)D" => |_, a| ok_d(a[0].d().cos()),
        "java/lang/Math.tan(D)D" => |_, a| ok_d(a[0].d().tan()),
        "java/lang/Math.asin(D)D" => |_, a| ok_d(a[0].d().asin()),
        "java/lang/Math.acos(D)D" => |_, a| ok_d(a[0].d().acos()),
        "java/lang/Math.atan(D)D" => |_, a| ok_d(a[0].d().atan()),
        "java/lang/Math.atan2(DD)D" => |_, a| ok_d(a[0].d().atan2(a[1].d())),
        "java/lang/Math.hypot(DD)D" => |_, a| ok_d(a[0].d().hypot(a[1].d())),
        "java/lang/Math.signum(D)D" => |_, a| { let x = a[0].d(); ok_d(if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { x }) },
        "java/lang/Math.toRadians(D)D" => |_, a| ok_d(a[0].d() * 0.017453292519943295),
        "java/lang/Math.toDegrees(D)D" => |_, a| ok_d(a[0].d() * 57.29577951308232),
        "java/lang/Math.floorDiv(II)I" => |vm, a| { let (x, y) = (a[0].i(), a[1].i()); if y == 0 { return Err(vm.throw("java/lang/ArithmeticException", Some("/ by zero"))); } let q = x.wrapping_div(y); ok_i(if (x % y != 0) && ((x ^ y) < 0) { q - 1 } else { q }) },
        "java/lang/Math.floorDiv(JJ)J" => |vm, a| { let (x, y) = (a[0].l(), a[1].l()); if y == 0 { return Err(vm.throw("java/lang/ArithmeticException", Some("/ by zero"))); } let q = x.wrapping_div(y); ok_l(if (x % y != 0) && ((x ^ y) < 0) { q - 1 } else { q }) },
        "java/lang/Math.floorMod(II)I" => |vm, a| { let (x, y) = (a[0].i(), a[1].i()); if y == 0 { return Err(vm.throw("java/lang/ArithmeticException", Some("/ by zero"))); } let m = x.wrapping_rem(y); ok_i(if m != 0 && ((m ^ y) < 0) { m + y } else { m }) },
        "java/lang/Math.floorMod(JJ)J" => |vm, a| { let (x, y) = (a[0].l(), a[1].l()); if y == 0 { return Err(vm.throw("java/lang/ArithmeticException", Some("/ by zero"))); } let m = x.wrapping_rem(y); ok_l(if m != 0 && ((m ^ y) < 0) { m + y } else { m }) },
        "java/lang/Math.addExact(II)I" => |vm, a| a[0].i().checked_add(a[1].i()).map(|v| Some(V::I(v))).ok_or_else(|| vm.throw("java/lang/ArithmeticException", Some("integer overflow"))),
        "java/lang/Math.addExact(JJ)J" => |vm, a| a[0].l().checked_add(a[1].l()).map(|v| Some(V::L(v))).ok_or_else(|| vm.throw("java/lang/ArithmeticException", Some("long overflow"))),
        "java/lang/Math.subtractExact(II)I" => |vm, a| a[0].i().checked_sub(a[1].i()).map(|v| Some(V::I(v))).ok_or_else(|| vm.throw("java/lang/ArithmeticException", Some("integer overflow"))),
        "java/lang/Math.subtractExact(JJ)J" => |vm, a| a[0].l().checked_sub(a[1].l()).map(|v| Some(V::L(v))).ok_or_else(|| vm.throw("java/lang/ArithmeticException", Some("long overflow"))),
        "java/lang/Math.multiplyExact(II)I" => |vm, a| a[0].i().checked_mul(a[1].i()).map(|v| Some(V::I(v))).ok_or_else(|| vm.throw("java/lang/ArithmeticException", Some("integer overflow"))),
        "java/lang/Math.multiplyExact(JJ)J" => |vm, a| a[0].l().checked_mul(a[1].l()).map(|v| Some(V::L(v))).ok_or_else(|| vm.throw("java/lang/ArithmeticException", Some("long overflow"))),
        "java/lang/Math.toIntExact(J)I" => |vm, a| i32::try_from(a[0].l()).map(|v| Some(V::I(v))).map_err(|_| vm.throw("java/lang/ArithmeticException", Some("integer overflow"))),

        // ---- Integer / Long / Double / Character / Boolean statics
        "java/lang/Integer.parseInt(Ljava/lang/String;)I" => |vm, a| {
            let v = parse_int(vm, &a[0], 10)?;
            if v < i32::MIN as i64 || v > i32::MAX as i64 {
                let s = rust_string(&a[0]).unwrap_or_default();
                return Err(vm.throw("java/lang/NumberFormatException", Some(&format!("For input string: \"{}\"", s))));
            }
            ok_i(v as i32)
        },
        "java/lang/Integer.parseInt(Ljava/lang/String;I)I" => |vm, a| {
            let r = a[1].i() as u32;
            let v = parse_int(vm, &a[0], r)?;
            if v < i32::MIN as i64 || v > i32::MAX as i64 {
                let s = rust_string(&a[0]).unwrap_or_default();
                return Err(vm.throw("java/lang/NumberFormatException", Some(&format!("For input string: \"{}\" under radix {}", s, r))));
            }
            ok_i(v as i32)
        },
        "java/lang/Integer.valueOf(I)Ljava/lang/Integer;" => |vm, a| Ok(Some(box_value(vm, b'I', V::I(a[0].i())))),
        "java/lang/Integer.toString(I)Ljava/lang/String;" => |vm, a| ret_rs(vm, &a[0].i().to_string()),
        "java/lang/Integer.toString(II)Ljava/lang/String;" => |vm, a| { let r = a[1].i(); let r = if (2..=36).contains(&r) { r } else { 10 }; ret_rs(vm, &jfmt::signed_radix(a[0].i() as i64, r as u32)) },
        "java/lang/Integer.toBinaryString(I)Ljava/lang/String;" => |vm, a| ret_rs(vm, &jfmt::radix(a[0].i() as u32 as u64, 2)),
        "java/lang/Integer.toHexString(I)Ljava/lang/String;" => |vm, a| ret_rs(vm, &jfmt::radix(a[0].i() as u32 as u64, 16)),
        "java/lang/Integer.toOctalString(I)Ljava/lang/String;" => |vm, a| ret_rs(vm, &jfmt::radix(a[0].i() as u32 as u64, 8)),
        "java/lang/Integer.bitCount(I)I" => |_, a| ok_i(a[0].i().count_ones() as i32),
        "java/lang/Integer.compare(II)I" => |_, a| ok_i(a[0].i().cmp(&a[1].i()) as i32),
        "java/lang/Integer.signum(I)I" => |_, a| ok_i(a[0].i().signum()),
        "java/lang/Integer.reverse(I)I" => |_, a| ok_i(a[0].i().reverse_bits()),
        "java/lang/Integer.highestOneBit(I)I" => |_, a| { let v = a[0].i() as u32; ok_i(if v == 0 { 0 } else { (1u32 << (31 - v.leading_zeros())) as i32 }) },
        "java/lang/Integer.lowestOneBit(I)I" => |_, a| { let v = a[0].i(); ok_i(v & v.wrapping_neg()) },
        "java/lang/Integer.numberOfLeadingZeros(I)I" => |_, a| ok_i(a[0].i().leading_zeros() as i32),
        "java/lang/Integer.numberOfTrailingZeros(I)I" => |_, a| ok_i(a[0].i().trailing_zeros() as i32),
        "java/lang/Integer.max(II)I" => |_, a| ok_i(a[0].i().max(a[1].i())),
        "java/lang/Integer.min(II)I" => |_, a| ok_i(a[0].i().min(a[1].i())),
        "java/lang/Integer.sum(II)I" => |_, a| ok_i(a[0].i().wrapping_add(a[1].i())),
        "java/lang/Long.parseLong(Ljava/lang/String;)J" => |vm, a| ok_l(parse_int(vm, &a[0], 10)?),
        "java/lang/Long.valueOf(J)Ljava/lang/Long;" => |vm, a| Ok(Some(box_value(vm, b'J', V::L(a[0].l())))),
        "java/lang/Long.toString(J)Ljava/lang/String;" => |vm, a| ret_rs(vm, &a[0].l().to_string()),
        "java/lang/Long.toBinaryString(J)Ljava/lang/String;" => |vm, a| ret_rs(vm, &jfmt::radix(a[0].l() as u64, 2)),
        "java/lang/Long.toHexString(J)Ljava/lang/String;" => |vm, a| ret_rs(vm, &jfmt::radix(a[0].l() as u64, 16)),
        "java/lang/Long.bitCount(J)I" => |_, a| ok_i(a[0].l().count_ones() as i32),
        "java/lang/Long.compare(JJ)I" => |_, a| ok_i(a[0].l().cmp(&a[1].l()) as i32),
        "java/lang/Long.signum(J)I" => |_, a| ok_i(a[0].l().signum() as i32),
        "java/lang/Long.numberOfLeadingZeros(J)I" => |_, a| ok_i(a[0].l().leading_zeros() as i32),
        "java/lang/Long.numberOfTrailingZeros(J)I" => |_, a| ok_i(a[0].l().trailing_zeros() as i32),
        "java/lang/Long.max(JJ)J" => |_, a| ok_l(a[0].l().max(a[1].l())),
        "java/lang/Long.min(JJ)J" => |_, a| ok_l(a[0].l().min(a[1].l())),
        "java/lang/Long.sum(JJ)J" => |_, a| ok_l(a[0].l().wrapping_add(a[1].l())),
        "java/lang/Double.parseDouble(Ljava/lang/String;)D" => |vm, a| ok_d(parse_double(vm, &a[0])?),
        "java/lang/Double.valueOf(D)Ljava/lang/Double;" => |vm, a| Ok(Some(box_value(vm, b'D', V::D(a[0].d())))),
        "java/lang/Double.toString(D)Ljava/lang/String;" => |vm, a| ret_rs(vm, &jfmt::double_to_string(a[0].d())),
        "java/lang/Double.isNaN(D)Z" => |_, a| ok_z(a[0].d().is_nan()),
        "java/lang/Double.isInfinite(D)Z" => |_, a| ok_z(a[0].d().is_infinite()),
        "java/lang/Double.isFinite(D)Z" => |_, a| ok_z(a[0].d().is_finite()),
        "java/lang/Double.compare(DD)I" => |_, a| ok_i(java_double_cmp(a[0].d(), a[1].d()) as i32),
        "java/lang/Double.max(DD)D" => |_, a| ok_d(jmax(a[0].d(), a[1].d())),
        "java/lang/Double.min(DD)D" => |_, a| ok_d(jmin(a[0].d(), a[1].d())),
        "java/lang/Double.sum(DD)D" => |_, a| ok_d(a[0].d() + a[1].d()),
        "java/lang/Character.valueOf(C)Ljava/lang/Character;" => |vm, a| Ok(Some(box_value(vm, b'C', V::I(a[0].i())))),
        "java/lang/Character.isDigit(C)Z" => |_, a| ok_z(ch_of(a[0].i() as u32).map_or(false, |c| c.is_ascii_digit() || (c as u32 > 127 && c.is_numeric()))),
        "java/lang/Character.isLetter(C)Z" => |_, a| ok_z(ch_of(a[0].i() as u32).map_or(false, |c| c.is_alphabetic())),
        "java/lang/Character.isLetterOrDigit(C)Z" => |_, a| ok_z(ch_of(a[0].i() as u32).map_or(false, |c| c.is_alphanumeric())),
        "java/lang/Character.isAlphabetic(I)Z" => |_, a| ok_z(ch_of(a[0].i() as u32).map_or(false, |c| c.is_alphabetic())),
        "java/lang/Character.isWhitespace(C)Z" => |_, a| ok_z(java_whitespace(a[0].i() as u32)),
        "java/lang/Character.isUpperCase(C)Z" => |_, a| ok_z(ch_of(a[0].i() as u32).map_or(false, |c| c.is_uppercase())),
        "java/lang/Character.isLowerCase(C)Z" => |_, a| ok_z(ch_of(a[0].i() as u32).map_or(false, |c| c.is_lowercase())),
        "java/lang/Character.toUpperCase(C)C" => |_, a| ok_i(upper16(a[0].i() as u16) as i32),
        "java/lang/Character.toLowerCase(C)C" => |_, a| ok_i(lower16(a[0].i() as u16) as i32),
        "java/lang/Character.getNumericValue(C)I" => |_, a| {
            let c = a[0].i() as u32;
            ok_i(match ch_of(c) {
                Some(ch) if ch.is_ascii_digit() => ch as i32 - '0' as i32,
                Some(ch) if ch.is_ascii_alphabetic() => ch.to_ascii_lowercase() as i32 - 'a' as i32 + 10,
                Some(ch) => ch.to_digit(10).map_or(-1, |d| d as i32),
                None => -1,
            })
        },
        "java/lang/Character.digit(CI)I" => |_, a| ok_i(ch_of(a[0].i() as u32).and_then(|c| c.to_digit(a[1].i().clamp(2, 36) as u32)).map_or(-1, |d| d as i32)),
        "java/lang/Character.forDigit(II)C" => |_, a| { let (d, r) = (a[0].i(), a[1].i()); ok_i(if (2..=36).contains(&r) && d >= 0 && d < r { std::char::from_digit(d as u32, r as u32).unwrap() as i32 } else { 0 }) },
        "java/lang/Character.toString(C)Ljava/lang/String;" => |vm, a| ret_s(vm, vec![a[0].i() as u16]),
        "java/lang/Character.compare(CC)I" => |_, a| ok_i(a[0].i() - a[1].i()),
        "java/lang/Boolean.valueOf(Z)Ljava/lang/Boolean;" => |vm, a| Ok(Some(box_value(vm, b'Z', V::I((a[0].i() != 0) as i32)))),
        "java/lang/Boolean.parseBoolean(Ljava/lang/String;)Z" => |_, a| ok_z(rust_string(&a[0]).map_or(false, |s| s.eq_ignore_ascii_case("true"))),
        "java/lang/Boolean.toString(Z)Ljava/lang/String;" => |vm, a| ret_rs(vm, bool_str(a[0].i() != 0)),
        "java/lang/Boolean.compare(ZZ)I" => |_, a| ok_i(a[0].i() - a[1].i()),
        "java/lang/Number.intValue()I" => |_, a| ok_i(match unbox(&a[0]) { Some(V::I(x)) => x, Some(V::L(x)) => x as i32, Some(V::D(x)) => x as i32, _ => 0 }),
        "java/lang/Number.longValue()J" => |_, a| ok_l(match unbox(&a[0]) { Some(V::I(x)) => x as i64, Some(V::L(x)) => x, Some(V::D(x)) => x as i64, _ => 0 }),
        "java/lang/Number.doubleValue()D" => |_, a| ok_d(match unbox(&a[0]) { Some(V::I(x)) => x as f64, Some(V::L(x)) => x as f64, Some(V::D(x)) => x, _ => 0.0 }),
        "java/lang/Character.charValue()C" => |_, a| ok_i(unbox(&a[0]).map(|v| v.i()).unwrap_or(0)),
        "java/lang/Boolean.booleanValue()Z" => |_, a| ok_i(unbox(&a[0]).map(|v| v.i()).unwrap_or(0)),

        // ---- System / Thread / streams
        "java/lang/System.currentTimeMillis()J" => |vm, _| ok_l(vm.host.epoch_ms() as i64),
        "java/lang/System.nanoTime()J" => |vm, _| ok_l((vm.host.now_ms() * 1e6) as i64),
        "java/lang/System.exit(I)V" => |vm, a| { vm.flush(); Err(Ex::Exit(a[0].i())) },
        "java/lang/System.arraycopy(Ljava/lang/Object;ILjava/lang/Object;II)V" => |vm, a| arraycopy(vm, &a),
        "java/lang/System.getProperty(Ljava/lang/String;)Ljava/lang/String;" => |vm, a| {
            let k = rust_string(&a[0]).unwrap_or_default();
            match k.as_str() {
                "line.separator" => ret_rs(vm, "\n"),
                "java.version" => ret_rs(vm, "17"),
                "os.name" => ret_rs(vm, "jolt-vm"),
                "file.separator" => ret_rs(vm, "/"),
                _ => Ok(Some(V::Null)),
            }
        },
        "java/lang/System.getenv(Ljava/lang/String;)Ljava/lang/String;" => |_, _| Ok(Some(V::Null)),
        "java/lang/System.lineSeparator()Ljava/lang/String;" => |vm, _| ret_rs(vm, "\n"),
        "java/lang/System.identityHashCode(Ljava/lang/Object;)I" => |vm, a| match &a[0] { V::R(o) => { let o = o.clone(); ok_i(vm.identity_hash(&o)) } _ => ok_i(0) },
        "java/lang/System.gc()V" => |_, _| Ok(None),
        "java/lang/Thread.sleep(J)V" => |vm, a| {
            let ms = a[0].l();
            if ms < 0 {
                return Err(vm.throw("java/lang/IllegalArgumentException", Some("timeout value is negative")));
            }
            vm.flush();
            vm.host.sleep_ms(ms as f64);
            Ok(None)
        },
        "java/io/InputStream.read()I" => |vm, _| {
            let stdin = vm.stdin();
            let byte = scan(vm, &stdin, |s, eof| {
                if s.pos < s.buf.len() {
                    Ok(Some(s.buf[s.pos]))
                } else if eof {
                    Ok(None)
                } else {
                    Err(NeedMore)
                }
            })?;
            match byte {
                Some(b) => {
                    stdin.borrow_mut().pos += 1;
                    ok_i((b & 0xff) as i32)
                }
                None => ok_i(-1),
            }
        },
        "java/io/PrintStream.println()V" => |vm, a| { print_to(vm, &a[0], "\n")?; Ok(None) },
        "java/io/PrintStream.println(Z)V" => |vm, a| { print_to(vm, &a[0], &format!("{}\n", bool_str(a[1].i() != 0)))?; Ok(None) },
        "java/io/PrintStream.println(C)V" => |vm, a| { let s = String::from_utf16_lossy(&[a[1].i() as u16]); print_to(vm, &a[0], &format!("{}\n", s))?; Ok(None) },
        "java/io/PrintStream.println(I)V" => |vm, a| { print_to(vm, &a[0], &format!("{}\n", a[1].i()))?; Ok(None) },
        "java/io/PrintStream.println(J)V" => |vm, a| { print_to(vm, &a[0], &format!("{}\n", a[1].l()))?; Ok(None) },
        "java/io/PrintStream.println(D)V" => |vm, a| { print_to(vm, &a[0], &format!("{}\n", jfmt::double_to_string(a[1].d())))?; Ok(None) },
        "java/io/PrintStream.println([C)V" => |vm, a| { let c = char_array(vm, &a[1])?; print_to(vm, &a[0], &format!("{}\n", String::from_utf16_lossy(&c)))?; Ok(None) },
        "java/io/PrintStream.println(Ljava/lang/String;)V" => |vm, a| { let s = to_rstring(vm, &a[1])?; print_to(vm, &a[0], &format!("{}\n", s))?; Ok(None) },
        "java/io/PrintStream.println(Ljava/lang/Object;)V" => |vm, a| { let s = to_rstring(vm, &a[1])?; print_to(vm, &a[0], &format!("{}\n", s))?; Ok(None) },
        "java/io/PrintStream.print(Z)V" => |vm, a| { print_to(vm, &a[0], bool_str(a[1].i() != 0))?; Ok(None) },
        "java/io/PrintStream.print(C)V" => |vm, a| { let s = String::from_utf16_lossy(&[a[1].i() as u16]); print_to(vm, &a[0], &s)?; Ok(None) },
        "java/io/PrintStream.print(I)V" => |vm, a| { print_to(vm, &a[0], &a[1].i().to_string())?; Ok(None) },
        "java/io/PrintStream.print(J)V" => |vm, a| { print_to(vm, &a[0], &a[1].l().to_string())?; Ok(None) },
        "java/io/PrintStream.print(D)V" => |vm, a| { print_to(vm, &a[0], &jfmt::double_to_string(a[1].d()))?; Ok(None) },
        "java/io/PrintStream.print([C)V" => |vm, a| { let c = char_array(vm, &a[1])?; print_to(vm, &a[0], &String::from_utf16_lossy(&c))?; Ok(None) },
        "java/io/PrintStream.print(Ljava/lang/String;)V" => |vm, a| { let s = to_rstring(vm, &a[1])?; print_to(vm, &a[0], &s)?; Ok(None) },
        "java/io/PrintStream.print(Ljava/lang/Object;)V" => |vm, a| { let s = to_rstring(vm, &a[1])?; print_to(vm, &a[0], &s)?; Ok(None) },
        "java/io/PrintStream.printf(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintStream;" => |vm, a| printf(vm, a),
        "java/io/PrintStream.format(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintStream;" => |vm, a| printf(vm, a),
        "java/io/PrintStream.flush()V" => |vm, _| { vm.flush(); Ok(None) },
        "java/io/PrintStream.write(I)V" => |vm, a| { let s = String::from_utf8_lossy(&[a[1].i() as u8]).into_owned(); print_to(vm, &a[0], &s)?; Ok(None) },

        // ---- Arrays
        "java/util/Arrays.toString([I)Ljava/lang/String;" => |vm, a| { let s = arr_to_string(vm, &a[0])?; ret_rs(vm, &s) },
        "java/util/Arrays.toString([J)Ljava/lang/String;" => |vm, a| { let s = arr_to_string(vm, &a[0])?; ret_rs(vm, &s) },
        "java/util/Arrays.toString([D)Ljava/lang/String;" => |vm, a| { let s = arr_to_string(vm, &a[0])?; ret_rs(vm, &s) },
        "java/util/Arrays.toString([C)Ljava/lang/String;" => |vm, a| { let s = arr_to_string(vm, &a[0])?; ret_rs(vm, &s) },
        "java/util/Arrays.toString([Z)Ljava/lang/String;" => |vm, a| { let s = arr_to_string(vm, &a[0])?; ret_rs(vm, &s) },
        "java/util/Arrays.toString([Ljava/lang/Object;)Ljava/lang/String;" => |vm, a| { let s = arr_to_string(vm, &a[0])?; ret_rs(vm, &s) },
        "java/util/Arrays.deepToString([Ljava/lang/Object;)Ljava/lang/String;" => |vm, a| { let s = deep_to_string(vm, &a[0], &mut vec![])?; ret_rs(vm, &s) },
        "java/util/Arrays.sort([I)V" => |vm, a| sort_range(vm, &a[0], None),
        "java/util/Arrays.sort([J)V" => |vm, a| sort_range(vm, &a[0], None),
        "java/util/Arrays.sort([D)V" => |vm, a| sort_range(vm, &a[0], None),
        "java/util/Arrays.sort([C)V" => |vm, a| sort_range(vm, &a[0], None),
        "java/util/Arrays.sort([III)V" => |vm, a| sort_range(vm, &a[0], Some((a[1].i(), a[2].i()))),
        "java/util/Arrays.sort([JII)V" => |vm, a| sort_range(vm, &a[0], Some((a[1].i(), a[2].i()))),
        "java/util/Arrays.sort([DII)V" => |vm, a| sort_range(vm, &a[0], Some((a[1].i(), a[2].i()))),
        "java/util/Arrays.sort([Ljava/lang/Object;)V" => |vm, a| sort_objects(vm, &a[0]),
        "java/util/Arrays.fill([II)V" => |vm, a| fill(vm, &a[0], &a[1]),
        "java/util/Arrays.fill([JJ)V" => |vm, a| fill(vm, &a[0], &a[1]),
        "java/util/Arrays.fill([DD)V" => |vm, a| fill(vm, &a[0], &a[1]),
        "java/util/Arrays.fill([CC)V" => |vm, a| fill(vm, &a[0], &a[1]),
        "java/util/Arrays.fill([ZZ)V" => |vm, a| fill(vm, &a[0], &a[1]),
        "java/util/Arrays.fill([Ljava/lang/Object;Ljava/lang/Object;)V" => |vm, a| fill(vm, &a[0], &a[1]),
        "java/util/Arrays.copyOf([II)[I" => |vm, a| copy_range(vm, &a[0], 0, a[1].i(), true),
        "java/util/Arrays.copyOf([JI)[J" => |vm, a| copy_range(vm, &a[0], 0, a[1].i(), true),
        "java/util/Arrays.copyOf([DI)[D" => |vm, a| copy_range(vm, &a[0], 0, a[1].i(), true),
        "java/util/Arrays.copyOf([CI)[C" => |vm, a| copy_range(vm, &a[0], 0, a[1].i(), true),
        "java/util/Arrays.copyOf([ZI)[Z" => |vm, a| copy_range(vm, &a[0], 0, a[1].i(), true),
        "java/util/Arrays.copyOfRange([III)[I" => |vm, a| copy_range(vm, &a[0], a[1].i(), a[2].i(), false),
        "java/util/Arrays.copyOfRange([JII)[J" => |vm, a| copy_range(vm, &a[0], a[1].i(), a[2].i(), false),
        "java/util/Arrays.copyOfRange([DII)[D" => |vm, a| copy_range(vm, &a[0], a[1].i(), a[2].i(), false),
        "java/util/Arrays.copyOfRange([CII)[C" => |vm, a| copy_range(vm, &a[0], a[1].i(), a[2].i(), false),
        "java/util/Arrays.equals([I[I)Z" => |vm, a| arrays_equal(vm, &a[0], &a[1]),
        "java/util/Arrays.equals([J[J)Z" => |vm, a| arrays_equal(vm, &a[0], &a[1]),
        "java/util/Arrays.equals([D[D)Z" => |vm, a| arrays_equal(vm, &a[0], &a[1]),
        "java/util/Arrays.equals([C[C)Z" => |vm, a| arrays_equal(vm, &a[0], &a[1]),
        "java/util/Arrays.equals([Z[Z)Z" => |vm, a| arrays_equal(vm, &a[0], &a[1]),
        "java/util/Arrays.equals([Ljava/lang/Object;[Ljava/lang/Object;)Z" => |vm, a| arrays_equal(vm, &a[0], &a[1]),
        "java/util/Arrays.hashCode([I)I" => |_, a| ok_i(match &a[0] { V::R(o) => match &*o.data.borrow() { Data::ArrI(v) => v.iter().fold(1i32, |h, &x| h.wrapping_mul(31).wrapping_add(x)), _ => 0 }, _ => 0 }),
        "java/util/Arrays.binarySearch([II)I" => |vm, a| binary_search(vm, &a[0], &a[1]),
        "java/util/Arrays.binarySearch([JJ)I" => |vm, a| binary_search(vm, &a[0], &a[1]),
        "java/util/Arrays.binarySearch([DD)I" => |vm, a| binary_search(vm, &a[0], &a[1]),
        "java/util/Arrays.binarySearch([CC)I" => |vm, a| binary_search(vm, &a[0], &a[1]),

        // ---- Scanner
        "java/util/Scanner.<init>(Ljava/io/InputStream;)V" => |vm, a| { let st = vm.stdin(); set_data(&a[0], Data::Scanner(st)); Ok(None) },
        "java/util/Scanner.<init>(Ljava/lang/String;)V" => |vm, a| {
            let s = str_of(vm, &a[1])?;
            set_data(&a[0], Data::Scanner(Rc::new(RefCell::new(Scanner { buf: s.to_vec(), pos: 0, eof: true }))));
            Ok(None)
        },
        "java/util/Scanner.next()Ljava/lang/String;" => |vm, a| { let t = scan_token(vm, &a[0])?; ret_rs(vm, &t) },
        "java/util/Scanner.nextInt()I" => |vm, a| scan_parse(vm, &a[0], |t| t.parse::<i32>().ok().map(V::I)),
        "java/util/Scanner.nextLong()J" => |vm, a| scan_parse(vm, &a[0], |t| t.parse::<i64>().ok().map(V::L)),
        "java/util/Scanner.nextDouble()D" => |vm, a| scan_parse(vm, &a[0], |t| t.parse::<f64>().ok().filter(|_| t.chars().any(|c| c.is_ascii_digit())).map(V::D)),
        "java/util/Scanner.nextBoolean()Z" => |vm, a| scan_parse(vm, &a[0], |t| match t.to_ascii_lowercase().as_str() { "true" => Some(V::I(1)), "false" => Some(V::I(0)), _ => None }),
        "java/util/Scanner.nextLine()Ljava/lang/String;" => |vm, a| {
            let st = scanner(&a[0]);
            match scan(vm, &st, |s, eof| s.line(eof))? {
                Some((end, next)) => {
                    let line = {
                        let mut s = st.borrow_mut();
                        let l = s.buf[s.pos..end].to_vec();
                        s.pos = next;
                        l
                    };
                    ret_s(vm, line)
                }
                None => Err(vm.throw("java/util/NoSuchElementException", Some("No line found"))),
            }
        },
        "java/util/Scanner.hasNext()Z" => |vm, a| { let st = scanner(&a[0]); ok_z(scan(vm, &st, |s, eof| s.has_token(eof))?) },
        "java/util/Scanner.hasNextInt()Z" => |vm, a| scan_has(vm, &a[0], |t| t.parse::<i32>().is_ok()),
        "java/util/Scanner.hasNextLong()Z" => |vm, a| scan_has(vm, &a[0], |t| t.parse::<i64>().is_ok()),
        "java/util/Scanner.hasNextDouble()Z" => |vm, a| scan_has(vm, &a[0], |t| t.parse::<f64>().is_ok()),
        "java/util/Scanner.hasNextLine()Z" => |vm, a| { let st = scanner(&a[0]); ok_z(scan(vm, &st, |s, eof| s.has_line(eof))?) },
        "java/util/Scanner.close()V" => |_, _| Ok(None),

        // ---- Random
        "java/util/Random.<init>()V" => |vm, a| {
            let t = (vm.host.epoch_ms() * 1000.0) as i64 ^ (vm.host.random() * 9.0e15) as i64;
            set_data(&a[0], Data::Random { seed: (t ^ MULT) & MASK, gauss: None });
            Ok(None)
        },
        "java/util/Random.<init>(J)V" => |_, a| { set_data(&a[0], Data::Random { seed: (a[1].l() ^ MULT) & MASK, gauss: None }); Ok(None) },
        "java/util/Random.setSeed(J)V" => |_, a| { set_data(&a[0], Data::Random { seed: (a[1].l() ^ MULT) & MASK, gauss: None }); Ok(None) },
        "java/util/Random.nextInt()I" => |_, a| ok_i(rnd_next_int(&a[0])),
        "java/util/Random.nextInt(I)I" => |vm, a| {
            let b = a[1].i();
            if b <= 0 {
                return Err(vm.throw("java/lang/IllegalArgumentException", Some("bound must be positive")));
            }
            ok_i(rnd_bounded(&a[0], b))
        },
        "java/util/Random.nextInt(II)I" => |vm, a| {
            let (origin, bound) = (a[1].i(), a[2].i());
            if origin >= bound {
                return Err(vm.throw("java/lang/IllegalArgumentException", Some("bound must be greater than origin")));
            }
            let mut r = rnd_next_int(&a[0]);
            let n = bound.wrapping_sub(origin);
            let m = n.wrapping_sub(1);
            if n & m == 0 {
                r = (r & m) + origin;
            } else if n > 0 {
                let mut u = ((r as u32) >> 1) as i32;
                loop {
                    r = u % n;
                    if u.wrapping_add(m).wrapping_sub(r) >= 0 {
                        break;
                    }
                    u = ((rnd_next_int(&a[0]) as u32) >> 1) as i32;
                }
                r += origin;
            } else {
                while r < origin || r >= bound {
                    r = rnd_next_int(&a[0]);
                }
            }
            ok_i(r)
        },
        "java/util/Random.nextLong()J" => |_, a| { let hi = rnd_next(&a[0], 32) as i64; let lo = rnd_next(&a[0], 32) as i64; ok_l((hi << 32).wrapping_add(lo)) },
        "java/util/Random.nextDouble()D" => |_, a| ok_d(rnd_double(&a[0])),
        "java/util/Random.nextBoolean()Z" => |_, a| ok_z(rnd_next(&a[0], 1) != 0),
        "java/util/Random.nextGaussian()D" => |_, a| {
            if let V::R(o) = &a[0] {
                if let Data::Random { gauss, .. } = &mut *o.data.borrow_mut() {
                    if let Some(g) = gauss.take() {
                        return ok_d(g);
                    }
                }
            }
            loop {
                let v1 = 2.0 * rnd_double(&a[0]) - 1.0;
                let v2 = 2.0 * rnd_double(&a[0]) - 1.0;
                let s = v1 * v1 + v2 * v2;
                if s < 1.0 && s != 0.0 {
                    let mul = (-2.0 * s.ln() / s).sqrt();
                    if let V::R(o) = &a[0] {
                        if let Data::Random { gauss, .. } = &mut *o.data.borrow_mut() {
                            *gauss = Some(v2 * mul);
                        }
                    }
                    return ok_d(v1 * mul);
                }
            }
        },

        // ---- Throwable
        "java/lang/Throwable.<init>()V" => |_, _| Ok(None),
        "java/lang/Throwable.<init>(Ljava/lang/String;)V" => |_, a| { set_throwable(&a[0], 0, a[1].clone()); Ok(None) },
        "java/lang/Throwable.<init>(Ljava/lang/String;Ljava/lang/Throwable;)V" => |_, a| { set_throwable(&a[0], 0, a[1].clone()); set_throwable(&a[0], 1, a[2].clone()); Ok(None) },
        "java/lang/Throwable.<init>(Ljava/lang/Throwable;)V" => |vm, a| {
            let msg = match &a[1] { V::Null => V::Null, c => { let s = to_jstring(vm, c)?; vm.str_from(s) } };
            set_throwable(&a[0], 0, msg);
            set_throwable(&a[0], 1, a[1].clone());
            Ok(None)
        },
        "java/lang/Throwable.getMessage()Ljava/lang/String;" => |_, a| Ok(Some(throwable_fields(&a[0]).map(|f| f.0).unwrap_or(V::Null))),
        "java/lang/Throwable.getLocalizedMessage()Ljava/lang/String;" => |vm, a| vm.call_virtual(a[0].clone(), "getMessage", "()Ljava/lang/String;", vec![]),
        "java/lang/Throwable.getCause()Ljava/lang/Throwable;" => |_, a| Ok(Some(throwable_fields(&a[0]).map(|f| f.1).unwrap_or(V::Null))),
        "java/lang/Throwable.toString()Ljava/lang/String;" => |vm, a| {
            let name = vm.class_name(a[0].obj().unwrap());
            let msg = vm.call_virtual(a[0].clone(), "getLocalizedMessage", "()Ljava/lang/String;", vec![])?.unwrap_or(V::Null);
            match rust_string(&msg) {
                Some(m) => ret_rs(vm, &format!("{}: {}", name, m)),
                None => ret_rs(vm, &name),
            }
        },
        "java/lang/Throwable.printStackTrace()V" => |vm, a| {
            let e = a[0].obj().unwrap().clone();
            let text = vm.describe_uncaught(&e);
            let text = text.trim_start_matches("Exception in thread \"main\" ").to_string();
            vm.emit(2, &text);
            Ok(None)
        },
    });

    // Boxed values: toString / equals / hashCode / compareTo for every wrapper class.
    for (_, cls) in BOX_KINDS {
        let base = cls.to_string();
        let entries: [(&str, Nf); 4] = [
            (".toString()Ljava/lang/String;", |vm, a| {
                let o = a[0].obj().unwrap().clone();
                let s = boxed_to_string(vm, &o);
                ret_rs(vm, &s)
            }),
            (".equals(Ljava/lang/Object;)Z", |vm, a| {
                let (x, y) = (a[0].obj().unwrap().clone(), a[1].clone());
                let Some(yo) = y.obj().cloned() else { return ok_z(false) };
                if x.cls != yo.cls {
                    return ok_z(false);
                }
                let _ = vm;
                ok_z(match (unbox(&a[0]), unbox(&y)) {
                    (Some(V::I(p)), Some(V::I(q))) => p == q,
                    (Some(V::L(p)), Some(V::L(q))) => p == q,
                    (Some(V::D(p)), Some(V::D(q))) => p.to_bits() == q.to_bits() || (p.is_nan() && q.is_nan()),
                    _ => false,
                })
            }),
            (".hashCode()I", |vm, a| {
                let o = a[0].obj().unwrap().clone();
                let k = box_kind(vm, &o);
                ok_i(match (k, unbox(&a[0])) {
                    (b'Z', Some(V::I(z))) => {
                        if z != 0 {
                            1231
                        } else {
                            1237
                        }
                    }
                    (_, Some(V::I(x))) => x,
                    (_, Some(V::L(x))) => (x ^ (x >> 32 & 0xffff_ffff)) as i32,
                    (_, Some(V::D(d))) => {
                        let b = if d.is_nan() { 0x7ff8000000000000u64 } else { d.to_bits() };
                        (b ^ (b >> 32)) as i32
                    }
                    _ => 0,
                })
            }),
            (".compareTo(Ljava/lang/Object;)I", |vm, a| {
                let r = compare_objects(vm, &a[0], &a[1])?;
                ok_i(r)
            }),
        ];
        for (suffix, f) in entries {
            vm.natives.push((format!("{}{}", base, suffix), f));
        }
    }

    for (i, (k, _)) in vm.natives.iter().enumerate() {
        vm.native_map.insert(k.clone(), i);
    }
}

fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_positive() {
            a
        } else {
            b
        }
    } else {
        a.max(b)
    }
}

fn jmin(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() {
            a
        } else {
            b
        }
    } else {
        a.min(b)
    }
}

fn jpow(a: f64, b: f64) -> f64 {
    if b.is_nan() {
        return f64::NAN;
    }
    if b == 0.0 {
        return 1.0;
    }
    if (a == 1.0 || a == -1.0) && b.is_infinite() {
        return f64::NAN;
    }
    a.powf(b)
}

fn set_throwable(v: &V, idx: usize, val: V) {
    if let V::R(o) = v {
        if let Data::Fields(f) = &mut *o.data.borrow_mut() {
            if f.len() > idx {
                f[idx] = val;
            }
        }
    }
}

fn printf(vm: &mut Vm, a: &[V]) -> R {
    let f = str_of(vm, &a[1])?;
    let args = match &a[2] {
        V::R(o) => match &*o.data.borrow() {
            Data::ArrA(v) => v.clone(),
            _ => vec![],
        },
        _ => vec![V::Null],
    };
    let s = java_format(vm, &f, &args)?;
    print_to(vm, &a[0], &s)?;
    Ok(Some(a[0].clone()))
}

fn scanner(v: &V) -> Rc<RefCell<Scanner>> {
    match v {
        V::R(o) => match &*o.data.borrow() {
            Data::Scanner(s) => s.clone(),
            _ => panic!("not a scanner"),
        },
        _ => panic!("null scanner"),
    }
}

fn scan_token(vm: &mut Vm, v: &V) -> Result<String, Ex> {
    let st = scanner(v);
    match scan(vm, &st, |s, eof| s.token(eof))? {
        Some((a, b)) => {
            let t = st.borrow().text(a, b);
            st.borrow_mut().pos = b;
            Ok(t)
        }
        None => Err(vm.throw("java/util/NoSuchElementException", None)),
    }
}

fn scan_parse(vm: &mut Vm, v: &V, parse: impl Fn(&str) -> Option<V>) -> R {
    let st = scanner(v);
    match scan(vm, &st, |s, eof| s.token(eof))? {
        None => Err(vm.throw("java/util/NoSuchElementException", None)),
        Some((a, b)) => {
            let text = st.borrow().text(a, b);
            match parse(&text) {
                Some(val) => {
                    st.borrow_mut().pos = b;
                    Ok(Some(val))
                }
                // Like Java, the offending token is left in place.
                None => Err(vm.throw("java/util/InputMismatchException", None)),
            }
        }
    }
}

fn scan_has(vm: &mut Vm, v: &V, ok: impl Fn(&str) -> bool) -> R {
    let st = scanner(v);
    let tok = scan(vm, &st, |s, eof| s.token(eof))?;
    let yes = tok.map_or(false, |(a, b)| ok(&st.borrow().text(a, b)));
    ok_z(yes)
}

fn arraycopy(vm: &mut Vm, a: &[V]) -> R {
    let (src, dst) = match (&a[0], &a[2]) {
        (V::R(s), V::R(d)) => (s.clone(), d.clone()),
        _ => return Err(npe(vm)),
    };
    let (sp, dp, n) = (a[1].i(), a[3].i(), a[4].i());
    let slen = arr_len(&a[0]).unwrap_or(0) as i64;
    let dlen = arr_len(&a[2]).unwrap_or(0) as i64;
    let tname = |vm: &Vm, o: &Ref| -> String {
        let n = vm.classes[o.cls].name.clone();
        match &n[1..] {
            "I" => "int[]".into(),
            "J" => "long[]".into(),
            "D" => "double[]".into(),
            "C" => "char[]".into(),
            "Z" => "boolean[]".into(),
            _ => "object array[]".into(),
        }
    };
    if vm.classes[src.cls].elem.is_none() || vm.classes[dst.cls].elem.is_none() {
        let msg = "arraycopy: source type is not an array".to_string();
        return Err(vm.throw("java/lang/ArrayStoreException", Some(&msg)));
    }
    let se = vm.classes[src.cls].elem.clone().unwrap();
    let de = vm.classes[dst.cls].elem.clone().unwrap();
    let prim = |e: &str| !e.starts_with('L') && !e.starts_with('[');
    if (prim(&se) || prim(&de)) && se != de {
        let msg = format!(
            "arraycopy: type mismatch: can not copy {} into {}",
            tname(vm, &src),
            tname(vm, &dst)
        );
        return Err(vm.throw("java/lang/ArrayStoreException", Some(&msg)));
    }
    if sp < 0 || dp < 0 || n < 0 || sp as i64 + n as i64 > slen || dp as i64 + n as i64 > dlen {
        let msg = if n < 0 {
            format!("arraycopy: length {} is negative", n)
        } else if sp < 0 {
            format!("arraycopy: source index {} out of bounds for {}", sp, tname(vm, &src))
        } else if dp < 0 {
            format!(
                "arraycopy: destination index {} out of bounds for {}",
                dp,
                tname(vm, &dst)
            )
        } else if sp as i64 + n as i64 > slen {
            format!(
                "arraycopy: last source index {} out of bounds for length {}",
                sp as i64 + n as i64,
                slen
            )
        } else {
            format!(
                "arraycopy: last destination index {} out of bounds for length {}",
                dp as i64 + n as i64,
                dlen
            )
        };
        return Err(vm.throw("java/lang/ArrayIndexOutOfBoundsException", Some(&msg)));
    }
    let (sp, dp, n) = (sp as usize, dp as usize, n as usize);
    macro_rules! copy {
        ($var:ident) => {{
            let tmp = match &*src.data.borrow() {
                Data::$var(v) => v[sp..sp + n].to_vec(),
                _ => unreachable!(),
            };
            if let Data::$var(v) = &mut *dst.data.borrow_mut() {
                v[dp..dp + n].clone_from_slice(&tmp);
            }
        }};
    }
    match &se[..1] {
        "I" => copy!(ArrI),
        "J" => copy!(ArrJ),
        "D" => copy!(ArrD),
        "C" => copy!(ArrC),
        "Z" => copy!(ArrZ),
        _ => copy!(ArrA),
    }
    Ok(None)
}

fn range_check(vm: &mut Vm, len: usize, from: i32, to: i32) -> Result<(), Ex> {
    if from > to {
        return Err(vm.throw(
            "java/lang/IllegalArgumentException",
            Some(&format!("fromIndex({}) > toIndex({})", from, to)),
        ));
    }
    if from < 0 {
        return Err(vm.throw(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&format!("Array index out of range: {}", from)),
        ));
    }
    if to as usize > len {
        return Err(vm.throw(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&format!("Array index out of range: {}", to)),
        ));
    }
    Ok(())
}

fn sort_range(vm: &mut Vm, arr: &V, range: Option<(i32, i32)>) -> R {
    let Some(len) = arr_len(arr) else { return Err(npe(vm)) };
    let (from, to) = match range {
        Some((f, t)) => {
            range_check(vm, len, f, t)?;
            (f as usize, t as usize)
        }
        None => (0, len),
    };
    let o = arr.obj().unwrap();
    match &mut *o.data.borrow_mut() {
        Data::ArrI(v) => v[from..to].sort_unstable(),
        Data::ArrJ(v) => v[from..to].sort_unstable(),
        Data::ArrC(v) => v[from..to].sort_unstable(),
        Data::ArrD(v) => v[from..to].sort_by(|a, b| java_double_cmp(*a, *b)),
        _ => {}
    }
    Ok(None)
}

fn sort_objects(vm: &mut Vm, arr: &V) -> R {
    let Some(o) = arr.obj().cloned() else {
        return Err(npe(vm));
    };
    let mut items = match &*o.data.borrow() {
        Data::ArrA(v) => v.clone(),
        _ => return Ok(None),
    };
    // Stable merge sort with a fallible comparator (like TimSort's ordering).
    fn merge_sort(vm: &mut Vm, v: &mut Vec<V>) -> Result<(), Ex> {
        if v.len() <= 1 {
            return Ok(());
        }
        let mut right = v.split_off(v.len() / 2);
        merge_sort(vm, v)?;
        merge_sort(vm, &mut right)?;
        let left = std::mem::take(v);
        let (mut i, mut j) = (0, 0);
        while i < left.len() && j < right.len() {
            if compare_objects(vm, &left[i], &right[j])? <= 0 {
                v.push(left[i].clone());
                i += 1;
            } else {
                v.push(right[j].clone());
                j += 1;
            }
        }
        v.extend_from_slice(&left[i..]);
        v.extend_from_slice(&right[j..]);
        Ok(())
    }
    merge_sort(vm, &mut items)?;
    *o.data.borrow_mut() = Data::ArrA(items);
    Ok(None)
}

fn fill(vm: &mut Vm, arr: &V, val: &V) -> R {
    let Some(o) = arr.obj() else { return Err(npe(vm)) };
    match &mut *o.data.borrow_mut() {
        Data::ArrI(v) => v.fill(val.i()),
        Data::ArrJ(v) => v.fill(val.l()),
        Data::ArrD(v) => v.fill(val.d()),
        Data::ArrC(v) => v.fill(val.i() as u16),
        Data::ArrZ(v) => v.fill(val.i() as i8),
        Data::ArrA(v) => v.fill(val.clone()),
        _ => {}
    }
    Ok(None)
}

fn copy_range(vm: &mut Vm, arr: &V, from: i32, to: i32, is_copy_of: bool) -> R {
    let Some(o) = arr.obj().cloned() else {
        return Err(npe(vm));
    };
    let len = arr_len(arr).unwrap() as i32;
    if is_copy_of && to < 0 {
        return Err(vm.throw("java/lang/NegativeArraySizeException", Some(&to.to_string())));
    }
    if !is_copy_of {
        if from > to {
            return Err(vm.throw(
                "java/lang/IllegalArgumentException",
                Some(&format!("{} > {}", from, to)),
            ));
        }
        if from < 0 || from > len {
            return Err(vm.throw(
                "java/lang/ArrayIndexOutOfBoundsException",
                Some(&format!("Array index out of range: {}", from)),
            ));
        }
    }
    let (f, n) = (from as usize, (to - from) as usize);
    let avail = (len as usize).saturating_sub(f).min(n);
    let data = match &*o.data.borrow() {
        Data::ArrI(v) => {
            let mut c = v[f..f + avail].to_vec();
            c.resize(n, 0);
            Data::ArrI(c)
        }
        Data::ArrJ(v) => {
            let mut c = v[f..f + avail].to_vec();
            c.resize(n, 0);
            Data::ArrJ(c)
        }
        Data::ArrD(v) => {
            let mut c = v[f..f + avail].to_vec();
            c.resize(n, 0.0);
            Data::ArrD(c)
        }
        Data::ArrC(v) => {
            let mut c = v[f..f + avail].to_vec();
            c.resize(n, 0);
            Data::ArrC(c)
        }
        Data::ArrZ(v) => {
            let mut c = v[f..f + avail].to_vec();
            c.resize(n, 0);
            Data::ArrZ(c)
        }
        Data::ArrA(v) => {
            let mut c = v[f..f + avail].to_vec();
            c.resize(n, V::Null);
            Data::ArrA(c)
        }
        _ => return Ok(Some(V::Null)),
    };
    let r = vm.new_obj(o.cls, data);
    Ok(Some(V::R(r)))
}

fn arrays_equal(vm: &mut Vm, a: &V, b: &V) -> R {
    let (x, y) = match (a, b) {
        (V::Null, V::Null) => return ok_z(true),
        (V::R(x), V::R(y)) => (x.clone(), y.clone()),
        _ => return ok_z(false),
    };
    if Rc::ptr_eq(&x, &y) {
        return ok_z(true);
    }
    let objs = {
        let (dx, dy) = (x.data.borrow(), y.data.borrow());
        match (&*dx, &*dy) {
            (Data::ArrI(p), Data::ArrI(q)) => return ok_z(p == q),
            (Data::ArrJ(p), Data::ArrJ(q)) => return ok_z(p == q),
            (Data::ArrC(p), Data::ArrC(q)) => return ok_z(p == q),
            (Data::ArrZ(p), Data::ArrZ(q)) => return ok_z(p == q),
            (Data::ArrD(p), Data::ArrD(q)) => {
                return ok_z(
                    p.len() == q.len()
                        && p.iter()
                            .zip(q)
                            .all(|(a, b)| a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())),
                )
            }
            (Data::ArrA(p), Data::ArrA(q)) => (p.clone(), q.clone()),
            _ => return ok_z(false),
        }
    };
    if objs.0.len() != objs.1.len() {
        return ok_z(false);
    }
    for (p, q) in objs.0.iter().zip(&objs.1) {
        let eq = match (p, q) {
            (V::Null, V::Null) => true,
            (V::Null, _) | (_, V::Null) => false,
            _ => vm
                .call_virtual(p.clone(), "equals", "(Ljava/lang/Object;)Z", vec![q.clone()])?
                .map_or(false, |v| v.i() != 0),
        };
        if !eq {
            return ok_z(false);
        }
    }
    ok_z(true)
}

fn binary_search(vm: &mut Vm, arr: &V, key: &V) -> R {
    let Some(o) = arr.obj() else { return Err(npe(vm)) };
    let cmp_at = |d: &Data, i: usize| -> std::cmp::Ordering {
        match d {
            Data::ArrI(v) => v[i].cmp(&key.i()),
            Data::ArrJ(v) => v[i].cmp(&key.l()),
            Data::ArrC(v) => (v[i] as i32).cmp(&key.i()),
            Data::ArrD(v) => java_double_cmp(v[i], key.d()),
            _ => std::cmp::Ordering::Equal,
        }
    };
    let d = o.data.borrow();
    let len = arr_len(arr).unwrap();
    let (mut lo, mut hi) = (0i64, len as i64 - 1);
    while lo <= hi {
        let mid = ((lo + hi) as u64 >> 1) as i64;
        match cmp_at(&d, mid as usize) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid - 1,
            std::cmp::Ordering::Equal => return ok_i(mid as i32),
        }
    }
    ok_i(-(lo as i32 + 1))
}
