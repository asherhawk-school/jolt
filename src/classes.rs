//! Class table: user classes plus a built-in slice of the JDK.
//!
//! Instead of reading the JDK's module image (slow), jolt ships a compact
//! description of the commonly used parts of java.lang / java.util / java.io.

use crate::fxhash::FxMap;

use crate::types::{parse_descriptor, parse_method_descriptor, Type};

#[derive(Clone, Debug)]
pub enum Const {
    Int(i32),
    Long(i64),
    Double(f64),
    Str(String),
}

#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub name: String,
    pub ty: Type,
    pub is_static: bool,
    pub flags: u16,
    pub constant: Option<Const>,
}

#[derive(Clone, Debug)]
pub struct MethodInfo {
    pub name: String,
    pub params: Vec<Type>,
    pub ret: Type,
    pub is_static: bool,
    pub flags: u16,
    pub varargs: bool,
    pub desc: String,
}

#[derive(Clone, Debug, Default)]
pub struct ClassInfo {
    pub name: String,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    pub is_interface: bool,
    pub is_abstract: bool,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub user: bool,
    pub outer: Option<String>,
    /// simple name -> internal name of directly nested classes
    pub nested: Vec<(String, String)>,
    /// Part of a nest (outer or nested class): private members are emitted
    /// package-private so nestmates can reach them on old class-file versions.
    pub in_nest: bool,
}

#[derive(Default)]
pub struct ClassTable {
    pub classes: FxMap<String, ClassInfo>,
    /// simple name -> internal names of top-level classes (user first, then JDK)
    pub by_simple: FxMap<String, Vec<String>>,
}

pub const ACC_PUBLIC: u16 = 0x0001;
pub const ACC_PRIVATE: u16 = 0x0002;
pub const ACC_STATIC: u16 = 0x0008;
pub const ACC_ABSTRACT: u16 = 0x0400;
pub const ACC_VARARGS: u16 = 0x0080;

