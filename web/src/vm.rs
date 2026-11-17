//! A small JVM for exactly the bytecode subset jolt emits.
//!
//! User classes are loaded from real class files and interpreted; the slice of
//! the JDK that jolt knows about is implemented natively in `natives.rs`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jolt::classes::ClassTable;
use jolt::fxhash::FxMap;

pub type Ref = Rc<Obj>;

#[derive(Clone)]
pub enum V {
    I(i32),
    L(i64),
    D(f64),
    Null,
    R(Ref),
}

impl V {
    #[inline]
    pub fn i(&self) -> i32 {
        match self {
            V::I(x) => *x,
            _ => 0,
        }
    }
    #[inline]
    pub fn l(&self) -> i64 {
        match self {
            V::L(x) => *x,
            _ => 0,
        }
    }
    #[inline]
    pub fn d(&self) -> f64 {
        match self {
            V::D(x) => *x,
            _ => 0.0,
        }
    }
    #[inline]
    pub fn wide(&self) -> bool {
        matches!(self, V::L(_) | V::D(_))
    }
    #[inline]
    pub fn obj(&self) -> Option<&Ref> {
        match self {
            V::R(r) => Some(r),
            _ => None,
        }
    }
    pub fn same(&self, o: &V) -> bool {
        match (self, o) {
            (V::Null, V::Null) => true,
            (V::R(a), V::R(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

pub struct Obj {
    pub cls: usize,
    pub data: RefCell<Data>,
    pub hash: Cell<i32>,
}

pub enum Data {
    /// A JDK object between `new` and its `<init>`.
    Pending,
    Plain,
    Fields(Vec<V>),
    Str(Rc<[u16]>),
    Sb(Vec<u16>),
    ArrI(Vec<i32>),
    ArrJ(Vec<i64>),
    ArrD(Vec<f64>),
    ArrZ(Vec<i8>),
    ArrC(Vec<u16>),
    ArrA(Vec<V>),
    Boxed(V),
    Random {
        seed: i64,
        gauss: Option<f64>,
    },
    Scanner(Rc<RefCell<crate::natives::Scanner>>),
    Stream(u32),
}

pub enum Ex {
    Throw(Ref),
    Exit(i32),
    Fatal(String),
    /// The program is waiting for input that has not been typed yet.
    NeedInput,
}

pub type R = Result<Option<V>, Ex>;

/// Throwables keep message, cause and stack trace in three hidden fields.
pub const THROWABLE_FIELDS: usize = 3;

// ---------------------------------------------------------------- class files

#[derive(Clone)]
pub enum Cp {
    Empty,
    Utf8(Rc<str>),
    Int(i32),
    Long(i64),
    Double(f64),
    Class(u16),
    Str(u16),
    NameType(u16, u16),
    Member(u16, u16),
}

pub struct RawMethod {
    pub flags: u16,
    pub name: Rc<str>,
    pub desc: Rc<str>,
    pub code: Option<Code>,
}

pub struct Code {
    pub max_locals: usize,
    pub code: Vec<u8>,
    pub exc: Vec<[u16; 4]>,
    pub lines: Vec<(u16, u16)>,
}

pub struct ClassFile {
    pub cp: Vec<Cp>,
    pub flags: u16,
    pub name: Rc<str>,
    pub super_name: Option<Rc<str>>,
    pub interfaces: Vec<Rc<str>>,
    pub fields: Vec<(u16, Rc<str>, Rc<str>)>,
    pub methods: Vec<RawMethod>,
    pub source: Rc<str>,
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, String> {
        let v = *self.b.get(self.i).ok_or("truncated class file")?;
        self.i += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(((self.u8()? as u16) << 8) | self.u8()? as u16)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(((self.u16()? as u32) << 16) | self.u16()? as u32)
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.b.get(self.i..self.i + n).ok_or("truncated class file")?;
        self.i += n;
        Ok(s)
    }
}

/// Decode "modified UTF-8" as used in class files.
pub fn decode_mutf8(b: &[u8]) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c < 0x80 {
            units.push(c as u16);
            i += 1;
        } else if c & 0xe0 == 0xc0 && i + 1 < b.len() {
            units.push((((c & 0x1f) as u16) << 6) | (b[i + 1] & 0x3f) as u16);
            i += 2;
        } else if i + 2 < b.len() {
            units.push((((c & 0x0f) as u16) << 12) | (((b[i + 1] & 0x3f) as u16) << 6) | (b[i + 2] & 0x3f) as u16);
            i += 3;
        } else {
            i += 1;
        }
    }
    String::from_utf16_lossy(&units)
}

pub fn parse_class(bytes: &[u8]) -> Result<ClassFile, String> {
    let mut r = Reader { b: bytes, i: 0 };
    if r.u32()? != 0xCAFEBABE {
        return Err("not a class file".into());
    }
    r.u16()?;
    r.u16()?;
    let n = r.u16()? as usize;
    let mut cp = vec![Cp::Empty; n.max(1)];
    let mut i = 1;
    while i < n {
        let tag = r.u8()?;
        cp[i] = match tag {
            1 => {
                let len = r.u16()? as usize;
                Cp::Utf8(decode_mutf8(r.bytes(len)?).into())
            }
            3 => Cp::Int(r.u32()? as i32),
            5 => {
                let hi = r.u32()? as u64;
                let lo = r.u32()? as u64;
                Cp::Long(((hi << 32) | lo) as i64)
            }
            6 => {
                let hi = r.u32()? as u64;
                let lo = r.u32()? as u64;
                Cp::Double(f64::from_bits((hi << 32) | lo))
            }
            7 => Cp::Class(r.u16()?),
            8 => Cp::Str(r.u16()?),
            9 | 10 | 11 => Cp::Member(r.u16()?, r.u16()?),
            12 => Cp::NameType(r.u16()?, r.u16()?),
            t => return Err(format!("unsupported constant pool tag {}", t)),
        };
        i += if tag == 5 || tag == 6 { 2 } else { 1 };
    }
    let utf = |cp: &Vec<Cp>, i: u16| -> Result<Rc<str>, String> {
        match cp.get(i as usize) {
            Some(Cp::Utf8(s)) => Ok(s.clone()),
            _ => Err("bad constant pool reference".into()),
        }
    };
    let class_name = |cp: &Vec<Cp>, i: u16| -> Result<Rc<str>, String> {
        match cp.get(i as usize) {
            Some(Cp::Class(u)) => utf(cp, *u),
            _ => Err("bad class reference".into()),
        }
    };
    let flags = r.u16()?;
    let name = class_name(&cp, r.u16()?)?;
    let sup = r.u16()?;
    let super_name = if sup == 0 { None } else { Some(class_name(&cp, sup)?) };
    let mut interfaces = Vec::new();
    for _ in 0..r.u16()? {
        interfaces.push(class_name(&cp, r.u16()?)?);
    }
    let mut fields = Vec::new();
    for _ in 0..r.u16()? {
        let f = r.u16()?;
        let nm = utf(&cp, r.u16()?)?;
        let d = utf(&cp, r.u16()?)?;
        for _ in 0..r.u16()? {
            r.u16()?;
            let len = r.u32()? as usize;
            r.bytes(len)?;
        }
        fields.push((f, nm, d));
    }
    let mut methods = Vec::new();
    for _ in 0..r.u16()? {
        let mflags = r.u16()?;
        let mname = utf(&cp, r.u16()?)?;
        let desc = utf(&cp, r.u16()?)?;
        let mut code = None;
        for _ in 0..r.u16()? {
            let an = utf(&cp, r.u16()?)?;
            let len = r.u32()? as usize;
            let body = r.bytes(len)?;
            if &*an == "Code" {
                let mut c = Reader { b: body, i: 0 };
                c.u16()?;
                let max_locals = c.u16()? as usize;
                let clen = c.u32()? as usize;
                let bc = c.bytes(clen)?.to_vec();
                let mut exc = Vec::new();
                for _ in 0..c.u16()? {
                    exc.push([c.u16()?, c.u16()?, c.u16()?, c.u16()?]);
                }
                let mut lines = Vec::new();
                for _ in 0..c.u16()? {
                    let an = utf(&cp, c.u16()?)?;
                    let alen = c.u32()? as usize;
                    let ab = c.bytes(alen)?;
                    if &*an == "LineNumberTable" {
                        let mut l = Reader { b: ab, i: 0 };
                        for _ in 0..l.u16()? {
                            lines.push((l.u16()?, l.u16()?));
                        }
                    }
                }
                code = Some(Code {
                    max_locals,
                    code: bc,
                    exc,
                    lines,
                });
            }
        }
        methods.push(RawMethod {
            flags: mflags,
            name: mname,
            desc,
            code,
        });
    }
    let mut source: Rc<str> = "Unknown".into();
    for _ in 0..r.u16()? {
        let an = utf(&cp, r.u16()?)?;
        let len = r.u32()? as usize;
        let body = r.bytes(len)?;
        if &*an == "SourceFile" && body.len() == 2 {
            source = utf(&cp, ((body[0] as u16) << 8) | body[1] as u16)?;
        }
    }
    Ok(ClassFile {
        cp,
        flags,
        name,
        super_name,
        interfaces,
        fields,
        methods,
        source,
    })
}

// ---------------------------------------------------------------- runtime classes

pub struct Method {
    pub name: Rc<str>,
    pub desc: Rc<str>,
    pub flags: u16,
    pub cls: usize,
    pub code: Vec<u8>,
    pub max_locals: usize,
    pub exc: Vec<[u16; 4]>,
    pub lines: Vec<(u16, u16)>,
    pub nargs: usize,
    pub is_static: bool,
    pub is_abstract: bool,
}

impl Method {
    pub fn line_at(&self, pc: usize) -> u16 {
        let mut line = 0;
        for &(start, l) in &self.lines {
            if start as usize <= pc {
                line = l;
            }
        }
        line
    }
}

#[derive(Clone)]
pub enum Target {
    Code(Rc<Method>),
    Native(usize),
    Abstract,
}

#[derive(Clone)]
pub enum Res {
    None,
    Static {
        cls: usize,
        slot: usize,
    },
    JdkStatic(V),
    Field(usize),
    Call {
        target: Target,
        nargs: usize,
        cls: usize,
    },
    Virtual {
        key: Rc<str>,
        name: Rc<str>,
        desc: Rc<str>,
        nargs: usize,
    },
    Class(usize),
    Val(V),
}

pub struct Class {
    pub name: Rc<str>,
    pub sup: Option<usize>,
    pub ifaces: Vec<usize>,
    pub is_interface: bool,
    pub file: Option<Rc<ClassFile>>,
    pub methods: Vec<Rc<Method>>,
    pub nfields: usize,
    pub own_fields: Vec<(Rc<str>, usize)>,
    pub defaults: Vec<V>,
    pub statics: RefCell<Vec<V>>,
    pub static_names: Vec<(Rc<str>, usize)>,
    pub init: Cell<u8>,
    pub cache: RefCell<Vec<Res>>,
    pub is_throwable: bool,
    /// Element descriptor for array classes.
    pub elem: Option<Rc<str>>,
}

pub fn default_for(desc: &str) -> V {
    match desc.as_bytes()[0] {
        b'J' => V::L(0),
        b'D' => V::D(0.0),
        b'L' | b'[' => V::Null,
        _ => V::I(0),
    }
}

/// Count parameters in a method descriptor.
pub fn count_params(desc: &str) -> usize {
    let b = desc.as_bytes();
    let mut i = 1;
    let mut n = 0;
    while i < b.len() && b[i] != b')' {
        while b[i] == b'[' {
            i += 1;
        }
        if b[i] == b'L' {
            while b[i] != b';' {
                i += 1;
            }
        }
        i += 1;
        n += 1;
    }
    n
}

pub struct Frame {
    pub m: Rc<Method>,
    pub pc: usize,
    pub cur: usize,
    pub locals: Vec<V>,
    pub stack: Vec<V>,
}

pub trait Host {
    fn write(&mut self, fd: u32, s: &str);
    fn now_ms(&mut self) -> f64;
    fn epoch_ms(&mut self) -> f64;
    fn random(&mut self) -> f64;
    fn sleep_ms(&mut self, ms: f64);
    /// Called the first time a program reads System.in (if input was not given up front).
    fn read_stdin(&mut self) -> String {
        String::new()
    }
}

pub struct Vm {
    pub host: Box<dyn Host>,
    pub classes: Vec<Rc<Class>>,
    pub by_name: FxMap<Rc<str>, usize>,
    pub jdk: ClassTable,
    pub natives: Vec<(String, crate::natives::Nf)>,
    pub native_map: FxMap<String, usize>,
    pub vcache: FxMap<(usize, usize), Target>,
    pub frames: Vec<Frame>,
    pub interned: FxMap<Rc<[u16]>, Ref>,
    stdin: Rc<RefCell<crate::natives::Scanner>>,
    stdin_loaded: bool,
    pub out: Vec<u8>,
    pub stdout_obj: Option<Ref>,
    pub stderr_obj: Option<Ref>,
    pub stdin_obj: Option<Ref>,
    pub hash_seed: u32,
    pub boxed_cache: FxMap<(u8, i32), Ref>,
    pub steps: u64,
    pub max_frames: usize,
    /// Pause (instead of hitting end-of-input) when System.in runs dry.
    interactive: bool,
    main_started: bool,
    run_depth: usize,
    last_flush: f64,
}

pub enum RunOutcome {
    Exited(i32),
    /// Paused: waiting for more input (interactive mode only).
    NeedInput,
}

enum Flow {
    Call(Frame),
    Return(Option<V>),
}

impl Vm {
    /// `stdin`: the program's input, or `None` to ask the host lazily.
    pub fn new(host: Box<dyn Host>, stdin: Option<&str>) -> Vm {
        let mut vm = Vm {
            host,
            classes: Vec::new(),
            by_name: FxMap::default(),
            jdk: ClassTable::with_jdk(),
            natives: Vec::new(),
            native_map: FxMap::default(),
            vcache: FxMap::default(),
            frames: Vec::new(),
            interned: FxMap::default(),
            stdin: Rc::new(RefCell::new(crate::natives::Scanner::new(stdin.unwrap_or("")))),
            stdin_loaded: stdin.is_some(),
            out: Vec::new(),
            stdout_obj: None,
            stderr_obj: None,
            stdin_obj: None,
            hash_seed: 0x2545_f491,
            boxed_cache: FxMap::default(),
            steps: 0,
            max_frames: 3000,
            interactive: false,
            main_started: false,
            run_depth: 0,
            last_flush: 0.0,
        };
        crate::natives::register(&mut vm);
        vm
    }

    /// Interactive mode: when the program needs more input than it has, it
    /// pauses (`RunOutcome::NeedInput`) until `feed_input` or `end_input`.
    pub fn set_interactive(&mut self) {
        self.interactive = true;
        self.stdin_loaded = true;
        self.stdin.borrow_mut().eof = false;
    }

    pub fn feed_input(&mut self, text: &str) {
        self.stdin_loaded = true;
        self.stdin.borrow_mut().buf.extend(text.encode_utf16());
    }

    pub fn end_input(&mut self) {
        self.stdin_loaded = true;
        self.stdin.borrow_mut().eof = true;
    }

    /// Only the outermost interpreter loop running `main` can pause: nested
    /// runs (static initializers, callbacks from natives) live on the Rust stack.
    pub fn can_suspend(&self) -> bool {
        self.interactive && self.main_started && self.run_depth == 1
    }

    pub fn stdin(&mut self) -> Rc<RefCell<crate::natives::Scanner>> {
        if !self.stdin_loaded {
            self.stdin_loaded = true;
            self.flush();
            let s = self.host.read_stdin();
            *self.stdin.borrow_mut() = crate::natives::Scanner::new(&s);
        }
        self.stdin.clone()
    }

    // ---------- output ----------
    pub fn emit(&mut self, fd: u32, s: &str) {
        if fd == 1 {
            self.out.extend_from_slice(s.as_bytes());
            if self.out.len() > 8192 || self.host.now_ms() - self.last_flush > 50.0 {
                self.flush();
            }
        } else {
            self.flush();
            self.host.write(fd, s);
        }
    }
    pub fn flush(&mut self) {
        self.last_flush = self.host.now_ms();
        if !self.out.is_empty() {
            let s = String::from_utf8_lossy(&self.out).into_owned();
            self.out.clear();
            self.host.write(1, &s);
        }
    }

    // ---------- classes ----------
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), String> {
        let cf = Rc::new(parse_class(bytes)?);
        // Placeholder registration; linked lazily on first use.
        let name = cf.name.clone();
        let id = self.classes.len();
        self.classes.push(Rc::new(Class {
            name: name.clone(),
            sup: None,
            ifaces: vec![],
            is_interface: cf.flags & 0x0200 != 0,
            file: Some(cf),
            methods: vec![],
            nfields: 0,
            own_fields: vec![],
            defaults: vec![],
            statics: RefCell::new(vec![]),
            static_names: vec![],
            init: Cell::new(0xff), // 0xff = loaded but not linked
            cache: RefCell::new(vec![]),
            is_throwable: false,
            elem: None,
        }));
        self.by_name.insert(name, id);
        Ok(())
    }

    /// Look up (and link if needed) a class by internal name or array descriptor.
    pub fn class_id(&mut self, name: &str) -> Result<usize, Ex> {
        if let Some(&id) = self.by_name.get(name) {
            if self.classes[id].init.get() == 0xff {
                self.link(id)?;
            }
            return Ok(id);
        }
        // JDK class or array class.
        let (sup_name, iface_names, elem): (Option<String>, Vec<String>, Option<Rc<str>>) =
            if let Some(e) = name.strip_prefix('[') {
                (Some("java/lang/Object".into()), vec![], Some(e.into()))
            } else if let Some(ci) = self.jdk.get(name) {
                (ci.super_name.clone(), ci.interfaces.clone(), None)
            } else if name == "java/lang/Object" {
                (None, vec![], None)
            } else {
                return Err(Ex::Fatal(format!("NoClassDefFoundError: {}", name.replace('/', "."))));
            };
        let sup = match sup_name {
            Some(s) => Some(self.class_id(&s)?),
            None => None,
        };
        let mut ifaces = Vec::new();
        for i in iface_names {
            ifaces.push(self.class_id(&i)?);
        }
        let is_throwable = name == "java/lang/Throwable" || sup.map_or(false, |s| self.classes[s].is_throwable);
        let nfields = if is_throwable { THROWABLE_FIELDS } else { 0 };
        let is_interface = self.jdk.get(name).map_or(false, |c| c.is_interface);
        let id = self.classes.len();
        let rc: Rc<str> = name.into();
        self.classes.push(Rc::new(Class {
            name: rc.clone(),
            sup,
            ifaces,
            is_interface,
            file: None,
            methods: vec![],
            nfields,
            own_fields: vec![],
            defaults: vec![V::Null; nfields],
            statics: RefCell::new(vec![]),
            static_names: vec![],
            init: Cell::new(2),
            cache: RefCell::new(vec![]),
            is_throwable,
            elem,
        }));
        self.by_name.insert(rc, id);
        Ok(id)
    }

    fn link(&mut self, id: usize) -> Result<(), Ex> {
        let cf = self.classes[id].file.clone().unwrap();
        // Mark as linking to break cycles through interfaces.
        self.classes[id].init.set(0);
        let sup = match &cf.super_name {
            Some(s) => Some(self.class_id(s)?),
            None => None,
        };
        let mut ifaces = Vec::new();
        for i in &cf.interfaces {
            ifaces.push(self.class_id(i)?);
        }
        let (mut nfields, mut defaults, is_throwable) = match sup {
            Some(s) => {
                let sc = &self.classes[s];
                (sc.nfields, sc.defaults.clone(), sc.is_throwable)
            }
            None => (0, vec![], false),
        };
        let mut own_fields = Vec::new();
        let mut static_names = Vec::new();
        let mut statics = Vec::new();
        for (flags, name, desc) in &cf.fields {
            if flags & 0x0008 != 0 {
                static_names.push((name.clone(), statics.len()));
                statics.push(default_for(desc));
            } else {
                own_fields.push((name.clone(), nfields));
                defaults.push(default_for(desc));
                nfields += 1;
            }
        }
        let methods = cf
            .methods
            .iter()
            .map(|m| {
                let (code, max_locals, exc, lines) = match &m.code {
                    Some(c) => (c.code.clone(), c.max_locals, c.exc.clone(), c.lines.clone()),
                    None => (vec![], 0, vec![], vec![]),
                };
                Rc::new(Method {
                    name: m.name.clone(),
                    desc: m.desc.clone(),
                    flags: m.flags,
                    cls: id,
                    code,
                    max_locals,
                    exc,
                    lines,
                    nargs: count_params(&m.desc),
                    is_static: m.flags & 0x0008 != 0,
                    is_abstract: m.code.is_none(),
                })
            })
            .collect();
        let old = &self.classes[id];
        self.classes[id] = Rc::new(Class {
            name: old.name.clone(),
            sup,
            ifaces,
            is_interface: old.is_interface,
            file: Some(cf.clone()),
            methods,
            nfields,
            own_fields,
            defaults,
            statics: RefCell::new(statics),
            static_names,
            init: Cell::new(0),
            cache: RefCell::new(vec![Res::None; cf.cp.len()]),
            is_throwable,
            elem: None,
        });
        Ok(())
    }

    pub fn is_subclass(&self, c: usize, target: usize) -> bool {
        if c == target {
            return true;
        }
        let cls = &self.classes[c];
        if let Some(s) = cls.sup {
            if self.is_subclass(s, target) {
                return true;
            }
        }
        cls.ifaces.iter().any(|&i| self.is_subclass(i, target))
    }

    /// Is a value of class `c` assignable to the class/array named `target`?
    pub fn assignable(&mut self, c: usize, target: &str) -> Result<bool, Ex> {
        if target == "java/lang/Object" {
            return Ok(true);
        }
        let celem = self.classes[c].elem.clone();
        if let Some(te) = target.strip_prefix('[') {
            let Some(ce) = celem else { return Ok(false) };
            return self.desc_assignable(&ce, te);
        }
        if celem.is_some() {
            return Ok(false);
        }
        let t = self.class_id(target)?;
        Ok(self.is_subclass(c, t))
    }

    fn desc_assignable(&mut self, from: &str, to: &str) -> Result<bool, Ex> {
        if from == to {
            return Ok(true);
        }
        match (from.as_bytes()[0], to.as_bytes()[0]) {
            (b'L', b'L') => {
                let f = &from[1..from.len() - 1];
                let fid = self.class_id(f)?;
                self.assignable(fid, &to[1..to.len() - 1])
            }
            (b'[', b'L') => Ok(&to[1..to.len() - 1] == "java/lang/Object"),
            (b'[', b'[') => self.desc_assignable(&from[1..], &to[1..]),
            _ => Ok(false),
        }
    }

    pub fn ensure_init(&mut self, id: usize) -> Result<(), Ex> {
        let cls = self.classes[id].clone();
        if cls.init.get() != 0 {
            return Ok(());
        }
        cls.init.set(1);
        if let Some(s) = cls.sup {
            self.ensure_init(s)?;
        }
        if let Some(m) = cls.methods.iter().find(|m| &*m.name == "<clinit>").cloned() {
            self.invoke(m, vec![])?;
        }
        cls.init.set(2);
        Ok(())
    }

    pub fn find_method(&self, cls: usize, name: &str, desc: &str) -> Option<Rc<Method>> {
        let mut c = Some(cls);
        while let Some(id) = c {
            let k = &self.classes[id];
            if let Some(m) = k.methods.iter().find(|m| &*m.name == name && &*m.desc == desc) {
                return Some(m.clone());
            }
            c = k.sup;
        }
        None
    }

    /// Virtual dispatch from the runtime class `cls`.
    pub fn lookup_virtual(&self, cls: usize, name: &str, desc: &str) -> Target {
        let mut c = Some(cls);
        let mut key = String::new();
        while let Some(id) = c {
            let k = &self.classes[id];
            if k.file.is_some() {
                if let Some(m) = k
                    .methods
                    .iter()
                    .find(|m| &*m.name == name && &*m.desc == desc && !m.is_static)
                {
                    if !m.is_abstract {
                        return Target::Code(m.clone());
                    }
                }
            } else {
                key.clear();
                key.push_str(&k.name);
                key.push('.');
                key.push_str(name);
                key.push_str(desc);
                if let Some(&n) = self.native_map.get(&key) {
                    return Target::Native(n);
                }
            }
            c = k.sup;
        }
        Target::Abstract
    }

    /// Resolve a JDK native by walking up from `owner` (for invokestatic/special).
    pub fn lookup_native(&mut self, owner: &str, name: &str, desc: &str) -> Result<Option<usize>, Ex> {
        let mut c = Some(self.class_id(owner)?);
        while let Some(id) = c {
            let key = format!("{}.{}{}", self.classes[id].name, name, desc);
            if let Some(&n) = self.native_map.get(&key) {
                return Ok(Some(n));
            }
            c = self.classes[id].sup;
        }
        Ok(None)
    }

    // ---------- objects ----------
    pub fn new_obj(&mut self, cls: usize, data: Data) -> Ref {
        Rc::new(Obj {
            cls,
            data: RefCell::new(data),
            hash: Cell::new(0),
        })
    }

    pub fn identity_hash(&mut self, o: &Ref) -> i32 {
        let h = o.hash.get();
        if h != 0 {
            return h;
        }
        // xorshift: deterministic, javac-like looking hashes
        let mut x = self.hash_seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.hash_seed = x;
        let h = (x & 0x7fff_ffff) as i32 | 1;
        o.hash.set(h);
        h
    }

    pub fn class_name(&self, o: &Ref) -> String {
        self.classes[o.cls].name.replace('/', ".")
    }

    /// Create a throwable of a JDK class with a message and capture the trace.
    pub fn make_throwable(&mut self, cls: &str, msg: Option<&str>, here: Option<&Frame>) -> Ref {
        let id = match self.class_id(cls) {
            Ok(id) => id,
            Err(_) => self.class_id("java/lang/RuntimeException").ok().unwrap_or(0),
        };
        let msgv = match msg {
            Some(m) => self.jstr(m),
            None => V::Null,
        };
        let trace = self.capture_trace(here);
        let tv = self.jstr(&trace);
        self.new_obj(id, Data::Fields(vec![msgv, V::Null, tv]))
    }

    pub fn throw(&mut self, cls: &str, msg: Option<&str>) -> Ex {
        Ex::Throw(self.make_throwable(cls, msg, None))
    }

    pub fn capture_trace(&self, here: Option<&Frame>) -> String {
        let mut s = String::new();
        let mut add = |f: &Frame| {
            let cls = &self.classes[f.m.cls];
            if s.lines().count() >= 64 {
                return;
            }
            let src = cls
                .file
                .as_ref()
                .map(|c| c.source.clone())
                .unwrap_or_else(|| "Unknown".into());
            s.push_str(&format!(
                "\tat {}.{}({}:{})\n",
                cls.name.replace('/', "."),
                f.m.name,
                src,
                f.m.line_at(f.cur)
            ));
        };
        if let Some(f) = here {
            add(f);
        }
        for f in self.frames.iter().rev() {
            add(f);
        }
        s
    }

    pub fn jstr(&mut self, s: &str) -> V {
        let units: Rc<[u16]> = s.encode_utf16().collect::<Vec<u16>>().into();
        self.str_from(units)
    }

    pub fn str_from(&mut self, units: Rc<[u16]>) -> V {
        let id = self.string_class();
        V::R(self.new_obj(id, Data::Str(units)))
    }

    pub fn string_class(&mut self) -> usize {
        match self.by_name.get("java/lang/String") {
            Some(&id) => id,
            None => self.class_id("java/lang/String").ok().unwrap(),
        }
    }

    fn intern(&mut self, s: &str) -> V {
        let units: Rc<[u16]> = s.encode_utf16().collect::<Vec<u16>>().into();
        self.intern_units(units)
    }

    pub fn intern_units(&mut self, units: Rc<[u16]>) -> V {
        if let Some(r) = self.interned.get(&units) {
            return V::R(r.clone());
        }
        let v = self.str_from(units.clone());
        if let V::R(r) = &v {
            self.interned.insert(units, r.clone());
        }
        v
    }

    pub fn new_array(&mut self, desc: &str, len: i32) -> Result<V, Ex> {
        if len < 0 {
            return Err(self.throw("java/lang/NegativeArraySizeException", Some(&len.to_string())));
        }
        let n = len as usize;
        let id = self.class_id(desc)?;
        let data = match &desc[1..2] {
            "I" => Data::ArrI(vec![0; n]),
            "J" => Data::ArrJ(vec![0; n]),
            "D" => Data::ArrD(vec![0.0; n]),
            "Z" => Data::ArrZ(vec![0; n]),
            "C" => Data::ArrC(vec![0; n]),
            _ => Data::ArrA(vec![V::Null; n]),
        };
        Ok(V::R(self.new_obj(id, data)))
    }

    fn multi_array(&mut self, desc: &str, dims: &[i32]) -> Result<V, Ex> {
        let arr = self.new_array(desc, dims[0])?;
        if dims.len() > 1 {
            let mut items = Vec::with_capacity(dims[0] as usize);
            for _ in 0..dims[0] {
                items.push(self.multi_array(&desc[1..], &dims[1..])?);
            }
            if let V::R(r) = &arr {
                *r.data.borrow_mut() = Data::ArrA(items);
            }
        }
        Ok(arr)
    }

    // ---------- invocation ----------
    /// Run a method to completion (re-entrant; used by natives and for <clinit>).
    pub fn invoke(&mut self, m: Rc<Method>, args: Vec<V>) -> R {
        if self.frames.len() >= self.max_frames {
            return Err(self.throw("java/lang/StackOverflowError", None));
        }
        let base = self.frames.len();
        let f = self.make_frame(m, args);
        self.frames.push(f);
        self.run(base)
    }

    /// Call a virtual method on an object by name and descriptor.
    pub fn call_virtual(&mut self, recv: V, name: &str, desc: &str, mut args: Vec<V>) -> R {
        let Some(o) = recv.obj().cloned() else {
            return Err(self.throw("java/lang/NullPointerException", None));
        };
        match self.lookup_virtual(o.cls, name, desc) {
            Target::Code(m) => {
                args.insert(0, recv);
                self.invoke(m, args)
            }
            Target::Native(n) => {
                args.insert(0, recv);
                let f = self.natives[n].1;
                f(self, &args)
            }
            Target::Abstract => Err(self.throw(
                "java/lang/AbstractMethodError",
                Some(&format!("{}.{}", self.class_name(&o), name)),
            )),
        }
    }

    fn make_frame(&self, m: Rc<Method>, args: Vec<V>) -> Frame {
        let mut locals = vec![V::Null; m.max_locals.max(args.len() * 2)];
        let mut slot = 0;
        for a in args {
            let w = a.wide();
            locals[slot] = a;
            slot += if w { 2 } else { 1 };
        }
        Frame {
            m,
            pc: 0,
            cur: 0,
            locals,
            stack: Vec::with_capacity(8),
        }
    }

    fn run(&mut self, base: usize) -> R {
        self.run_depth += 1;
        let r = self.run_loop(base);
        self.run_depth -= 1;
        r
    }

    /// Call a native from the interpreter. If it pauses for input, the call is
    /// rolled back so that resuming re-executes the same instruction.
    fn call_native(&mut self, f: &mut Frame, ni: usize, nargs: usize) -> Result<(), Ex> {
        let args = f.stack.split_off(f.stack.len() - nargs);
        let func = self.natives[ni].1;
        // A stand-in frame so stack traces from inside the native include this method.
        self.frames.push(Frame {
            m: f.m.clone(),
            pc: 0,
            cur: f.cur,
            locals: vec![],
            stack: vec![],
        });
        let r = func(self, &args);
        self.frames.pop();
        match r {
            Ok(Some(v)) => {
                f.stack.push(v);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(Ex::NeedInput) => {
                f.stack.extend(args);
                f.pc = f.cur;
                Err(Ex::NeedInput)
            }
            Err(e) => Err(e),
        }
    }

    fn run_loop(&mut self, base: usize) -> R {
        loop {
            let mut f = self.frames.pop().unwrap();
            match self.exec(&mut f) {
                Ok(Flow::Call(callee)) => {
                    self.frames.push(f);
                    if self.frames.len() >= self.max_frames {
                        let e = self.make_throwable("java/lang/StackOverflowError", None, Some(&callee));
                        if let Err(x) = self.unwind(e, base) {
                            return Err(x);
                        }
                        continue;
                    }
                    self.frames.push(callee);
                }
                Ok(Flow::Return(v)) => {
                    drop(f);
                    if self.frames.len() == base {
                        return Ok(v);
                    }
                    if let Some(v) = v {
                        self.frames.last_mut().unwrap().stack.push(v);
                    }
                }
                Err(Ex::Throw(e)) => {
                    self.frames.push(f);
                    self.unwind(e, base)?;
                }
                Err(Ex::NeedInput) => {
                    // Keep every frame: `resume` continues from right here.
                    self.frames.push(f);
                    return Err(Ex::NeedInput);
                }
                Err(other) => {
                    self.frames.truncate(base);
                    return Err(other);
                }
            }
        }
    }

    /// Find a handler for `e` in frames above `base`; pops frames that have none.
    fn unwind(&mut self, e: Ref, base: usize) -> Result<(), Ex> {
        loop {
            if self.frames.len() <= base {
                return Err(Ex::Throw(e));
            }
            let f = self.frames.last().unwrap();
            let m = f.m.clone();
            let pc = f.cur;
            let mut handler = None;
            for h in &m.exc {
                if pc >= h[0] as usize && pc < h[1] as usize {
                    if h[3] == 0 {
                        handler = Some(h[2]);
                        break;
                    }
                    let cname = self.cp_class_name(m.cls, h[3]);
                    if self.assignable(e.cls, &cname)? {
                        handler = Some(h[2]);
                        break;
                    }
                }
            }
            match handler {
                Some(h) => {
                    let f = self.frames.last_mut().unwrap();
                    f.stack.clear();
                    f.stack.push(V::R(e));
                    f.pc = h as usize;
                    return Ok(());
                }
                None => {
                    self.frames.pop();
                }
            }
        }
    }

    // ---------- constant pool helpers ----------
    fn cf(&self, cls: usize) -> &ClassFile {
        self.classes[cls].file.as_ref().unwrap()
    }
    fn utf(&self, cls: usize, i: u16) -> Rc<str> {
        match &self.cf(cls).cp[i as usize] {
            Cp::Utf8(s) => s.clone(),
            _ => "".into(),
        }
    }
    pub fn cp_class_name(&self, cls: usize, i: u16) -> Rc<str> {
        match &self.cf(cls).cp[i as usize] {
            Cp::Class(u) => self.utf(cls, *u),
            _ => "".into(),
        }
    }
    fn cp_member(&self, cls: usize, i: u16) -> (Rc<str>, Rc<str>, Rc<str>) {
        match &self.cf(cls).cp[i as usize] {
            Cp::Member(c, nt) => {
                let owner = self.cp_class_name(cls, *c);
                match &self.cf(cls).cp[*nt as usize] {
                    Cp::NameType(n, d) => (owner, self.utf(cls, *n), self.utf(cls, *d)),
                    _ => (owner, "".into(), "".into()),
                }
            }
            _ => ("".into(), "".into(), "".into()),
        }
    }

    fn cached(&self, cls: usize, i: u16) -> Res {
        self.classes[cls].cache.borrow()[i as usize].clone()
    }
    fn set_cache(&self, cls: usize, i: u16, r: Res) {
        self.classes[cls].cache.borrow_mut()[i as usize] = r;
    }

    fn resolve_static_field(&mut self, cls: usize, i: u16) -> Result<Res, Ex> {
        let r = self.cached(cls, i);
        if !matches!(r, Res::None) {
            return Ok(r);
        }
        let (owner, name, _desc) = self.cp_member(cls, i);
        let oid = self.class_id(&owner)?;
        let r = if self.classes[oid].file.is_some() {
            // search the class, its supers and interfaces
            let mut stack = vec![oid];
            let mut found = None;
            while let Some(c) = stack.pop() {
                let k = self.classes[c].clone();
                if let Some((_, s)) = k.static_names.iter().find(|(n, _)| **n == *name) {
                    found = Some((c, *s));
                    break;
                }
                stack.extend(k.ifaces.iter().copied());
                if let Some(s) = k.sup {
                    stack.push(s);
                }
            }
            match found {
                Some((c, slot)) => {
                    self.ensure_init(c)?;
                    Res::Static { cls: c, slot }
                }
                None => return Err(Ex::Fatal(format!("NoSuchFieldError: {}", name))),
            }
        } else {
            Res::JdkStatic(self.jdk_static(&owner, &name)?)
        };
        self.set_cache(cls, i, r.clone());
        Ok(r)
    }

    fn jdk_static(&mut self, owner: &str, name: &str) -> Result<V, Ex> {
        Ok(match (owner, name) {
            ("java/lang/System", "out") => V::R(self.stream(1)),
            ("java/lang/System", "err") => V::R(self.stream(2)),
            ("java/lang/System", "in") => V::R(self.stream(0)),
            ("java/lang/Double", "MAX_VALUE") => V::D(f64::MAX),
            ("java/lang/Double", "MIN_VALUE") => V::D(f64::from_bits(1)),
            ("java/lang/Double", "POSITIVE_INFINITY") => V::D(f64::INFINITY),
            ("java/lang/Double", "NEGATIVE_INFINITY") => V::D(f64::NEG_INFINITY),
            ("java/lang/Double", "NaN") => V::D(f64::NAN),
            ("java/lang/Math", "PI") => V::D(std::f64::consts::PI),
            ("java/lang/Math", "E") => V::D(std::f64::consts::E),
            ("java/lang/Integer", "MAX_VALUE") => V::I(i32::MAX),
            ("java/lang/Integer", "MIN_VALUE") => V::I(i32::MIN),
            ("java/lang/Long", "MAX_VALUE") => V::L(i64::MAX),
            ("java/lang/Long", "MIN_VALUE") => V::L(i64::MIN),
            _ => return Err(Ex::Fatal(format!("unsupported static field {}.{}", owner, name))),
        })
    }

    pub fn stream(&mut self, fd: u32) -> Ref {
        let slot = match fd {
            1 => &self.stdout_obj,
            2 => &self.stderr_obj,
            _ => &self.stdin_obj,
        };
        if let Some(r) = slot {
            return r.clone();
        }
        let cls = if fd == 0 {
            "java/io/InputStream"
        } else {
            "java/io/PrintStream"
        };
        let id = self.class_id(cls).ok().unwrap();
        let r = self.new_obj(id, Data::Stream(fd));
        match fd {
            1 => self.stdout_obj = Some(r.clone()),
            2 => self.stderr_obj = Some(r.clone()),
            _ => self.stdin_obj = Some(r.clone()),
        }
        r
    }

    fn resolve_field(&mut self, cls: usize, i: u16) -> Result<usize, Ex> {
        if let Res::Field(s) = self.cached(cls, i) {
            return Ok(s);
        }
        let (owner, name, _) = self.cp_member(cls, i);
        let mut c = Some(self.class_id(&owner)?);
        while let Some(id) = c {
            if let Some((_, s)) = self.classes[id].own_fields.iter().find(|(n, _)| **n == *name) {
                let s = *s;
                self.set_cache(cls, i, Res::Field(s));
                return Ok(s);
            }
            c = self.classes[id].sup;
        }
        Err(Ex::Fatal(format!("NoSuchFieldError: {}", name)))
    }

    fn resolve_call(&mut self, cls: usize, i: u16, is_static: bool) -> Result<Res, Ex> {
        let r = self.cached(cls, i);
        if !matches!(r, Res::None) {
            return Ok(r);
        }
        let (owner, name, desc) = self.cp_member(cls, i);
        let oid = self.class_id(&owner)?;
        let nargs = count_params(&desc) + if is_static { 0 } else { 1 };
        let target = if self.classes[oid].file.is_some() {
            match self.find_method(oid, &name, &desc) {
                Some(m) => Target::Code(m),
                None => match self.lookup_native(&owner, &name, &desc)? {
                    Some(n) => Target::Native(n),
                    None => {
                        // inherited from a JDK superclass (e.g. super(...) into Throwable)
                        let mut c = self.classes[oid].sup;
                        let mut t = None;
                        while let Some(id) = c {
                            if self.classes[id].file.is_none() {
                                let nm = self.classes[id].name.clone();
                                t = self.lookup_native(&nm, &name, &desc)?;
                                break;
                            }
                            c = self.classes[id].sup;
                        }
                        match t {
                            Some(n) => Target::Native(n),
                            None => return Err(Ex::Fatal(format!("NoSuchMethodError: {}.{}{}", owner, name, desc))),
                        }
                    }
                },
            }
        } else {
            match self.lookup_native(&owner, &name, &desc)? {
                Some(n) => Target::Native(n),
                None => {
                    return Err(Ex::Fatal(format!(
                        "jolt's browser JVM does not implement {}.{}{} yet",
                        owner.replace('/', "."),
                        name,
                        desc
                    )))
                }
            }
        };
        let r = Res::Call {
            target,
            nargs,
            cls: oid,
        };
        self.set_cache(cls, i, r.clone());
        Ok(r)
    }

    fn resolve_virtual(&mut self, cls: usize, i: u16) -> Res {
        let r = self.cached(cls, i);
        if !matches!(r, Res::None) {
            return r;
        }
        let (_, name, desc) = self.cp_member(cls, i);
        let key: Rc<str> = format!("{}{}", name, desc).into();
        let r = Res::Virtual {
            key,
            nargs: count_params(&desc) + 1,
            name,
            desc,
        };
        self.set_cache(cls, i, r.clone());
        r
    }

    fn resolve_class(&mut self, cls: usize, i: u16) -> Result<usize, Ex> {
        if let Res::Class(c) = self.cached(cls, i) {
            return Ok(c);
        }
        let name = self.cp_class_name(cls, i);
        let c = self.class_id(&name)?;
        self.set_cache(cls, i, Res::Class(c));
        Ok(c)
    }

    fn ldc(&mut self, cls: usize, i: u16) -> Result<V, Ex> {
        if let Res::Val(v) = self.cached(cls, i) {
            return Ok(v);
        }
        let v = match self.cf(cls).cp[i as usize].clone() {
            Cp::Int(x) => V::I(x),
            Cp::Long(x) => V::L(x),
            Cp::Double(x) => V::D(x),
            Cp::Str(u) => {
                let s = self.utf(cls, u);
                self.intern(&s)
            }
            _ => return Err(Ex::Fatal("unsupported ldc constant".into())),
        };
        self.set_cache(cls, i, Res::Val(v.clone()));
        Ok(v)
    }

    // ---------- the interpreter ----------
    fn exec(&mut self, f: &mut Frame) -> Result<Flow, Ex> {
        let m = f.m.clone();
        let code = &m.code[..];
        let cls = m.cls;
        macro_rules! u1 {
            ($o:expr) => {
                code[f.pc + $o] as usize
            };
        }
        macro_rules! u2 {
            ($o:expr) => {
                ((code[f.pc + $o] as u16) << 8 | code[f.pc + $o + 1] as u16)
            };
        }
        macro_rules! s2 {
            ($o:expr) => {
                u2!($o) as i16 as isize
            };
        }
        macro_rules! pop {
            () => {
                f.stack.pop().unwrap()
            };
        }
        macro_rules! push {
            ($v:expr) => {
                f.stack.push($v)
            };
        }
        macro_rules! throw {
            ($c:expr, $m:expr) => {{
                let e = self.make_throwable($c, $m, Some(f));
                return Err(Ex::Throw(e));
            }};
        }
        macro_rules! npe {
            () => {
                throw!("java/lang/NullPointerException", None)
            };
        }
        macro_rules! jump {
            ($cond:expr) => {{
                if $cond {
                    f.pc = (f.pc as isize + s2!(1)) as usize;
                } else {
                    f.pc += 3;
                }
            }};
        }
        macro_rules! arr {
            ($v:expr) => {
                match $v {
                    V::R(r) => r,
                    _ => npe!(),
                }
            };
        }
        macro_rules! bounds {
            ($idx:expr, $len:expr) => {
                if $idx < 0 || $idx as usize >= $len {
                    let msg = format!("Index {} out of bounds for length {}", $idx, $len);
                    throw!("java/lang/ArrayIndexOutOfBoundsException", Some(&msg));
                }
            };
        }
        loop {
            f.cur = f.pc;
            self.steps += 1;
            let op = code[f.pc];
            match op {
                0 => f.pc += 1,
                1 => {
                    push!(V::Null);
                    f.pc += 1
                }
                2..=8 => {
                    push!(V::I(op as i32 - 3));
                    f.pc += 1
                }
                9 | 10 => {
                    push!(V::L(op as i64 - 9));
                    f.pc += 1
                }
                14 | 15 => {
                    push!(V::D((op - 14) as f64));
                    f.pc += 1
                }
                16 => {
                    push!(V::I(code[f.pc + 1] as i8 as i32));
                    f.pc += 2
                }
                17 => {
                    push!(V::I(s2!(1) as i32));
                    f.pc += 3
                }
                18 => {
                    let v = self.ldc(cls, u1!(1) as u16)?;
                    push!(v);
                    f.pc += 2
                }
                19 | 20 => {
                    let v = self.ldc(cls, u2!(1))?;
                    push!(v);
                    f.pc += 3
                }
                21..=25 => {
                    push!(f.locals[u1!(1)].clone());
                    f.pc += 2
                }
                26..=45 => {
                    push!(f.locals[((op - 26) % 4) as usize].clone());
                    f.pc += 1
                }
                46..=53 => {
                    let i = pop!().i();
                    let a = pop!();
                    let r = arr!(a);
                    let v = {
                        let d = r.data.borrow();
                        match &*d {
                            Data::ArrI(v) => {
                                bounds!(i, v.len());
                                V::I(v[i as usize])
                            }
                            Data::ArrJ(v) => {
                                bounds!(i, v.len());
                                V::L(v[i as usize])
                            }
                            Data::ArrD(v) => {
                                bounds!(i, v.len());
                                V::D(v[i as usize])
                            }
                            Data::ArrZ(v) => {
                                bounds!(i, v.len());
                                V::I(v[i as usize] as i32)
                            }
                            Data::ArrC(v) => {
                                bounds!(i, v.len());
                                V::I(v[i as usize] as i32)
                            }
                            Data::ArrA(v) => {
                                bounds!(i, v.len());
                                v[i as usize].clone()
                            }
                            _ => return Err(Ex::Fatal("array load on non-array".into())),
                        }
                    };
                    push!(v);
                    f.pc += 1
                }
                54..=58 => {
                    let v = pop!();
                    f.locals[u1!(1)] = v;
                    f.pc += 2
                }
                59..=78 => {
                    let v = pop!();
                    f.locals[((op - 59) % 4) as usize] = v;
                    f.pc += 1
                }
                79..=86 => {
                    let v = pop!();
                    let i = pop!().i();
                    let a = pop!();
                    let r = arr!(a);
                    if op == 83 {
                        // aastore: check the element type
                        if let V::R(vo) = &v {
                            let elem = self.classes[r.cls]
                                .elem
                                .clone()
                                .unwrap_or_else(|| "Ljava/lang/Object;".into());
                            if &*elem != "Ljava/lang/Object;" {
                                let target = if elem.starts_with('L') {
                                    elem[1..elem.len() - 1].to_string()
                                } else {
                                    elem.to_string()
                                };
                                if !self.assignable(vo.cls, &target)? {
                                    let n = self.class_name(vo);
                                    throw!("java/lang/ArrayStoreException", Some(&n));
                                }
                            }
                        }
                    }
                    let mut d = r.data.borrow_mut();
                    match &mut *d {
                        Data::ArrI(a) => {
                            bounds!(i, a.len());
                            a[i as usize] = v.i()
                        }
                        Data::ArrJ(a) => {
                            bounds!(i, a.len());
                            a[i as usize] = v.l()
                        }
                        Data::ArrD(a) => {
                            bounds!(i, a.len());
                            a[i as usize] = v.d()
                        }
                        Data::ArrZ(a) => {
                            bounds!(i, a.len());
                            a[i as usize] = v.i() as i8
                        }
                        Data::ArrC(a) => {
                            bounds!(i, a.len());
                            a[i as usize] = v.i() as u16
                        }
                        Data::ArrA(a) => {
                            bounds!(i, a.len());
                            a[i as usize] = v
                        }
                        _ => return Err(Ex::Fatal("array store on non-array".into())),
                    }
                    drop(d);
                    f.pc += 1
                }
                87 => {
                    pop!();
                    f.pc += 1
                }
                88 => {
                    let v = pop!();
                    if !v.wide() {
                        pop!();
                    }
                    f.pc += 1
                }
                89 => {
                    let v = f.stack.last().unwrap().clone();
                    push!(v);
                    f.pc += 1
                }
                90 => {
                    // dup_x1
                    let v1 = pop!();
                    let v2 = pop!();
                    push!(v1.clone());
                    push!(v2);
                    push!(v1);
                    f.pc += 1
                }
                91 => {
                    // dup_x2
                    let v1 = pop!();
                    let v2 = pop!();
                    if v2.wide() {
                        push!(v1.clone());
                        push!(v2);
                        push!(v1);
                    } else {
                        let v3 = pop!();
                        push!(v1.clone());
                        push!(v3);
                        push!(v2);
                        push!(v1);
                    }
                    f.pc += 1
                }
                92 => {
                    // dup2
                    let v1 = pop!();
                    if v1.wide() {
                        push!(v1.clone());
                        push!(v1);
                    } else {
                        let v2 = pop!();
                        push!(v2.clone());
                        push!(v1.clone());
                        push!(v2);
                        push!(v1);
                    }
                    f.pc += 1
                }
                93 => {
                    // dup2_x1
                    let v1 = pop!();
                    if v1.wide() {
                        let v2 = pop!();
                        push!(v1.clone());
                        push!(v2);
                        push!(v1);
                    } else {
                        let v2 = pop!();
                        let v3 = pop!();
                        push!(v2.clone());
                        push!(v1.clone());
                        push!(v3);
                        push!(v2);
                        push!(v1);
                    }
                    f.pc += 1
                }
                94 => {
                    // dup2_x2
                    let v1 = pop!();
                    if v1.wide() {
                        let v2 = pop!();
                        if v2.wide() {
                            push!(v1.clone());
                            push!(v2);
                            push!(v1);
                        } else {
                            let v3 = pop!();
                            push!(v1.clone());
                            push!(v3);
                            push!(v2);
                            push!(v1);
                        }
                    } else {
                        let v2 = pop!();
                        let v3 = pop!();
                        if v3.wide() {
                            push!(v2.clone());
                            push!(v1.clone());
                            push!(v3);
                            push!(v2);
                            push!(v1);
                        } else {
                            let v4 = pop!();
                            push!(v2.clone());
                            push!(v1.clone());
                            push!(v4);
                            push!(v3);
                            push!(v2);
                            push!(v1);
                        }
                    }
                    f.pc += 1
                }
                95 => {
                    let n = f.stack.len();
                    f.stack.swap(n - 1, n - 2);
                    f.pc += 1
                }
                96..=131 => {
                    let b = pop!();
                    let r = if matches!(op, 116..=119) {
                        // negation: unary
                        match op {
                            116 => V::I(b.i().wrapping_neg()),
                            117 => V::L(b.l().wrapping_neg()),
                            _ => V::D(-b.d()),
                        }
                    } else {
                        let a = pop!();
                        match op {
                            96 => V::I(a.i().wrapping_add(b.i())),
                            97 => V::L(a.l().wrapping_add(b.l())),
                            99 => V::D(a.d() + b.d()),
                            100 => V::I(a.i().wrapping_sub(b.i())),
                            101 => V::L(a.l().wrapping_sub(b.l())),
                            103 => V::D(a.d() - b.d()),
                            104 => V::I(a.i().wrapping_mul(b.i())),
                            105 => V::L(a.l().wrapping_mul(b.l())),
                            107 => V::D(a.d() * b.d()),
                            108 => {
                                if b.i() == 0 {
                                    throw!("java/lang/ArithmeticException", Some("/ by zero"));
                                }
                                V::I(a.i().wrapping_div(b.i()))
                            }
                            109 => {
                                if b.l() == 0 {
                                    throw!("java/lang/ArithmeticException", Some("/ by zero"));
                                }
                                V::L(a.l().wrapping_div(b.l()))
                            }
                            111 => V::D(a.d() / b.d()),
                            112 => {
                                if b.i() == 0 {
                                    throw!("java/lang/ArithmeticException", Some("/ by zero"));
                                }
                                V::I(a.i().wrapping_rem(b.i()))
                            }
                            113 => {
                                if b.l() == 0 {
                                    throw!("java/lang/ArithmeticException", Some("/ by zero"));
                                }
                                V::L(a.l().wrapping_rem(b.l()))
                            }
                            115 => V::D(a.d() % b.d()),
                            120 => V::I(a.i().wrapping_shl(b.i() as u32)),
                            121 => V::L(a.l().wrapping_shl(b.i() as u32)),
                            122 => V::I(a.i().wrapping_shr(b.i() as u32)),
                            123 => V::L(a.l().wrapping_shr(b.i() as u32)),
                            124 => V::I((a.i() as u32).wrapping_shr(b.i() as u32) as i32),
                            125 => V::L((a.l() as u64).wrapping_shr(b.i() as u32) as i64),
                            126 => V::I(a.i() & b.i()),
                            127 => V::L(a.l() & b.l()),
                            128 => V::I(a.i() | b.i()),
                            129 => V::L(a.l() | b.l()),
                            130 => V::I(a.i() ^ b.i()),
                            131 => V::L(a.l() ^ b.l()),
                            _ => return Err(Ex::Fatal(format!("unsupported opcode {}", op))),
                        }
                    };
                    push!(r);
                    f.pc += 1
                }
                132 => {
                    let i = u1!(1);
                    let c = code[f.pc + 2] as i8 as i32;
                    f.locals[i] = V::I(f.locals[i].i().wrapping_add(c));
                    f.pc += 3
                }
                133 => {
                    let v = pop!().i();
                    push!(V::L(v as i64));
                    f.pc += 1
                }
                135 => {
                    let v = pop!().i();
                    push!(V::D(v as f64));
                    f.pc += 1
                }
                136 => {
                    let v = pop!().l();
                    push!(V::I(v as i32));
                    f.pc += 1
                }
                138 => {
                    let v = pop!().l();
                    push!(V::D(v as f64));
                    f.pc += 1
                }
                142 => {
                    let v = pop!().d();
                    push!(V::I(v as i32));
                    f.pc += 1
                }
                143 => {
                    let v = pop!().d();
                    push!(V::L(v as i64));
                    f.pc += 1
                }
                145 => {
                    let v = pop!().i();
                    push!(V::I(v as i8 as i32));
                    f.pc += 1
                }
                146 => {
                    let v = pop!().i();
                    push!(V::I(v as u16 as i32));
                    f.pc += 1
                }
                147 => {
                    let v = pop!().i();
                    push!(V::I(v as i16 as i32));
                    f.pc += 1
                }
                148 => {
                    let b = pop!().l();
                    let a = pop!().l();
                    push!(V::I((a > b) as i32 - (a < b) as i32));
                    f.pc += 1
                }
                151 | 152 => {
                    let b = pop!().d();
                    let a = pop!().d();
                    let r = if a.is_nan() || b.is_nan() {
                        if op == 151 {
                            -1
                        } else {
                            1
                        }
                    } else {
                        (a > b) as i32 - (a < b) as i32
                    };
                    push!(V::I(r));
                    f.pc += 1
                }
                153..=158 => {
                    let v = pop!().i();
                    jump!(match op {
                        153 => v == 0,
                        154 => v != 0,
                        155 => v < 0,
                        156 => v >= 0,
                        157 => v > 0,
                        _ => v <= 0,
                    })
                }
                159..=164 => {
                    let b = pop!().i();
                    let a = pop!().i();
                    jump!(match op {
                        159 => a == b,
                        160 => a != b,
                        161 => a < b,
                        162 => a >= b,
                        163 => a > b,
                        _ => a <= b,
                    })
                }
                165 | 166 => {
                    let b = pop!();
                    let a = pop!();
                    jump!(a.same(&b) == (op == 165))
                }
                167 => jump!(true),
                170 | 171 => {
                    let key = pop!().i();
                    let base = f.pc;
                    let mut p = (base + 4) & !3;
                    let rd = |p: usize| i32::from_be_bytes([code[p], code[p + 1], code[p + 2], code[p + 3]]);
                    let default = rd(p);
                    p += 4;
                    let off = if op == 170 {
                        let lo = rd(p);
                        let hi = rd(p + 4);
                        p += 8;
                        if key >= lo && key <= hi {
                            rd(p + 4 * (key - lo) as usize)
                        } else {
                            default
                        }
                    } else {
                        let n = rd(p) as usize;
                        p += 4;
                        let mut off = default;
                        for k in 0..n {
                            if rd(p + 8 * k) == key {
                                off = rd(p + 8 * k + 4);
                                break;
                            }
                        }
                        off
                    };
                    f.pc = (base as isize + off as isize) as usize;
                }
                172..=176 => return Ok(Flow::Return(Some(pop!()))),
                177 => return Ok(Flow::Return(None)),
                178 | 179 => {
                    let r = self.resolve_static_field(cls, u2!(1))?;
                    match r {
                        Res::Static { cls: c, slot } => {
                            let k = self.classes[c].clone();
                            if op == 178 {
                                push!(k.statics.borrow()[slot].clone());
                            } else {
                                let v = pop!();
                                k.statics.borrow_mut()[slot] = v;
                            }
                        }
                        Res::JdkStatic(v) => {
                            if op == 178 {
                                push!(v)
                            } else {
                                return Err(Ex::Fatal("cannot assign a JDK static field".into()));
                            }
                        }
                        _ => unreachable!(),
                    }
                    f.pc += 3
                }
                180 => {
                    let slot = self.resolve_field(cls, u2!(1))?;
                    let o = pop!();
                    let r = arr!(o);
                    let v = match &*r.data.borrow() {
                        Data::Fields(fs) => fs[slot].clone(),
                        _ => return Err(Ex::Fatal("getfield on non-object".into())),
                    };
                    push!(v);
                    f.pc += 3
                }
                181 => {
                    let slot = self.resolve_field(cls, u2!(1))?;
                    let v = pop!();
                    let o = pop!();
                    let r = arr!(o);
                    match &mut *r.data.borrow_mut() {
                        Data::Fields(fs) => fs[slot] = v,
                        _ => return Err(Ex::Fatal("putfield on non-object".into())),
                    }
                    f.pc += 3
                }
                182 | 185 => {
                    let Res::Virtual { key, nargs, name, desc } = self.resolve_virtual(cls, u2!(1)) else {
                        unreachable!()
                    };
                    let n = f.stack.len();
                    let recv = f.stack[n - nargs].clone();
                    let Some(o) = recv.obj() else {
                        let msg = format!("Cannot invoke \"{}()\" because value is null", name);
                        throw!("java/lang/NullPointerException", Some(&msg));
                    };
                    let ck = (o.cls, Rc::as_ptr(&key) as *const u8 as usize);
                    let target = match self.vcache.get(&ck) {
                        Some(t) => t.clone(),
                        None => {
                            let t = self.lookup_virtual(o.cls, &name, &desc);
                            self.vcache.insert(ck, t.clone());
                            t
                        }
                    };
                    f.pc += if op == 185 { 5 } else { 3 };
                    match target {
                        Target::Code(callee) => {
                            let args = f.stack.split_off(n - nargs);
                            return Ok(Flow::Call(self.make_frame(callee, args)));
                        }
                        Target::Native(ni) => self.call_native(f, ni, nargs)?,
                        Target::Abstract => {
                            let msg = format!("{}.{}{}", self.class_name(o), name, desc);
                            throw!("java/lang/AbstractMethodError", Some(&msg));
                        }
                    }
                }
                183 | 184 => {
                    let r = self.resolve_call(cls, u2!(1), op == 184)?;
                    let Res::Call {
                        target,
                        nargs,
                        cls: owner,
                    } = r
                    else {
                        unreachable!()
                    };
                    if op == 184 {
                        self.ensure_init(owner)?;
                    }
                    let n = f.stack.len();
                    if op == 183 && matches!(f.stack[n - nargs], V::Null) {
                        npe!();
                    }
                    f.pc += 3;
                    match target {
                        Target::Code(callee) => {
                            let args = f.stack.split_off(n - nargs);
                            return Ok(Flow::Call(self.make_frame(callee, args)));
                        }
                        Target::Native(ni) => self.call_native(f, ni, nargs)?,
                        Target::Abstract => return Err(Ex::Fatal("abstract method".into())),
                    }
                }
                187 => {
                    let c = self.resolve_class(cls, u2!(1))?;
                    self.ensure_init(c)?;
                    let k = self.classes[c].clone();
                    let data = if k.file.is_some() || k.is_throwable {
                        let mut fields = k.defaults.clone();
                        if k.is_throwable {
                            let t = self.capture_trace(Some(f));
                            fields[2] = self.jstr(&t);
                        }
                        Data::Fields(fields)
                    } else if &*k.name == "java/lang/Object" {
                        Data::Plain
                    } else {
                        Data::Pending
                    };
                    let o = self.new_obj(c, data);
                    push!(V::R(o));
                    f.pc += 3
                }
                188 => {
                    let n = pop!().i();
                    let desc = match code[f.pc + 1] {
                        4 => "[Z",
                        5 => "[C",
                        7 => "[D",
                        10 => "[I",
                        11 => "[J",
                        _ => return Err(Ex::Fatal("unsupported newarray type".into())),
                    };
                    if n < 0 {
                        throw!("java/lang/NegativeArraySizeException", Some(&n.to_string()));
                    }
                    let a = self.new_array(desc, n)?;
                    push!(a);
                    f.pc += 2
                }
                189 => {
                    let n = pop!().i();
                    let elem = self.cp_class_name(cls, u2!(1));
                    let desc = if elem.starts_with('[') {
                        format!("[{}", elem)
                    } else {
                        format!("[L{};", elem)
                    };
                    if n < 0 {
                        throw!("java/lang/NegativeArraySizeException", Some(&n.to_string()));
                    }
                    let a = self.new_array(&desc, n)?;
                    push!(a);
                    f.pc += 3
                }
                190 => {
                    let a = pop!();
                    let r = arr!(a);
                    let n = match &*r.data.borrow() {
                        Data::ArrI(v) => v.len(),
                        Data::ArrJ(v) => v.len(),
                        Data::ArrD(v) => v.len(),
                        Data::ArrZ(v) => v.len(),
                        Data::ArrC(v) => v.len(),
                        Data::ArrA(v) => v.len(),
                        _ => 0,
                    };
                    push!(V::I(n as i32));
                    f.pc += 1
                }
                191 => {
                    let e = pop!();
                    match e {
                        V::R(r) => return Err(Ex::Throw(r)),
                        _ => npe!(),
                    }
                }
                192 | 193 => {
                    let target = self.cp_class_name(cls, u2!(1));
                    let v = pop!();
                    match &v {
                        V::Null => push!(if op == 192 { V::Null } else { V::I(0) }),
                        V::R(o) => {
                            let ok = self.assignable(o.cls, &target)?;
                            if op == 193 {
                                push!(V::I(ok as i32));
                            } else if ok {
                                push!(v.clone());
                            } else {
                                let from = self.class_name(o);
                                let to = target.replace('/', ".");
                                let where_ = |n: &str| {
                                    if n.starts_with("java.") {
                                        "module java.base of loader 'bootstrap'"
                                    } else {
                                        "unnamed module of loader 'app'"
                                    }
                                };
                                let tail = if where_(&from) == where_(&to) {
                                    format!("{} and {} are in {}", from, to, where_(&from))
                                } else {
                                    format!("{} is in {}; {} is in {}", from, where_(&from), to, where_(&to))
                                };
                                let msg = format!("class {} cannot be cast to class {} ({})", from, to, tail);
                                throw!("java/lang/ClassCastException", Some(&msg));
                            }
                        }
                        _ => return Err(Ex::Fatal("checkcast on primitive".into())),
                    }
                    f.pc += 3
                }
                196 => {
                    let wop = code[f.pc + 1];
                    let idx = u2!(2) as usize;
                    match wop {
                        21..=25 => {
                            push!(f.locals[idx].clone());
                            f.pc += 4
                        }
                        54..=58 => {
                            let v = pop!();
                            f.locals[idx] = v;
                            f.pc += 4
                        }
                        132 => {
                            let c = s2!(4) as i32;
                            f.locals[idx] = V::I(f.locals[idx].i().wrapping_add(c));
                            f.pc += 6
                        }
                        _ => return Err(Ex::Fatal("unsupported wide instruction".into())),
                    }
                }
                197 => {
                    let desc = self.cp_class_name(cls, u2!(1));
                    let n = code[f.pc + 3] as usize;
                    let dims: Vec<i32> = f.stack.split_off(f.stack.len() - n).iter().map(|v| v.i()).collect();
                    if let Some(neg) = dims.iter().find(|d| **d < 0) {
                        throw!("java/lang/NegativeArraySizeException", Some(&neg.to_string()));
                    }
                    let a = self.multi_array(&desc, &dims)?;
                    push!(a);
                    f.pc += 4
                }
                198 | 199 => {
                    let v = pop!();
                    jump!(matches!(v, V::Null) == (op == 198))
                }
                _ => {
                    return Err(Ex::Fatal(format!(
                        "unsupported opcode {} in {}.{}",
                        op, self.classes[cls].name, m.name
                    )))
                }
            }
        }
    }

    // ---------- entry points ----------
    /// Start `main_class.main(args)`.
    pub fn start_main(&mut self, main_class: &str, args: &[String]) -> RunOutcome {
        let internal = main_class.replace('.', "/");
        let r = self.run_main_inner(&internal, args);
        self.finish(r)
    }

    /// Continue a program that paused for input.
    pub fn resume(&mut self) -> RunOutcome {
        let r = self.run(0).map(|_| ());
        self.finish(r)
    }

    fn finish(&mut self, r: Result<(), Ex>) -> RunOutcome {
        let code = match r {
            Ok(()) => 0,
            Err(Ex::NeedInput) => {
                self.flush();
                return RunOutcome::NeedInput;
            }
            Err(Ex::Exit(c)) => c,
            Err(Ex::Throw(e)) => {
                let text = self.describe_uncaught(&e);
                self.emit(2, &text);
                1
            }
            Err(Ex::Fatal(msg)) => {
                self.emit(2, &format!("jolt-vm: {}\n", msg));
                1
            }
        };
        self.frames.clear();
        self.flush();
        RunOutcome::Exited(code)
    }

    fn run_main_inner(&mut self, internal: &str, args: &[String]) -> Result<(), Ex> {
        let id = self.class_id(internal)?;
        self.ensure_init(id)?;
        let m = self.classes[id]
            .methods
            .iter()
            .find(|m| &*m.name == "main" && &*m.desc == "([Ljava/lang/String;)V" && m.is_static)
            .cloned()
            .ok_or_else(|| Ex::Fatal(format!("no main method in {}", internal)))?;
        let arr = self.new_array("[Ljava/lang/String;", args.len() as i32)?;
        let vals: Vec<V> = args.iter().map(|a| self.jstr(a)).collect();
        if let V::R(r) = &arr {
            *r.data.borrow_mut() = Data::ArrA(vals);
        }
        self.main_started = true;
        self.invoke(m, vec![arr])?;
        Ok(())
    }

    pub fn describe_uncaught(&mut self, e: &Ref) -> String {
        let mut out = String::from("Exception in thread \"main\" ");
        let mut cur = Some(e.clone());
        let mut first = true;
        let mut depth = 0;
        while let Some(t) = cur {
            if !first {
                out.push_str("Caused by: ");
            }
            first = false;
            out.push_str(&crate::natives::throwable_to_string(self, &t));
            out.push('\n');
            let (trace, cause) = match &*t.data.borrow() {
                Data::Fields(fs) => (fs[2].clone(), fs[1].clone()),
                _ => (V::Null, V::Null),
            };
            if let Some(s) = crate::natives::rust_string(&trace) {
                out.push_str(&s);
            }
            depth += 1;
            cur = match cause {
                V::R(c) if !Rc::ptr_eq(&c, &t) && depth < 8 => Some(c),
                _ => None,
            };
        }
        out
    }
}
