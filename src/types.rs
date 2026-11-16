//! Java types as seen by the compiler, plus JVM descriptor helpers.

use std::fmt;
use std::sync::{Arc, OnceLock};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Void,
    Boolean,
    Char,
    Int,
    Long,
    Double,
    /// The type of the `null` literal.
    Null,
    /// A class or interface, by JVM internal name (`java/lang/String`).
    /// Shared so that cloning a type never allocates.
    Class(Arc<str>),
    Array(Box<Type>),
}

pub const STRING: &str = "java/lang/String";
pub const OBJECT: &str = "java/lang/Object";

impl Type {
    pub fn class(name: &str) -> Type {
        match name {
            STRING => Type::string(),
            OBJECT => Type::object(),
            _ => Type::Class(name.into()),
        }
    }
    pub fn string() -> Type {
        static S: OnceLock<Arc<str>> = OnceLock::new();
        Type::Class(S.get_or_init(|| STRING.into()).clone())
    }
    pub fn object() -> Type {
        static O: OnceLock<Arc<str>> = OnceLock::new();
        Type::Class(O.get_or_init(|| OBJECT.into()).clone())
    }
    pub fn array_of(t: Type) -> Type {
        Type::Array(Box::new(t))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Type::Class(n) if &**n == STRING)
    }
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Char | Type::Int | Type::Long | Type::Double)
    }
    pub fn is_integral(&self) -> bool {
        matches!(self, Type::Char | Type::Int | Type::Long)
    }
    pub fn is_primitive(&self) -> bool {
        matches!(self, Type::Boolean | Type::Char | Type::Int | Type::Long | Type::Double)
    }
    pub fn is_reference(&self) -> bool {
        matches!(self, Type::Null | Type::Class(_) | Type::Array(_))
    }
    /// Number of JVM stack/local slots a value of this type occupies.
    pub fn slots(&self) -> i32 {
        match self {
            Type::Void => 0,
            Type::Long | Type::Double => 2,
            _ => 1,
        }
    }

    pub fn descriptor(&self) -> String {
        let mut s = String::new();
        self.write_descriptor(&mut s);
        s
    }

    pub fn write_descriptor(&self, out: &mut String) {
        match self {
            Type::Void => out.push('V'),
            Type::Boolean => out.push('Z'),
            Type::Char => out.push('C'),
            Type::Int => out.push('I'),
            Type::Long => out.push('J'),
            Type::Double => out.push('D'),
            Type::Null => out.push_str("Ljava/lang/Object;"),
            Type::Class(n) => {
                out.push('L');
                out.push_str(n);
                out.push(';');
            }
            Type::Array(e) => {
                out.push('[');
                e.write_descriptor(out);
            }
        }
    }

    /// The name used in a CONSTANT_Class entry: internal name for classes,
    /// descriptor for arrays.
    pub fn class_ref_name(&self) -> String {
        match self {
            Type::Class(n) => n.to_string(),
            _ => self.descriptor(),
        }
    }
}

pub fn method_descriptor(params: &[Type], ret: &Type) -> String {
    let mut s = String::from("(");
    for p in params {
        p.write_descriptor(&mut s);
    }
    s.push(')');
    ret.write_descriptor(&mut s);
    s
}

/// Parse a single field descriptor starting at `s[*i]`.
pub fn parse_descriptor(s: &[u8], i: &mut usize) -> Option<Type> {
    let c = *s.get(*i)?;
    *i += 1;
    Some(match c {
        b'V' => Type::Void,
        b'Z' => Type::Boolean,
        b'C' => Type::Char,
        b'I' => Type::Int,
        b'J' => Type::Long,
        b'D' => Type::Double,
        b'[' => Type::Array(Box::new(parse_descriptor(s, i)?)),
        b'L' => {
            let start = *i;
            while *s.get(*i)? != b';' {
                *i += 1;
            }
            let name = std::str::from_utf8(&s[start..*i]).ok()?;
            *i += 1;
            Type::class(name)
        }
        _ => return None,
    })
}

/// Parse `(params)ret`.
pub fn parse_method_descriptor(desc: &str) -> Option<(Vec<Type>, Type)> {
    let b = desc.as_bytes();
    if b.first() != Some(&b'(') {
        return None;
    }
    let mut i = 1;
    let mut params = Vec::new();
    while *b.get(i)? != b')' {
        params.push(parse_descriptor(b, &mut i)?);
    }
    i += 1;
    let ret = parse_descriptor(b, &mut i)?;
    Some((params, ret))
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Void => write!(f, "void"),
            Type::Boolean => write!(f, "boolean"),
            Type::Char => write!(f, "char"),
            Type::Int => write!(f, "int"),
            Type::Long => write!(f, "long"),
            Type::Double => write!(f, "double"),
            Type::Null => write!(f, "<null>"),
            Type::Class(n) => {
                let simple = n.rsplit('/').next().unwrap_or(n);
                write!(f, "{}", simple.replace('$', "."))
            }
            Type::Array(e) => write!(f, "{}[]", e),
        }
    }
}