const JDK: &str = r#"
C java/lang/Object -
c ()V
m equals (Ljava/lang/Object;)Z
m hashCode ()I
m toString ()Ljava/lang/String;
I java/lang/CharSequence
m length ()I
m charAt (I)C
m isEmpty ()Z
I java/lang/Comparable
m compareTo (Ljava/lang/Object;)I
I java/lang/Runnable
m run ()V
C java/lang/String java/lang/Object java/lang/CharSequence java/lang/Comparable
c ()V
c (Ljava/lang/String;)V
c ([C)V
c ([CII)V
m length ()I
m charAt (I)C
m isEmpty ()Z
m isBlank ()Z
m substring (I)Ljava/lang/String;
m substring (II)Ljava/lang/String;
m indexOf (Ljava/lang/String;)I
m indexOf (I)I
m indexOf (Ljava/lang/String;I)I
m indexOf (II)I
m lastIndexOf (Ljava/lang/String;)I
m lastIndexOf (I)I
m contains (Ljava/lang/CharSequence;)Z
m startsWith (Ljava/lang/String;)Z
m endsWith (Ljava/lang/String;)Z
m equalsIgnoreCase (Ljava/lang/String;)Z
m compareTo (Ljava/lang/String;)I
m compareToIgnoreCase (Ljava/lang/String;)I
m toUpperCase ()Ljava/lang/String;
m toLowerCase ()Ljava/lang/String;
m trim ()Ljava/lang/String;
m strip ()Ljava/lang/String;
m concat (Ljava/lang/String;)Ljava/lang/String;
m replace (CC)Ljava/lang/String;
m replace (Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Ljava/lang/String;
m replaceAll (Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;
m matches (Ljava/lang/String;)Z
m split (Ljava/lang/String;)[Ljava/lang/String;
m toCharArray ()[C
m repeat (I)Ljava/lang/String;
m intern ()Ljava/lang/String;
s valueOf (I)Ljava/lang/String;
s valueOf (J)Ljava/lang/String;
s valueOf (D)Ljava/lang/String;
s valueOf (Z)Ljava/lang/String;
s valueOf (C)Ljava/lang/String;
s valueOf ([C)Ljava/lang/String;
s valueOf (Ljava/lang/Object;)Ljava/lang/String;
v format (Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;
v join (Ljava/lang/CharSequence;[Ljava/lang/CharSequence;)Ljava/lang/String;
C java/lang/StringBuilder java/lang/Object java/lang/CharSequence
c ()V
c (I)V
c (Ljava/lang/String;)V
m append (I)Ljava/lang/StringBuilder;
m append (J)Ljava/lang/StringBuilder;
m append (D)Ljava/lang/StringBuilder;
m append (Z)Ljava/lang/StringBuilder;
m append (C)Ljava/lang/StringBuilder;
m append ([C)Ljava/lang/StringBuilder;
m append (Ljava/lang/String;)Ljava/lang/StringBuilder;
m append (Ljava/lang/CharSequence;)Ljava/lang/StringBuilder;
m append (Ljava/lang/Object;)Ljava/lang/StringBuilder;
m insert (ILjava/lang/String;)Ljava/lang/StringBuilder;
m insert (IC)Ljava/lang/StringBuilder;
m insert (II)Ljava/lang/StringBuilder;
m reverse ()Ljava/lang/StringBuilder;
m length ()I
m charAt (I)C
m setCharAt (IC)V
m deleteCharAt (I)Ljava/lang/StringBuilder;
m delete (II)Ljava/lang/StringBuilder;
m setLength (I)V
m indexOf (Ljava/lang/String;)I
m replace (IILjava/lang/String;)Ljava/lang/StringBuilder;
m substring (I)Ljava/lang/String;
m substring (II)Ljava/lang/String;
C java/lang/Math java/lang/Object
k PI D 3.141592653589793
k E D 2.718281828459045
s abs (I)I
s abs (J)J
s abs (D)D
s max (II)I
s max (JJ)J
s max (DD)D
s min (II)I
s min (JJ)J
s min (DD)D
s sqrt (D)D
s cbrt (D)D
s pow (DD)D
s exp (D)D
s log (D)D
s log10 (D)D
s floor (D)D
s ceil (D)D
s rint (D)D
s round (D)J
s random ()D
s sin (D)D
s cos (D)D
s tan (D)D
s asin (D)D
s acos (D)D
s atan (D)D
s atan2 (DD)D
s hypot (DD)D
s signum (D)D
s toRadians (D)D
s toDegrees (D)D
s floorDiv (II)I
s floorDiv (JJ)J
s floorMod (II)I
s floorMod (JJ)J
s addExact (II)I
s addExact (JJ)J
s subtractExact (II)I
s subtractExact (JJ)J
s multiplyExact (II)I
s multiplyExact (JJ)J
s toIntExact (J)I
C java/lang/Number java/lang/Object
m intValue ()I
m longValue ()J
m doubleValue ()D
C java/lang/Integer java/lang/Number java/lang/Comparable
k MAX_VALUE I 2147483647
k MIN_VALUE I -2147483648
s parseInt (Ljava/lang/String;)I
s parseInt (Ljava/lang/String;I)I
s valueOf (I)Ljava/lang/Integer;
s toString (I)Ljava/lang/String;
s toString (II)Ljava/lang/String;
s toBinaryString (I)Ljava/lang/String;
s toHexString (I)Ljava/lang/String;
s toOctalString (I)Ljava/lang/String;
s bitCount (I)I
s compare (II)I
s signum (I)I
s reverse (I)I
s highestOneBit (I)I
s lowestOneBit (I)I
s numberOfLeadingZeros (I)I
s numberOfTrailingZeros (I)I
s max (II)I
s min (II)I
s sum (II)I
C java/lang/Long java/lang/Number java/lang/Comparable
k MAX_VALUE J 9223372036854775807
k MIN_VALUE J -9223372036854775808
s parseLong (Ljava/lang/String;)J
s valueOf (J)Ljava/lang/Long;
s toString (J)Ljava/lang/String;
s toBinaryString (J)Ljava/lang/String;
s toHexString (J)Ljava/lang/String;
s bitCount (J)I
s compare (JJ)I
s signum (J)I
s numberOfLeadingZeros (J)I
s numberOfTrailingZeros (J)I
s max (JJ)J
s min (JJ)J
s sum (JJ)J
C java/lang/Double java/lang/Number java/lang/Comparable
f MAX_VALUE D
f MIN_VALUE D
f POSITIVE_INFINITY D
f NEGATIVE_INFINITY D
f NaN D
s parseDouble (Ljava/lang/String;)D
s valueOf (D)Ljava/lang/Double;
s toString (D)Ljava/lang/String;
s isNaN (D)Z
s isInfinite (D)Z
s isFinite (D)Z
s compare (DD)I
s max (DD)D
s min (DD)D
s sum (DD)D
C java/lang/Character java/lang/Object java/lang/Comparable
k MAX_VALUE C 65535
k MIN_VALUE C 0
s valueOf (C)Ljava/lang/Character;
s isDigit (C)Z
s isLetter (C)Z
s isLetterOrDigit (C)Z
s isAlphabetic (I)Z
s isWhitespace (C)Z
s isUpperCase (C)Z
s isLowerCase (C)Z
s toUpperCase (C)C
s toLowerCase (C)C
s getNumericValue (C)I
s digit (CI)I
s forDigit (II)C
s toString (C)Ljava/lang/String;
s compare (CC)I
m charValue ()C
C java/lang/Boolean java/lang/Object java/lang/Comparable
s valueOf (Z)Ljava/lang/Boolean;
s parseBoolean (Ljava/lang/String;)Z
s toString (Z)Ljava/lang/String;
s compare (ZZ)I
m booleanValue ()Z
C java/lang/System java/lang/Object
f out Ljava/io/PrintStream;
f err Ljava/io/PrintStream;
f in Ljava/io/InputStream;
s currentTimeMillis ()J
s nanoTime ()J
s exit (I)V
s arraycopy (Ljava/lang/Object;ILjava/lang/Object;II)V
s getProperty (Ljava/lang/String;)Ljava/lang/String;
s getenv (Ljava/lang/String;)Ljava/lang/String;
s lineSeparator ()Ljava/lang/String;
s identityHashCode (Ljava/lang/Object;)I
s gc ()V
C java/lang/Thread java/lang/Object java/lang/Runnable
s sleep (J)V
C java/io/InputStream java/lang/Object
m read ()I
C java/io/PrintStream java/lang/Object
m println ()V
m println (Z)V
m println (C)V
m println (I)V
m println (J)V
m println (D)V
m println ([C)V
m println (Ljava/lang/String;)V
m println (Ljava/lang/Object;)V
m print (Z)V
m print (C)V
m print (I)V
m print (J)V
m print (D)V
m print ([C)V
m print (Ljava/lang/String;)V
m print (Ljava/lang/Object;)V
w printf (Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintStream;
w format (Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintStream;
m flush ()V
m write (I)V
C java/util/Arrays java/lang/Object
s toString ([I)Ljava/lang/String;
s toString ([J)Ljava/lang/String;
s toString ([D)Ljava/lang/String;
s toString ([C)Ljava/lang/String;
s toString ([Z)Ljava/lang/String;
s toString ([Ljava/lang/Object;)Ljava/lang/String;
s deepToString ([Ljava/lang/Object;)Ljava/lang/String;
s sort ([I)V
s sort ([J)V
s sort ([D)V
s sort ([C)V
s sort ([Ljava/lang/Object;)V
s sort ([III)V
s sort ([JII)V
s sort ([DII)V
s fill ([II)V
s fill ([JJ)V
s fill ([DD)V
s fill ([CC)V
s fill ([ZZ)V
s fill ([Ljava/lang/Object;Ljava/lang/Object;)V
s copyOf ([II)[I
s copyOf ([JI)[J
s copyOf ([DI)[D
s copyOf ([CI)[C
s copyOf ([ZI)[Z
s copyOfRange ([III)[I
s copyOfRange ([JII)[J
s copyOfRange ([DII)[D
s copyOfRange ([CII)[C
s equals ([I[I)Z
s equals ([J[J)Z
s equals ([D[D)Z
s equals ([C[C)Z
s equals ([Z[Z)Z
s equals ([Ljava/lang/Object;[Ljava/lang/Object;)Z
s hashCode ([I)I
s binarySearch ([II)I
s binarySearch ([JJ)I
s binarySearch ([DD)I
s binarySearch ([CC)I
C java/util/Scanner java/lang/Object
c (Ljava/io/InputStream;)V
c (Ljava/lang/String;)V
m nextInt ()I
m nextLong ()J
m nextDouble ()D
m nextBoolean ()Z
m next ()Ljava/lang/String;
m nextLine ()Ljava/lang/String;
m hasNext ()Z
m hasNextInt ()Z
m hasNextLong ()Z
m hasNextDouble ()Z
m hasNextLine ()Z
m close ()V
C java/util/Random java/lang/Object
c ()V
c (J)V
m nextInt ()I
m nextInt (I)I
m nextInt (II)I
m nextLong ()J
m nextDouble ()D
m nextBoolean ()Z
m nextGaussian ()D
m setSeed (J)V
C java/lang/Throwable java/lang/Object
c ()V
c (Ljava/lang/String;)V
c (Ljava/lang/String;Ljava/lang/Throwable;)V
c (Ljava/lang/Throwable;)V
m getMessage ()Ljava/lang/String;
m getLocalizedMessage ()Ljava/lang/String;
m getCause ()Ljava/lang/Throwable;
m printStackTrace ()V
X java/lang/Exception java/lang/Throwable
X java/lang/Error java/lang/Throwable
X java/lang/RuntimeException java/lang/Exception
X java/lang/IllegalArgumentException java/lang/RuntimeException
X java/lang/IllegalStateException java/lang/RuntimeException
X java/lang/UnsupportedOperationException java/lang/RuntimeException
E java/lang/ArithmeticException java/lang/RuntimeException
E java/lang/NullPointerException java/lang/RuntimeException
E java/lang/ClassCastException java/lang/RuntimeException
E java/lang/NegativeArraySizeException java/lang/RuntimeException
E java/lang/IndexOutOfBoundsException java/lang/RuntimeException
E java/lang/ArrayIndexOutOfBoundsException java/lang/IndexOutOfBoundsException
E java/lang/StringIndexOutOfBoundsException java/lang/IndexOutOfBoundsException
E java/lang/NumberFormatException java/lang/IllegalArgumentException
E java/lang/ArrayStoreException java/lang/RuntimeException
E java/util/NoSuchElementException java/lang/RuntimeException
E java/util/InputMismatchException java/util/NoSuchElementException
E java/lang/InterruptedException java/lang/Exception
E java/lang/CloneNotSupportedException java/lang/Exception
X java/io/IOException java/lang/Exception
E java/lang/StackOverflowError java/lang/Error
E java/lang/OutOfMemoryError java/lang/Error
E java/lang/AssertionError java/lang/Error
"#;

fn simple_name(internal: &str) -> &str {
    internal.rsplit('/').next().unwrap_or(internal)
}

impl ClassTable {
    pub fn with_jdk() -> ClassTable {
        let mut t = ClassTable::default();
        let mut cur: Option<ClassInfo> = None;
        let flush = |t: &mut ClassTable, c: Option<ClassInfo>| {
            if let Some(c) = c {
                t.by_simple
                    .entry(simple_name(&c.name).to_string())
                    .or_default()
                    .push(c.name.clone());
                t.classes.insert(c.name.clone(), c);
            }
        };
        for line in JDK.lines() {
            let mut parts = line.split_whitespace();
            let Some(kind) = parts.next() else { continue };
            match kind {
                "C" | "I" | "E" | "X" => {
                    flush(&mut t, cur.take());
                    let name = parts.next().unwrap().to_string();
                    let mut c = ClassInfo {
                        name,
                        is_interface: kind == "I",
                        ..Default::default()
                    };
                    if kind == "I" {
                        c.super_name = Some("java/lang/Object".into());
                    } else {
                        let sup = parts.next().unwrap();
                        if sup != "-" {
                            c.super_name = Some(sup.to_string());
                        }
                    }
                    c.interfaces = parts.map(|s| s.to_string()).collect();
                    if kind == "E" || kind == "X" {
                        let mut ctors = vec!["()V", "(Ljava/lang/String;)V"];
                        if kind == "X" {
                            ctors.push("(Ljava/lang/String;Ljava/lang/Throwable;)V");
                            ctors.push("(Ljava/lang/Throwable;)V");
                        }
                        for d in ctors {
                            c.methods.push(method("<init>", d, false, false));
                        }
                    }
                    cur = Some(c);
                }
                _ => {
                    let c = cur.as_mut().unwrap();
                    match kind {
                        "c" => c.methods.push(method("<init>", parts.next().unwrap(), false, false)),
                        "m" | "s" | "v" | "w" => {
                            let name = parts.next().unwrap();
                            let desc = parts.next().unwrap();
                            c.methods.push(method(
                                name,
                                desc,
                                kind == "s" || kind == "v",
                                kind == "v" || kind == "w",
                            ));
                        }
                        "f" | "k" => {
                            let name = parts.next().unwrap().to_string();
                            let desc = parts.next().unwrap();
                            let ty = parse_descriptor(desc.as_bytes(), &mut 0).unwrap();
                            let constant = if kind == "k" {
                                let v = parts.next().unwrap();
                                Some(match ty {
                                    Type::Int | Type::Char => Const::Int(v.parse().unwrap()),
                                    Type::Long => Const::Long(v.parse().unwrap()),
                                    Type::Double => Const::Double(v.parse().unwrap()),
                                    _ => unreachable!(),
                                })
                            } else {
                                None
                            };
                            c.fields.push(FieldInfo {
                                name,
                                ty,
                                is_static: true,
                                flags: ACC_PUBLIC | ACC_STATIC,
                                constant,
                            });
                        }
                        _ => panic!("bad JDK table line: {}", line),
                    }
                }
            }
        }
        flush(&mut t, cur.take());
        t
    }

    pub fn get(&self, name: &str) -> Option<&ClassInfo> {
        self.classes.get(name)
    }

    /// Is class `sub` the same as or a subtype of `sup`?
    pub fn is_subclass(&self, sub: &str, sup: &str) -> bool {
        if sub == sup || sup == "java/lang/Object" {
            return true;
        }
        let mut stack = vec![sub];
        let mut guard = 0;
        while let Some(c) = stack.pop() {
            guard += 1;
            if guard > 10_000 {
                return false;
            }
            if c == sup {
                return true;
            }
            if let Some(ci) = self.classes.get(c) {
                if let Some(s) = &ci.super_name {
                    stack.push(s);
                }
                for i in &ci.interfaces {
                    stack.push(i);
                }
            }
        }
        false
    }

    /// Reference widening: can a value of type `from` be assigned to `to`?
    pub fn ref_assignable(&self, from: &Type, to: &Type) -> bool {
        match (from, to) {
            (Type::Null, t) => t.is_reference(),
            (_, Type::Class(o)) if &**o == "java/lang/Object" => from.is_reference(),
            (Type::Class(a), Type::Class(b)) => self.is_subclass(a, b),
            (Type::Array(_), Type::Class(_)) => false,
            (Type::Array(a), Type::Array(b)) => {
                if a.is_primitive() || b.is_primitive() {
                    a == b
                } else {
                    self.ref_assignable(a, b)
                }
            }
            _ => false,
        }
    }

    /// All methods named `name` visible on class `cls`, most-derived first,
    /// with overridden duplicates removed. Returns (declaring class, method).
    pub fn find_methods<'s>(&'s self, cls: &str, name: &str) -> Vec<(&'s str, &'s MethodInfo)> {
        let mut out: Vec<(&str, &MethodInfo)> = Vec::new();
        let Some(start) = self.classes.get(cls) else { return out };
        let mut ifaces: Vec<&str> = Vec::new();
        let mut cur = Some(start);
        while let Some(ci) = cur {
            for m in &ci.methods {
                if m.name == name && !out.iter().any(|(_, o)| o.desc == m.desc) {
                    out.push((&ci.name, m));
                }
            }
            if name == "<init>" {
                return out;
            }
            ifaces.extend(ci.interfaces.iter().map(|s| s.as_str()));
            cur = ci.super_name.as_deref().and_then(|s| self.classes.get(s));
        }
        let mut seen: Vec<&str> = Vec::new();
        while let Some(i) = ifaces.pop() {
            if seen.contains(&i) {
                continue;
            }
            seen.push(i);
            if let Some(ci) = self.classes.get(i) {
                for m in &ci.methods {
                    if m.name == name && !m.is_static && !out.iter().any(|(_, o)| o.desc == m.desc) {
                        out.push((&ci.name, m));
                    }
                }
                ifaces.extend(ci.interfaces.iter().map(|s| s.as_str()));
            }
        }
        if start.is_interface {
            if let Some(ci) = self.classes.get("java/lang/Object") {
                for m in &ci.methods {
                    if m.name == name && !out.iter().any(|(_, o)| o.desc == m.desc) {
                        out.push((&ci.name, m));
                    }
                }
            }
        }
        out
    }

    /// Find a field by walking up the class hierarchy (and interfaces).
    pub fn find_field(&self, cls: &str, name: &str) -> Option<(String, &FieldInfo)> {
        let mut stack = vec![cls.to_string()];
        let mut guard = 0;
        while let Some(c) = stack.pop() {
            guard += 1;
            if guard > 10_000 {
                return None;
            }
            let Some(ci) = self.classes.get(&c) else { continue };
            if let Some(f) = ci.fields.iter().find(|f| f.name == name) {
                return Some((c, f));
            }
            for i in ci.interfaces.iter().rev() {
                stack.push(i.clone());
            }
            if let Some(s) = &ci.super_name {
                stack.push(s.clone());
            }
        }
        None
    }
}

fn method(name: &str, desc: &str, is_static: bool, varargs: bool) -> MethodInfo {
    let (params, ret) = parse_method_descriptor(desc).unwrap_or_else(|| panic!("bad descriptor {}", desc));
    MethodInfo {
        name: name.to_string(),
        params,
        ret,
        is_static,
        flags: ACC_PUBLIC | if is_static { ACC_STATIC } else { 0 } | if varargs { ACC_VARARGS } else { 0 },
        varargs,
        desc: desc.to_string(),
    }
}
