//! Constant pool and class-file serialization.

use crate::fxhash::FxMap;

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Int(i32),
    Long(i64),
    Double(u64),
    Str(u16),
    Class(u16),
    NameType(u16, u16),
    Field(u16, u16),
    Method(u16, u16),
    IMethod(u16, u16),
}

/// Constant pool that serializes entries as they are added.
pub struct ConstPool {
    bytes: Vec<u8>,
    next: u32,
    map: FxMap<Key, u16>,
    utf8s: FxMap<String, u16>,
}

pub fn modified_utf8(s: &str, out: &mut Vec<u8>) {
    for c in s.encode_utf16() {
        match c {
            0x0001..=0x007f => out.push(c as u8),
            0x0000 | 0x0080..=0x07ff => {
                out.push(0xc0 | (c >> 6) as u8);
                out.push(0x80 | (c & 0x3f) as u8);
            }
            _ => {
                out.push(0xe0 | (c >> 12) as u8);
                out.push(0x80 | ((c >> 6) & 0x3f) as u8);
                out.push(0x80 | (c & 0x3f) as u8);
            }
        }
    }
}

impl ConstPool {
    pub fn new() -> Self {
        ConstPool {
            bytes: Vec::with_capacity(1024),
            next: 1,
            map: FxMap::default(),
            utf8s: FxMap::default(),
        }
    }

    pub fn count(&self) -> u32 {
        self.next
    }

    fn add(&mut self, key: Key, wide: bool, write: impl FnOnce(&mut Vec<u8>)) -> u16 {
        if let Some(&i) = self.map.get(&key) {
            return i;
        }
        let idx = self.next.min(u16::MAX as u32) as u16;
        write(&mut self.bytes);
        self.next += if wide { 2 } else { 1 };
        self.map.insert(key, idx);
        idx
    }

    pub fn utf8(&mut self, s: &str) -> u16 {
        if let Some(&i) = self.utf8s.get(s) {
            return i;
        }
        let idx = self.next.min(u16::MAX as u32) as u16;
        let b = &mut self.bytes;
        b.push(1);
        let len_at = b.len();
        b.extend_from_slice(&[0, 0]);
        modified_utf8(s, b);
        let len = (b.len() - len_at - 2).min(65535) as u16;
        b[len_at..len_at + 2].copy_from_slice(&len.to_be_bytes());
        self.next += 1;
        self.utf8s.insert(s.to_string(), idx);
        idx
    }

    pub fn int(&mut self, v: i32) -> u16 {
        self.add(Key::Int(v), false, |b| {
            b.push(3);
            b.extend_from_slice(&v.to_be_bytes());
        })
    }

    pub fn long(&mut self, v: i64) -> u16 {
        self.add(Key::Long(v), true, |b| {
            b.push(5);
            b.extend_from_slice(&v.to_be_bytes());
        })
    }

    pub fn double(&mut self, v: f64) -> u16 {
        let bits = v.to_bits();
        self.add(Key::Double(bits), true, |b| {
            b.push(6);
            b.extend_from_slice(&bits.to_be_bytes());
        })
    }

    pub fn string(&mut self, s: &str) -> u16 {
        let u = self.utf8(s);
        self.add(Key::Str(u), false, |b| {
            b.push(8);
            b.extend_from_slice(&u.to_be_bytes());
        })
    }

    pub fn class(&mut self, name: &str) -> u16 {
        let u = self.utf8(name);
        self.add(Key::Class(u), false, |b| {
            b.push(7);
            b.extend_from_slice(&u.to_be_bytes());
        })
    }

    fn name_type(&mut self, name: &str, desc: &str) -> u16 {
        let n = self.utf8(name);
        let d = self.utf8(desc);
        self.add(Key::NameType(n, d), false, |b| {
            b.push(12);
            b.extend_from_slice(&n.to_be_bytes());
            b.extend_from_slice(&d.to_be_bytes());
        })
    }

    fn member(&mut self, tag: u8, owner: &str, name: &str, desc: &str) -> u16 {
        let c = self.class(owner);
        let nt = self.name_type(name, desc);
        let key = match tag {
            9 => Key::Field(c, nt),
            10 => Key::Method(c, nt),
            _ => Key::IMethod(c, nt),
        };
        self.add(key, false, |b| {
            b.push(tag);
            b.extend_from_slice(&c.to_be_bytes());
            b.extend_from_slice(&nt.to_be_bytes());
        })
    }

    pub fn field(&mut self, owner: &str, name: &str, desc: &str) -> u16 {
        self.member(9, owner, name, desc)
    }
    pub fn method(&mut self, owner: &str, name: &str, desc: &str) -> u16 {
        self.member(10, owner, name, desc)
    }
    pub fn imethod(&mut self, owner: &str, name: &str, desc: &str) -> u16 {
        self.member(11, owner, name, desc)
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.next as u16).to_be_bytes());
        out.extend_from_slice(&self.bytes);
    }
}

pub struct MemberOut {
    pub flags: u16,
    pub name: u16,
    pub desc: u16,
    /// Pre-serialized attributes.
    pub attrs: Vec<Vec<u8>>,
}

pub struct ClassOut {
    pub flags: u16,
    pub this: u16,
    pub sup: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<MemberOut>,
    pub methods: Vec<MemberOut>,
    pub attrs: Vec<Vec<u8>>,
}

/// Class-file version 49 (Java 5): no StackMapTable frames are required, the
/// JVM verifies by type inference, and every modern JVM still loads it.
pub const MAJOR_VERSION: u16 = 49;

pub fn serialize(cp: &ConstPool, c: &ClassOut) -> Vec<u8> {
    let mut out = Vec::with_capacity(cp.bytes.len() + 512);
    out.extend_from_slice(&0xCAFEBABEu32.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&MAJOR_VERSION.to_be_bytes());
    cp.write(&mut out);
    out.extend_from_slice(&c.flags.to_be_bytes());
    out.extend_from_slice(&c.this.to_be_bytes());
    out.extend_from_slice(&c.sup.to_be_bytes());
    out.extend_from_slice(&(c.interfaces.len() as u16).to_be_bytes());
    for i in &c.interfaces {
        out.extend_from_slice(&i.to_be_bytes());
    }
    for list in [&c.fields, &c.methods] {
        out.extend_from_slice(&(list.len() as u16).to_be_bytes());
        for m in list.iter() {
            out.extend_from_slice(&m.flags.to_be_bytes());
            out.extend_from_slice(&m.name.to_be_bytes());
            out.extend_from_slice(&m.desc.to_be_bytes());
            out.extend_from_slice(&(m.attrs.len() as u16).to_be_bytes());
            for a in &m.attrs {
                out.extend_from_slice(a);
            }
        }
    }
    out.extend_from_slice(&(c.attrs.len() as u16).to_be_bytes());
    for a in &c.attrs {
        out.extend_from_slice(a);
    }
    out
}
