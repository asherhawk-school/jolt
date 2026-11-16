//! Semantic analysis: name resolution, type checking, overload resolution,
//! constant folding and reachability. Produces the typed IR in `tir`.

use crate::ast::modifier as m;
use crate::ast::*;
use crate::classes::*;
use crate::declare::{self, lookup_class, top_level};
use crate::diag::{err, Diag, Pos, Result};
use crate::tir::*;
use crate::types::{Type, OBJECT, STRING};

pub struct Checker<'a> {
    t: &'a ClassTable,
    cls: &'a ClassInfo,
    this_ty: Type,
    file: usize,
    is_static: bool,
    ret: Type,
    scopes: Vec<Vec<(String, u16, Type)>>,
    next_slot: u32,
    max_slot: u32,
    targets: Vec<Target>,
    next_id: u32,
    breaks: Vec<bool>,
    continues: Vec<bool>,
    pending_label: Option<String>,
}

struct Target {
    id: u32,
    label: Option<String>,
    is_loop: bool,
    is_switch: bool,
}

enum R {
    V(TE),
    C(String),
    P(String),
}

fn te(kind: TK, ty: Type) -> TE {
    TE { kind, ty }
}

pub fn const_bool(e: &TE) -> Option<bool> {
    match (&e.kind, &e.ty) {
        (TK::Int(v), Type::Boolean) => Some(*v != 0),
        _ => None,
    }
}

fn is_const(e: &TE) -> bool {
    matches!(e.kind, TK::Int(_) | TK::Long(_) | TK::Double(_) | TK::Str(_))
}

fn widening(from: &Type, to: &Type) -> bool {
    matches!(
        (from, to),
        (Type::Char, Type::Int | Type::Long | Type::Double)
            | (Type::Int, Type::Long | Type::Double)
            | (Type::Long, Type::Double)
    )
}

fn promote1(t: &Type) -> Type {
    if *t == Type::Char {
        Type::Int
    } else {
        t.clone()
    }
}

fn promote2(a: &Type, b: &Type) -> Option<Type> {
    if !a.is_numeric() || !b.is_numeric() {
        return None;
    }
    Some(if *a == Type::Double || *b == Type::Double {
        Type::Double
    } else if *a == Type::Long || *b == Type::Long {
        Type::Long
    } else {
        Type::Int
    })
}

fn wrapper(t: &Type) -> Option<(&'static str, &'static str)> {
    Some(match t {
        Type::Int => ("java/lang/Integer", "(I)Ljava/lang/Integer;"),
        Type::Long => ("java/lang/Long", "(J)Ljava/lang/Long;"),
        Type::Double => ("java/lang/Double", "(D)Ljava/lang/Double;"),
        Type::Char => ("java/lang/Character", "(C)Ljava/lang/Character;"),
        Type::Boolean => ("java/lang/Boolean", "(Z)Ljava/lang/Boolean;"),
        _ => return None,
    })
}

/// Convert a constant to another primitive type (Java semantics).
fn conv_const(k: &TK, to: &Type) -> Option<TK> {
    Some(match (k, to) {
        (TK::Int(v), Type::Int) => TK::Int(*v),
        (TK::Int(v), Type::Char) => TK::Int(*v as u16 as i32),
        (TK::Int(v), Type::Long) => TK::Long(*v as i64),
        (TK::Int(v), Type::Double) => TK::Double(*v as f64),
        (TK::Long(v), Type::Int) => TK::Int(*v as i32),
        (TK::Long(v), Type::Char) => TK::Int(*v as u16 as i32),
        (TK::Long(v), Type::Long) => TK::Long(*v),
        (TK::Long(v), Type::Double) => TK::Double(*v as f64),
        (TK::Double(v), Type::Int) => TK::Int(*v as i32),
        (TK::Double(v), Type::Char) => TK::Int(*v as i32 as u16 as i32),
        (TK::Double(v), Type::Long) => TK::Long(*v as i64),
        (TK::Double(v), Type::Double) => TK::Double(*v),
        _ => return None,
    })
}

fn convert(e: TE, to: &Type) -> TE {
    if e.ty == *to {
        return e;
    }
    if let Some(k) = conv_const(&e.kind, to) {
        return te(k, to.clone());
    }
    te(TK::Convert(Box::new(e)), to.clone())
}

fn fold_arith(op: BinOp, ty: &Type, l: &TK, r: &TK) -> Option<TK> {
    use BinOp::*;
    match (ty, l, r) {
        (Type::Int | Type::Boolean, TK::Int(a), TK::Int(b)) => {
            let (a, b) = (*a, *b);
            Some(TK::Int(match op {
                Add => a.wrapping_add(b),
                Sub => a.wrapping_sub(b),
                Mul => a.wrapping_mul(b),
                Div if b != 0 => a.wrapping_div(b),
                Rem if b != 0 => a.wrapping_rem(b),
                Shl => a.wrapping_shl(b as u32),
                Shr => a.wrapping_shr(b as u32),
                UShr => (a as u32).wrapping_shr(b as u32) as i32,
                BitAnd => a & b,
                BitOr => a | b,
                BitXor => a ^ b,
                _ => return None,
            }))
        }
        (Type::Long, TK::Long(a), TK::Int(b)) if matches!(op, Shl | Shr | UShr) => {
            let (a, b) = (*a, *b as u32);
            Some(TK::Long(match op {
                Shl => a.wrapping_shl(b),
                Shr => a.wrapping_shr(b),
                _ => (a as u64).wrapping_shr(b) as i64,
            }))
        }
        (Type::Long, TK::Long(a), TK::Long(b)) => {
            let (a, b) = (*a, *b);
            Some(TK::Long(match op {
                Add => a.wrapping_add(b),
                Sub => a.wrapping_sub(b),
                Mul => a.wrapping_mul(b),
                Div if b != 0 => a.wrapping_div(b),
                Rem if b != 0 => a.wrapping_rem(b),
                BitAnd => a & b,
                BitOr => a | b,
                BitXor => a ^ b,
                _ => return None,
            }))
        }
        (Type::Double, TK::Double(a), TK::Double(b)) => Some(TK::Double(match op {
            Add => a + b,
            Sub => a - b,
            Mul => a * b,
            Div => a / b,
            Rem => a % b,
            _ => return None,
        })),
        _ => None,
    }
}

fn fold_cmp(op: BinOp, l: &TK, r: &TK) -> Option<bool> {
    use std::cmp::Ordering;
    use BinOp::*;
    let ord: Option<Ordering> = match (l, r) {
        (TK::Int(a), TK::Int(b)) => Some(a.cmp(b)),
        (TK::Long(a), TK::Long(b)) => Some(a.cmp(b)),
        (TK::Double(a), TK::Double(b)) => a.partial_cmp(b),
        _ => return None,
    };
    Some(match ord {
        None => op == Ne, // NaN: every comparison except != is false
        Some(o) => match op {
            Eq => o == Ordering::Equal,
            Ne => o != Ordering::Equal,
            Lt => o == Ordering::Less,
            Le => o != Ordering::Greater,
            Gt => o == Ordering::Greater,
            Ge => o != Ordering::Less,
            _ => return None,
        },
    })
}

fn const_string(e: &TE) -> Option<String> {
    Some(match (&e.kind, &e.ty) {
        (TK::Str(s), _) => s.clone(),
        (TK::Int(v), Type::Int) => v.to_string(),
        (TK::Int(v), Type::Char) => String::from_utf16_lossy(&[*v as u16]),
        (TK::Int(v), Type::Boolean) => (if *v != 0 { "true" } else { "false" }).to_string(),
        (TK::Long(v), _) => v.to_string(),
        _ => return None,
    })
}

fn bool_te(b: bool) -> TE {
    te(TK::Int(b as i32), Type::Boolean)
}

fn is_pure(e: &TE) -> bool {
    matches!(e.kind, TK::Local(_) | TK::This)
}

impl<'a> Checker<'a> {
    pub fn new(t: &'a ClassTable, cls: &'a ClassInfo, file: usize) -> Self {
        Checker {
            t,
            cls,
            this_ty: Type::class(&cls.name),
            file,
            is_static: true,
            ret: Type::Void,
            scopes: vec![vec![]],
            next_slot: 0,
            max_slot: 0,
            targets: vec![],
            next_id: 0,
            breaks: vec![],
            continues: vec![],
            pending_label: None,
        }
    }

    fn err<T>(&self, pos: Pos, msg: impl Into<String>) -> Result<T> {
        err(self.file, pos, msg)
    }

    fn resolve_type(&self, t: &AstType) -> Result<Type> {
        declare::resolve_type(self.t, t, &self.cls.name, self.file)
    }

    fn this_te(&self) -> TE {
        te(TK::This, self.this_ty.clone())
    }

    fn is_interface(&self, c: &str) -> bool {
        self.t.get(c).map_or(false, |c| c.is_interface)
    }

    fn in_nest(&self, c: &str) -> bool {
        self.t.get(c).map_or(false, |c| c.in_nest)
    }

    // ---------- locals ----------
    fn reset_locals(&mut self, is_static: bool) {
        self.scopes = vec![vec![]];
        self.next_slot = if is_static { 0 } else { 1 };
        self.max_slot = self.next_slot;
        self.is_static = is_static;
    }

    fn push_scope(&mut self) {
        self.scopes.push(vec![]);
    }

    fn pop_scope(&mut self) {
        let s = self.scopes.pop().unwrap();
        if let Some((_, slot, _)) = s.first() {
            self.next_slot = *slot as u32;
        }
    }

    fn lookup_local(&self, name: &str) -> Option<(u16, Type)> {
        for s in self.scopes.iter().rev() {
            for (n, slot, ty) in s.iter().rev() {
                if n == name {
                    return Some((*slot, ty.clone()));
                }
            }
        }
        None
    }

    fn alloc(&mut self, ty: &Type, pos: Pos) -> Result<u16> {
        let slot = self.next_slot;
        self.next_slot += ty.slots() as u32;
        if self.next_slot > 65535 {
            return self.err(pos, "too many local variables");
        }
        self.max_slot = self.max_slot.max(self.next_slot);
        Ok(slot as u16)
    }

    fn declare_local(&mut self, name: &str, ty: Type, pos: Pos) -> Result<u16> {
        if self.lookup_local(name).is_some() {
            return self.err(pos, format!("variable {} is already defined in method", name));
        }
        let slot = self.alloc(&ty, pos)?;
        self.scopes.last_mut().unwrap().push((name.to_string(), slot, ty));
        Ok(slot)
    }

    /// Anonymous temporary in the current scope.
    fn temp(&mut self, ty: Type, pos: Pos) -> Result<u16> {
        let slot = self.alloc(&ty, pos)?;
        self.scopes.last_mut().unwrap().push((String::from("\0tmp"), slot, ty));
        Ok(slot)
    }

    // ---------- conversions ----------
    fn method_conv(&self, from: &Type, to: &Type, boxing: bool) -> bool {
        if from == to {
            return true;
        }
        if from.is_primitive() && to.is_primitive() {
            return widening(from, to);
        }
        if from.is_reference() && to.is_reference() {
            return self.t.ref_assignable(from, to);
        }
        if boxing && from.is_primitive() && to.is_reference() {
            if let Some((w, _)) = wrapper(from) {
                return self.t.ref_assignable(&Type::class(w), to);
            }
        }
        false
    }

    fn box_value(&self, e: TE) -> TE {
        let (w, desc) = wrapper(&e.ty).unwrap();
        te(
            TK::Call {
                kind: Invoke::Static,
                owner: w.into(),
                name: "valueOf".into(),
                desc: desc.into(),
                recv: None,
                args: vec![e],
            },
            Type::class(w),
        )
    }

    fn coerce(&self, e: TE, to: &Type, pos: Pos) -> Result<TE> {
        if e.ty == *to {
            return Ok(e);
        }
        if e.ty == Type::Void {
            return self.err(pos, "'void' type not allowed here");
        }
        if e.ty.is_primitive() && to.is_primitive() {
            if widening(&e.ty, to) {
                return Ok(convert(e, to));
            }
            if *to == Type::Char && e.ty == Type::Int {
                if let TK::Int(v) = e.kind {
                    if (0..=65535).contains(&v) {
                        return Ok(te(TK::Int(v), Type::Char));
                    }
                }
            }
            if e.ty == Type::Boolean || *to == Type::Boolean {
                return self.err(
                    pos,
                    format!("incompatible types: {} cannot be converted to {}", e.ty, to),
                );
            }
            return self.err(
                pos,
                format!("incompatible types: possible lossy conversion from {} to {}", e.ty, to),
            );
        }
        if e.ty.is_reference() && to.is_reference() {
            if self.t.ref_assignable(&e.ty, to) {
                return Ok(e);
            }
            return self.err(
                pos,
                format!("incompatible types: {} cannot be converted to {}", e.ty, to),
            );
        }
        if e.ty.is_primitive() && self.method_conv(&e.ty, to, true) {
            return Ok(self.box_value(e));
        }
        if e.ty.is_primitive() && to.is_reference() {
            return self.err(
                pos,
                format!(
                    "incompatible types: {} cannot be converted to {} (boxing is only supported to Object)",
                    e.ty, to
                ),
            );
        }
        self.err(
            pos,
            format!(
                "incompatible types: {} cannot be converted to {} (unboxing is not supported)",
                e.ty, to
            ),
        )
    }

    fn cond_expr(&mut self, e: &Expr) -> Result<TE> {
        let c = self.expr(e)?;
        if c.ty != Type::Boolean {
            return self.err(
                e.pos,
                format!("incompatible types: {} cannot be converted to boolean", c.ty),
            );
        }
        Ok(c)
    }

    fn check_private(&self, owner: &str, flags: u16, what: &str, pos: Pos) -> Result<()> {
        if flags & ACC_PRIVATE != 0 && top_level(self.t, owner) != top_level(self.t, &self.cls.name) {
            return self.err(pos, format!("{} has private access in {}", what, Type::class(owner)));
        }
        Ok(())
    }

    // ---------- names and fields ----------
    fn static_field_te(&self, owner: &str, f: &FieldInfo, pos: Pos) -> Result<TE> {
        self.check_private(owner, f.flags, &f.name, pos)?;
        if let Some(c) = &f.constant {
            let k = match c {
                Const::Int(v) => TK::Int(*v),
                Const::Long(v) => TK::Long(*v),
                Const::Double(v) => TK::Double(*v),
                Const::Str(s) => TK::Str(s.clone()),
            };
            return Ok(te(k, f.ty.clone()));
        }
        Ok(te(
            TK::GetStatic {
                owner: owner.to_string(),
                name: f.name.clone(),
            },
            f.ty.clone(),
        ))
    }

    fn field_by_simple_name(&self, n: &str, pos: Pos) -> Result<Option<TE>> {
        if let Some((owner, f)) = self.t.find_field(&self.cls.name, n) {
            if f.is_static {
                return self.static_field_te(&owner, f, pos).map(Some);
            }
            if self.is_static {
                return self.err(
                    pos,
                    format!("non-static variable {} cannot be referenced from a static context", n),
                );
            }
            self.check_private(&owner, f.flags, n, pos)?;
            return Ok(Some(te(
                TK::GetField {
                    obj: Box::new(self.this_te()),
                    owner,
                    name: n.to_string(),
                },
                f.ty.clone(),
            )));
        }
        let mut outer = self.cls.outer.clone();
        while let Some(o) = outer {
            if let Some((owner, f)) = self.t.find_field(&o, n) {
                if !f.is_static {
                    return self.err(pos, format!("cannot access instance field {} of an enclosing class: jolt treats nested classes as static", n));
                }
                return self.static_field_te(&owner, f, pos).map(Some);
            }
            outer = self.t.get(&o).and_then(|c| c.outer.clone());
        }
        Ok(None)
    }

    fn instance_field(&self, obj: TE, name: &str, pos: Pos) -> Result<TE> {
        match &obj.ty {
            Type::Array(_) if name == "length" => Ok(te(TK::ArrayLen(Box::new(obj)), Type::Int)),
            Type::Class(c) => {
                let Some((owner, f)) = self.t.find_field(c, name) else {
                    return self.err(pos, format!("cannot find symbol: variable {} in {}", name, obj.ty));
                };
                if f.is_static {
                    if is_pure(&obj) {
                        return self.static_field_te(&owner, f, pos);
                    }
                    return self.err(
                        pos,
                        format!(
                            "static field {} must be accessed as {}.{}",
                            name,
                            Type::class(&owner),
                            name
                        ),
                    );
                }
                self.check_private(&owner, f.flags, name, pos)?;
                Ok(te(
                    TK::GetField {
                        obj: Box::new(obj),
                        owner,
                        name: name.to_string(),
                    },
                    f.ty.clone(),
                ))
            }
            t => self.err(pos, format!("{} cannot be dereferenced", t)),
        }
    }

    fn resolve(&mut self, e: &Expr) -> Result<R> {
        match &e.kind {
            ExprKind::Name(n) => {
                if let Some((slot, ty)) = self.lookup_local(n) {
                    return Ok(R::V(te(TK::Local(slot), ty)));
                }
                if let Some(v) = self.field_by_simple_name(n, e.pos)? {
                    return Ok(R::V(v));
                }
                if let Some(c) = lookup_class(self.t, n, &self.cls.name) {
                    return Ok(R::C(c));
                }
                Ok(R::P(n.clone()))
            }
            ExprKind::Field(target, name) if matches!(target.kind, ExprKind::Super) => {
                if self.is_static {
                    return self.err(
                        target.pos,
                        "non-static variable super cannot be referenced from a static context",
                    );
                }
                let sup = self.cls.super_name.clone().unwrap_or_else(|| OBJECT.into());
                let this = TE {
                    kind: TK::This,
                    ty: Type::class(&sup),
                };
                self.instance_field(this, name, e.pos).map(R::V)
            }
            ExprKind::Field(target, name) => match self.resolve(target)? {
                R::P(p) => {
                    let full = format!("{}/{}", p, name);
                    if self.t.get(&full).is_some() {
                        Ok(R::C(full))
                    } else {
                        Ok(R::P(full))
                    }
                }
                R::C(c) => {
                    if let Some((owner, f)) = self.t.find_field(&c, name) {
                        if !f.is_static {
                            return self.err(
                                e.pos,
                                format!(
                                    "non-static variable {} cannot be referenced from a static context",
                                    name
                                ),
                            );
                        }
                        return self.static_field_te(&owner, f, e.pos).map(R::V);
                    }
                    if let Some((_, n)) = self.t.get(&c).and_then(|ci| ci.nested.iter().find(|(s, _)| s == name)) {
                        return Ok(R::C(n.clone()));
                    }
                    if name == "length" || name == "class" {
                        return self.err(e.pos, format!("cannot find symbol: {}", name));
                    }
                    self.err(
                        e.pos,
                        format!("cannot find symbol: variable {} in class {}", name, Type::class(&c)),
                    )
                }
                R::V(obj) => self.instance_field(obj, name, e.pos).map(R::V),
            },
            _ => Ok(R::V(self.expr(e)?)),
        }
    }

    // ---------- method calls ----------
    fn select(
        &self,
        cands: Vec<(&'a str, &'a MethodInfo)>,
        args: Vec<TE>,
        name: &str,
        pos: Pos,
    ) -> Result<(String, &'a MethodInfo, Vec<TE>)> {
        let shown_name = if name == "<init>" { "constructor" } else { name };
        let arg_list = || args.iter().map(|a| a.ty.to_string()).collect::<Vec<_>>().join(",");
        if cands.is_empty() {
            return self.err(
                pos,
                format!("cannot find symbol: method {}({})", shown_name, arg_list()),
            );
        }
        let fits = |m: &MethodInfo, boxing: bool| {
            m.params.len() == args.len()
                && m.params
                    .iter()
                    .zip(&args)
                    .all(|(p, a)| self.method_conv(&a.ty, p, boxing))
        };
        let mut phase = 1;
        let mut app: Vec<usize> = (0..cands.len()).filter(|&i| fits(cands[i].1, false)).collect();
        if app.is_empty() {
            phase = 2;
            app = (0..cands.len()).filter(|&i| fits(cands[i].1, true)).collect();
        }
        if app.is_empty() {
            phase = 3;
            app = (0..cands.len())
                .filter(|&i| {
                    let m = cands[i].1;
                    if !m.varargs || args.len() + 1 < m.params.len() {
                        return false;
                    }
                    let nfix = m.params.len() - 1;
                    let Type::Array(elem) = &m.params[nfix] else {
                        return false;
                    };
                    m.params[..nfix]
                        .iter()
                        .zip(&args)
                        .all(|(p, a)| self.method_conv(&a.ty, p, true))
                        && args[nfix..].iter().all(|a| self.method_conv(&a.ty, elem, true))
                })
                .collect();
        }
        if app.is_empty() {
            let what = if name == "<init>" {
                format!(
                    "no suitable constructor found for {}({})",
                    Type::class(cands[0].0),
                    arg_list()
                )
            } else {
                format!("no suitable method found for {}({})", name, arg_list())
            };
            return self.err(pos, what);
        }
        let more_specific = |a: &MethodInfo, b: &MethodInfo| {
            a.params.len() == b.params.len()
                && a.params
                    .iter()
                    .zip(&b.params)
                    .all(|(x, y)| self.method_conv(x, y, false))
        };
        let best = app
            .iter()
            .copied()
            .find(|&i| app.iter().all(|&j| i == j || more_specific(cands[i].1, cands[j].1)));
        let Some(best) = best else {
            return self.err(pos, format!("reference to {} is ambiguous", shown_name));
        };
        let (owner, m) = (cands[best].0.to_string(), cands[best].1);
        let mut out = Vec::with_capacity(m.params.len());
        if phase == 3 {
            let nfix = m.params.len() - 1;
            let mut it = args.into_iter();
            for p in &m.params[..nfix] {
                out.push(self.coerce(it.next().unwrap(), p, pos)?);
            }
            let arr_ty = m.params[nfix].clone();
            let Type::Array(elem) = &arr_ty else { unreachable!() };
            let mut elems = Vec::new();
            for a in it {
                elems.push(self.coerce(a, elem, pos)?);
            }
            out.push(te(TK::ArrayLit(elems), arr_ty));
        } else {
            for (a, p) in args.into_iter().zip(&m.params) {
                out.push(self.coerce(a, p, pos)?);
            }
        }
        Ok((owner, m, out))
    }

    fn invoke_static(&self, owner: &str, m: &MethodInfo, args: Vec<TE>, pos: Pos) -> Result<TE> {
        self.check_private(owner, m.flags, &m.name, pos)?;
        Ok(te(
            TK::Call {
                kind: Invoke::Static,
                owner: owner.to_string(),
                name: m.name.clone(),
                desc: m.desc.clone(),
                recv: None,
                args,
            },
            m.ret.clone(),
        ))
    }

    fn invoke_virtual(
        &self,
        recv: TE,
        recv_class: &str,
        decl_owner: &str,
        m: &MethodInfo,
        args: Vec<TE>,
        pos: Pos,
    ) -> Result<TE> {
        self.check_private(decl_owner, m.flags, &m.name, pos)?;
        let (kind, owner) = if self.is_interface(recv_class) {
            (Invoke::Interface, recv_class)
        } else if m.flags & ACC_PRIVATE != 0 && !self.in_nest(decl_owner) {
            (Invoke::Special, decl_owner)
        } else {
            (Invoke::Virtual, recv_class)
        };
        Ok(te(
            TK::Call {
                kind,
                owner: owner.to_string(),
                name: m.name.clone(),
                desc: m.desc.clone(),
                recv: Some(Box::new(recv)),
                args,
            },
            m.ret.clone(),
        ))
    }

    fn call(&mut self, target: Option<&Expr>, name: &str, args: &[Expr], pos: Pos) -> Result<TE> {
        let mut targs = Vec::with_capacity(args.len());
        for a in args {
            let v = self.expr(a)?;
            if v.ty == Type::Void {
                return self.err(a.pos, "'void' type not allowed here");
            }
            targs.push(v);
        }
        let t = self.t;
        match target {
            None => {
                let cands = t.find_methods(&self.cls.name, name);
                if !cands.is_empty() {
                    let (owner, m, args) = self.select(cands, targs, name, pos)?;
                    if m.is_static {
                        return self.invoke_static(&owner, m, args, pos);
                    }
                    if self.is_static {
                        return self.err(
                            pos,
                            format!(
                                "non-static method {}({}) cannot be referenced from a static context",
                                name,
                                m.params.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
                            ),
                        );
                    }
                    let this = self.this_te();
                    let cls = self.cls.name.clone();
                    return self.invoke_virtual(this, &cls, &owner, m, args, pos);
                }
                let mut outer = self.cls.outer.clone();
                while let Some(o) = outer {
                    let cands = t.find_methods(&o, name);
                    if !cands.is_empty() {
                        let (owner, m, args) = self.select(cands, targs, name, pos)?;
                        if !m.is_static {
                            return self.err(pos, format!("cannot call instance method {} of an enclosing class: jolt treats nested classes as static", name));
                        }
                        return self.invoke_static(&owner, m, args, pos);
                    }
                    outer = t.get(&o).and_then(|c| c.outer.clone());
                }
                self.err(
                    pos,
                    format!(
                        "cannot find symbol: method {}({})",
                        name,
                        targs.iter().map(|a| a.ty.to_string()).collect::<Vec<_>>().join(",")
                    ),
                )
            }
            Some(Expr {
                kind: ExprKind::Super,
                pos: spos,
            }) => {
                if self.is_static {
                    return self.err(
                        *spos,
                        "non-static variable super cannot be referenced from a static context",
                    );
                }
                let sup = self.cls.super_name.clone().unwrap_or_else(|| OBJECT.into());
                let cands = t.find_methods(&sup, name);
                let (owner, m, args) = self.select(cands, targs, name, pos)?;
                if m.is_static {
                    return self.invoke_static(&owner, m, args, pos);
                }
                if m.flags & ACC_ABSTRACT != 0 {
                    return self.err(pos, format!("abstract method {} cannot be accessed directly", name));
                }
                Ok(te(
                    TK::Call {
                        kind: Invoke::Special,
                        owner: sup,
                        name: name.to_string(),
                        desc: m.desc.clone(),
                        recv: Some(Box::new(self.this_te())),
                        args,
                    },
                    m.ret.clone(),
                ))
            }
            Some(tg) => match self.resolve(tg)? {
                R::C(c) => {
                    let cands = t.find_methods(&c, name);
                    let (owner, m, args) = self.select(cands, targs, name, pos)?;
                    if !m.is_static {
                        return self.err(
                            pos,
                            format!("non-static method {} cannot be referenced from a static context", name),
                        );
                    }
                    self.invoke_static(&owner, m, args, pos)
                }
                R::P(p) => self.err(tg.pos, format!("cannot find symbol: {}", p.replace('/', "."))),
                R::V(recv) => {
                    let cls = match &recv.ty {
                        Type::Class(c) => c.clone(),
                        Type::Array(_) => std::sync::Arc::<str>::from(OBJECT),
                        Type::Null => return self.err(pos, "cannot call a method on null"),
                        other => return self.err(tg.pos, format!("{} cannot be dereferenced", other)),
                    };
                    let cands = t.find_methods(&cls, name);
                    if cands.is_empty() {
                        return self.err(
                            pos,
                            format!(
                                "cannot find symbol: method {}({}) in {}",
                                name,
                                targs.iter().map(|a| a.ty.to_string()).collect::<Vec<_>>().join(","),
                                recv.ty
                            ),
                        );
                    }
                    let (owner, m, args) = self.select(cands, targs, name, pos)?;
                    if m.is_static {
                        if is_pure(&recv) {
                            return self.invoke_static(&owner, m, args, pos);
                        }
                        return self.err(
                            pos,
                            format!(
                                "static method {} should be called as {}.{}(...)",
                                name,
                                Type::class(&owner),
                                name
                            ),
                        );
                    }
                    self.invoke_virtual(recv, &cls, &owner, m, args, pos)
                }
            },
        }
    }

    // ---------- expressions ----------
    pub fn expr(&mut self, e: &Expr) -> Result<TE> {
        let pos = e.pos;
        match &e.kind {
            ExprKind::Lit(l) => Ok(match l {
                Lit::Int(v) => {
                    if *v > i32::MAX as i64 {
                        return self.err(pos, "integer number too large");
                    }
                    te(TK::Int(*v as i32), Type::Int)
                }
                Lit::Long(v) => te(TK::Long(*v as i64), Type::Long),
                Lit::Double(v) => te(TK::Double(*v), Type::Double),
                Lit::Char(c) => te(TK::Int(*c as i32), Type::Char),
                Lit::Str(s) => te(TK::Str(s.clone()), Type::string()),
                Lit::Bool(b) => bool_te(*b),
                Lit::Null => te(TK::Null, Type::Null),
            }),
            ExprKind::Name(_) | ExprKind::Field(..) => match self.resolve(e)? {
                R::V(v) => Ok(v),
                R::C(c) => self.err(pos, format!("{} is a class, not a value", Type::class(&c))),
                R::P(p) => {
                    let last = p.rsplit('/').next().unwrap_or(&p).to_string();
                    self.err(pos, format!("cannot find symbol: variable {}", last))
                }
            },
            ExprKind::This => {
                if self.is_static {
                    return self.err(
                        pos,
                        "non-static variable this cannot be referenced from a static context",
                    );
                }
                Ok(self.this_te())
            }
            ExprKind::Super => self.err(pos, "'super' must be followed by a method call"),
            ExprKind::Index(a, i) => {
                let arr = self.expr(a)?;
                let Type::Array(elem) = arr.ty.clone() else {
                    return self.err(pos, format!("array required, but {} found", arr.ty));
                };
                let idx = self.expr(i)?;
                if !matches!(idx.ty, Type::Int | Type::Char) {
                    return self.err(
                        i.pos,
                        format!("incompatible types: {} cannot be converted to int", idx.ty),
                    );
                }
                let idx = convert(idx, &Type::Int);
                Ok(te(
                    TK::ArrayLoad {
                        arr: Box::new(arr),
                        idx: Box::new(idx),
                    },
                    *elem,
                ))
            }
            ExprKind::Call { target, name, args } => self.call(target.as_deref(), name, args, pos),
            ExprKind::New(ty, args) => {
                let ty = self.resolve_type(ty)?;
                let Type::Class(c) = &ty else {
                    return self.err(pos, "cannot instantiate a primitive type");
                };
                let ci = self.t.get(c).unwrap();
                if ci.is_abstract {
                    return self.err(pos, format!("{} is abstract; cannot be instantiated", ty));
                }
                let mut targs = Vec::new();
                for a in args {
                    targs.push(self.expr(a)?);
                }
                let cands = self.t.find_methods(c, "<init>");
                if cands.is_empty() {
                    return self.err(pos, format!("jolt does not know the constructors of {}", ty));
                }
                let (_, m, args) = self.select(cands, targs, "<init>", pos)?;
                self.check_private(c, m.flags, &format!("{}()", ty), pos)?;
                Ok(te(
                    TK::New {
                        class: c.to_string(),
                        desc: m.desc.clone(),
                        args,
                    },
                    ty.clone(),
                ))
            }
            ExprKind::NewArray {
                elem,
                dims,
                extra_dims,
                init,
            } => {
                let mut ty = self.resolve_type(elem)?;
                if ty == Type::Void {
                    return self.err(pos, "'void' type not allowed here");
                }
                for _ in 0..dims.len() + extra_dims {
                    ty = Type::array_of(ty);
                }
                if let Some(init) = init {
                    return self.array_init(init, &ty, pos);
                }
                let mut tdims = Vec::new();
                for d in dims {
                    let v = self.expr(d)?;
                    if !matches!(v.ty, Type::Int | Type::Char) {
                        return self.err(
                            d.pos,
                            format!("incompatible types: {} cannot be converted to int", v.ty),
                        );
                    }
                    tdims.push(convert(v, &Type::Int));
                }
                Ok(te(TK::NewArray(tdims), ty))
            }
            ExprKind::ArrayInit(_) => self.err(pos, "array initializer is not allowed here; use new T[]{...}"),
            ExprKind::Unary(op, x) => {
                let v = self.expr(x)?;
                match op {
                    UnOp::Not => {
                        if v.ty != Type::Boolean {
                            return self.err(pos, format!("bad operand type {} for unary operator '!'", v.ty));
                        }
                        if let Some(b) = const_bool(&v) {
                            return Ok(bool_te(!b));
                        }
                        Ok(te(TK::Not(Box::new(v)), Type::Boolean))
                    }
                    UnOp::Neg | UnOp::Plus => {
                        if !v.ty.is_numeric() {
                            return self.err(
                                pos,
                                format!(
                                    "bad operand type {} for unary operator '{}'",
                                    v.ty,
                                    if *op == UnOp::Neg { "-" } else { "+" }
                                ),
                            );
                        }
                        let ty = promote1(&v.ty);
                        let v = convert(v, &ty);
                        if *op == UnOp::Plus {
                            return Ok(v);
                        }
                        Ok(match v.kind {
                            TK::Int(x) => te(TK::Int(x.wrapping_neg()), ty),
                            TK::Long(x) => te(TK::Long(x.wrapping_neg()), ty),
                            TK::Double(x) => te(TK::Double(-x), ty),
                            _ => te(TK::Neg(Box::new(v)), ty),
                        })
                    }
                    UnOp::BitNot => {
                        if !v.ty.is_integral() {
                            return self.err(pos, format!("bad operand type {} for unary operator '~'", v.ty));
                        }
                        let ty = promote1(&v.ty);
                        let v = convert(v, &ty);
                        let m1 = if ty == Type::Long {
                            te(TK::Long(-1), Type::Long)
                        } else {
                            te(TK::Int(-1), Type::Int)
                        };
                        Ok(self.arith(BinOp::BitXor, v, m1, ty))
                    }
                }
            }
            ExprKind::IncDec { pre, inc, target } => {
                let lv = self.lvalue(target)?;
                if !lv.ty().is_numeric() {
                    return self.err(
                        pos,
                        format!(
                            "bad operand type {} for unary operator '{}'",
                            lv.ty(),
                            if *inc { "++" } else { "--" }
                        ),
                    );
                }
                let ty = lv.ty().clone();
                Ok(te(
                    TK::IncDec {
                        target: Box::new(lv),
                        pre: *pre,
                        inc: *inc,
                    },
                    ty,
                ))
            }
            ExprKind::Binary(op, l, r) => {
                let lv = self.expr(l)?;
                let rv = self.expr(r)?;
                self.binary(*op, lv, rv, pos)
            }
            ExprKind::Assign { op, target, value } => {
                let lv = self.lvalue(target)?;
                let lty = lv.ty().clone();
                let v = match (&value.kind, op) {
                    (ExprKind::ArrayInit(elems), None) => self.array_init(elems, &lty, value.pos)?,
                    _ => self.expr(value)?,
                };
                match op {
                    None => {
                        let v = self.coerce(v, &lty, value.pos)?;
                        Ok(te(TK::Assign(Box::new(lv), Box::new(v)), lty))
                    }
                    Some(BinOp::Add) if lty.is_string() => {
                        if v.ty == Type::Void {
                            return self.err(value.pos, "'void' type not allowed here");
                        }
                        let mut parts = Vec::new();
                        match v.kind {
                            TK::Concat(ps) => parts.extend(ps),
                            _ => parts.push(v),
                        }
                        Ok(te(
                            TK::StrAppendAssign {
                                target: Box::new(lv),
                                parts,
                            },
                            lty,
                        ))
                    }
                    Some(op) => {
                        let op = *op;
                        let op_ty = match op {
                            BinOp::Shl | BinOp::Shr | BinOp::UShr => {
                                if !lty.is_integral() || !v.ty.is_integral() {
                                    return self
                                        .err(pos, format!("bad operand types for binary operator '{}='", op.symbol()));
                                }
                                promote1(&lty)
                            }
                            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor
                                if lty == Type::Boolean && v.ty == Type::Boolean =>
                            {
                                Type::Boolean
                            }
                            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                                if !lty.is_integral() || !v.ty.is_integral() {
                                    return self
                                        .err(pos, format!("bad operand types for binary operator '{}='", op.symbol()));
                                }
                                promote2(&lty, &v.ty).unwrap()
                            }
                            _ => match promote2(&lty, &v.ty) {
                                Some(t) => t,
                                None => {
                                    return self.err(
                                        pos,
                                        format!(
                                            "bad operand types for binary operator '{}=': {} and {}",
                                            op.symbol(),
                                            lty,
                                            v.ty
                                        ),
                                    )
                                }
                            },
                        };
                        let v = if matches!(op, BinOp::Shl | BinOp::Shr | BinOp::UShr) {
                            convert(v, &Type::Int)
                        } else {
                            convert(v, &op_ty)
                        };
                        Ok(te(
                            TK::CompoundAssign {
                                target: Box::new(lv),
                                op,
                                value: Box::new(v),
                                op_ty,
                            },
                            lty,
                        ))
                    }
                }
            }
            ExprKind::Cond(c, a, b) => {
                let c = self.cond_expr(c)?;
                let a = self.expr(a)?;
                let b = self.expr(b)?;
                let ty = self.cond_type(&a, &b, pos)?;
                let a = self.cond_branch(a, &ty, pos)?;
                let b = self.cond_branch(b, &ty, pos)?;
                if let Some(cb) = const_bool(&c) {
                    return Ok(if cb { a } else { b });
                }
                Ok(te(TK::Cond(Box::new(c), Box::new(a), Box::new(b)), ty))
            }
            ExprKind::Cast(ty, x) => {
                let to = self.resolve_type(ty)?;
                let v = self.expr(x)?;
                self.cast(v, &to, pos)
            }
            ExprKind::InstanceOf(x, ty) => {
                let v = self.expr(x)?;
                let to = self.resolve_type(ty)?;
                if !v.ty.is_reference() || !to.is_reference() {
                    return self.err(pos, format!("unexpected type for instanceof: {}", v.ty));
                }
                if !self.castable(&v.ty, &to) {
                    return self.err(
                        pos,
                        format!("incompatible types: {} cannot be converted to {}", v.ty, to),
                    );
                }
                Ok(te(TK::InstanceOf(Box::new(v), to), Type::Boolean))
            }
        }
    }

    fn castable(&self, from: &Type, to: &Type) -> bool {
        if self.t.ref_assignable(from, to) || self.t.ref_assignable(to, from) {
            return true;
        }
        match (from, to) {
            (Type::Class(a), Type::Class(b)) => self.is_interface(a) || self.is_interface(b),
            _ => false,
        }
    }

    fn cast(&self, v: TE, to: &Type, pos: Pos) -> Result<TE> {
        if v.ty == *to {
            return Ok(v);
        }
        if v.ty.is_primitive() && to.is_primitive() {
            if v.ty == Type::Boolean || *to == Type::Boolean {
                return self.err(
                    pos,
                    format!("incompatible types: {} cannot be converted to {}", v.ty, to),
                );
            }
            return Ok(convert(v, to));
        }
        if v.ty.is_reference() && to.is_reference() {
            if self.t.ref_assignable(&v.ty, to) {
                return Ok(TE { ty: to.clone(), ..v });
            }
            if self.castable(&v.ty, to) {
                return Ok(te(TK::CheckCast(Box::new(v)), to.clone()));
            }
            return self.err(
                pos,
                format!("incompatible types: {} cannot be converted to {}", v.ty, to),
            );
        }
        if v.ty.is_primitive() && *to == Type::object() {
            return Ok(self.box_value(v));
        }
        self.err(
            pos,
            format!(
                "incompatible types: {} cannot be converted to {} (boxing/unboxing casts are not supported)",
                v.ty, to
            ),
        )
    }

    fn cond_type(&self, a: &TE, b: &TE, pos: Pos) -> Result<Type> {
        if a.ty == b.ty {
            return Ok(a.ty.clone());
        }
        if a.ty.is_numeric() && b.ty.is_numeric() {
            let fits_char = |e: &TE| matches!(e.kind, TK::Int(v) if (0..=65535).contains(&v)) && e.ty == Type::Int;
            if (a.ty == Type::Char && fits_char(b)) || (b.ty == Type::Char && fits_char(a)) {
                return Ok(Type::Char);
            }
            return Ok(promote2(&a.ty, &b.ty).unwrap());
        }
        if a.ty == Type::Null && b.ty.is_reference() {
            return Ok(b.ty.clone());
        }
        if b.ty == Type::Null && a.ty.is_reference() {
            return Ok(a.ty.clone());
        }
        if a.ty.is_reference() && b.ty.is_reference() {
            if self.t.ref_assignable(&a.ty, &b.ty) {
                return Ok(b.ty.clone());
            }
            if self.t.ref_assignable(&b.ty, &a.ty) {
                return Ok(a.ty.clone());
            }
            return Ok(Type::object());
        }
        self.err(
            pos,
            format!("incompatible types in conditional expression: {} and {}", a.ty, b.ty),
        )
    }

    fn cond_branch(&self, e: TE, ty: &Type, pos: Pos) -> Result<TE> {
        if e.ty == *ty {
            return Ok(e);
        }
        if e.ty.is_primitive() {
            if *ty == Type::Char {
                if let TK::Int(v) = e.kind {
                    return Ok(te(TK::Int(v), Type::Char));
                }
            }
            return Ok(convert(e, ty));
        }
        let _ = pos;
        Ok(e)
    }

    fn arith(&self, op: BinOp, l: TE, r: TE, ty: Type) -> TE {
        if let Some(k) = fold_arith(op, &ty, &l.kind, &r.kind) {
            return te(k, ty);
        }
        te(TK::Arith(op, Box::new(l), Box::new(r)), ty)
    }

    fn binary(&mut self, op: BinOp, l: TE, r: TE, pos: Pos) -> Result<TE> {
        use BinOp::*;
        if l.ty == Type::Void || r.ty == Type::Void {
            return self.err(pos, "'void' type not allowed here");
        }
        let bad = |s: &Self| {
            s.err(
                pos,
                format!(
                    "bad operand types for binary operator '{}': {} and {}",
                    op.symbol(),
                    l.ty,
                    r.ty
                ),
            )
        };
        match op {
            And | Or => {
                if l.ty != Type::Boolean || r.ty != Type::Boolean {
                    return bad(self);
                }
                if let (Some(a), Some(b)) = (const_bool(&l), const_bool(&r)) {
                    return Ok(bool_te(if op == And { a && b } else { a || b }));
                }
                Ok(te(
                    if op == And {
                        TK::And(Box::new(l), Box::new(r))
                    } else {
                        TK::Or(Box::new(l), Box::new(r))
                    },
                    Type::Boolean,
                ))
            }
            Add if l.ty.is_string() || r.ty.is_string() => {
                let mut parts = Vec::new();
                for p in [l, r] {
                    match p.kind {
                        TK::Concat(ps) => parts.extend(ps),
                        _ => parts.push(p),
                    }
                }
                Ok(self.fold_concat(parts))
            }
            Add | Sub | Mul | Div | Rem => {
                let Some(ty) = promote2(&l.ty, &r.ty) else {
                    return bad(self);
                };
                let (l, r) = (convert(l, &ty), convert(r, &ty));
                if matches!(op, Div | Rem) && matches!(r.kind, TK::Int(0) | TK::Long(0)) && is_const(&l) {
                    // Division by constant zero is left to run time, like javac.
                    return Ok(te(TK::Arith(op, Box::new(l), Box::new(r)), ty));
                }
                Ok(self.arith(op, l, r, ty))
            }
            Shl | Shr | UShr => {
                if !l.ty.is_integral() || !r.ty.is_integral() {
                    return bad(self);
                }
                let ty = promote1(&l.ty);
                let l = convert(l, &ty);
                let r = convert(r, &Type::Int);
                Ok(self.arith(op, l, r, ty))
            }
            BitAnd | BitOr | BitXor => {
                if l.ty == Type::Boolean && r.ty == Type::Boolean {
                    return Ok(self.arith(op, l, r, Type::Boolean));
                }
                if !l.ty.is_integral() || !r.ty.is_integral() {
                    return bad(self);
                }
                let ty = promote2(&l.ty, &r.ty).unwrap();
                let (l, r) = (convert(l, &ty), convert(r, &ty));
                Ok(self.arith(op, l, r, ty))
            }
            Lt | Le | Gt | Ge => {
                let Some(ty) = promote2(&l.ty, &r.ty) else {
                    return bad(self);
                };
                let (l, r) = (convert(l, &ty), convert(r, &ty));
                if let Some(b) = fold_cmp(op, &l.kind, &r.kind) {
                    return Ok(bool_te(b));
                }
                Ok(te(TK::Cmp(op, Box::new(l), Box::new(r)), Type::Boolean))
            }
            Eq | Ne => {
                if let Some(ty) = promote2(&l.ty, &r.ty) {
                    let (l, r) = (convert(l, &ty), convert(r, &ty));
                    if let Some(b) = fold_cmp(op, &l.kind, &r.kind) {
                        return Ok(bool_te(b));
                    }
                    return Ok(te(TK::Cmp(op, Box::new(l), Box::new(r)), Type::Boolean));
                }
                if l.ty == Type::Boolean && r.ty == Type::Boolean {
                    if let Some(b) = fold_cmp(op, &l.kind, &r.kind) {
                        return Ok(bool_te(b));
                    }
                    return Ok(te(TK::Cmp(op, Box::new(l), Box::new(r)), Type::Boolean));
                }
                if l.ty.is_reference() && r.ty.is_reference() {
                    if !self.castable(&l.ty, &r.ty) && l.ty != Type::Null && r.ty != Type::Null {
                        return self.err(pos, format!("incomparable types: {} and {}", l.ty, r.ty));
                    }
                    return Ok(te(TK::Cmp(op, Box::new(l), Box::new(r)), Type::Boolean));
                }
                bad(self)
            }
        }
    }

    fn fold_concat(&self, parts: Vec<TE>) -> TE {
        let mut out: Vec<TE> = Vec::with_capacity(parts.len());
        for p in parts {
            if let Some(s) = const_string(&p) {
                if let Some(last) = out.last_mut() {
                    if let TK::Str(prev) = &mut last.kind {
                        prev.push_str(&s);
                        continue;
                    }
                }
                out.push(te(TK::Str(s), Type::string()));
            } else {
                out.push(p);
            }
        }
        out.retain(|p| !matches!(&p.kind, TK::Str(s) if s.is_empty()));
        if out.is_empty() {
            return te(TK::Str(String::new()), Type::string());
        }
        if out.len() == 1 && matches!(out[0].kind, TK::Str(_)) {
            return out.pop().unwrap();
        }
        te(TK::Concat(out), Type::string())
    }

    fn lvalue(&mut self, e: &Expr) -> Result<LV> {
        match &e.kind {
            ExprKind::Name(_) | ExprKind::Field(..) | ExprKind::Index(..) => {}
            _ => return self.err(e.pos, "unexpected type: required variable, found value"),
        }
        let v = self.expr(e)?;
        Ok(match v.kind {
            TK::Local(s) => LV::Local(s, v.ty),
            TK::GetStatic { owner, name } => LV::Static { owner, name, ty: v.ty },
            TK::GetField { obj, owner, name } => LV::Field {
                obj: *obj,
                owner,
                name,
                ty: v.ty,
            },
            TK::ArrayLoad { arr, idx } => LV::Array {
                arr: *arr,
                idx: *idx,
                ty: v.ty,
            },
            TK::ArrayLen(_) => return self.err(e.pos, "cannot assign a value to final variable length"),
            _ => return self.err(e.pos, "cannot assign a value to a final variable"),
        })
    }

    fn array_init(&mut self, elems: &[Expr], ty: &Type, pos: Pos) -> Result<TE> {
        let Type::Array(et) = ty else {
            return self.err(pos, format!("illegal initializer for {}", ty));
        };
        let mut out = Vec::with_capacity(elems.len());
        for e in elems {
            let v = match &e.kind {
                ExprKind::ArrayInit(inner) => self.array_init(inner, et, e.pos)?,
                _ => {
                    let v = self.expr(e)?;
                    self.coerce(v, et, e.pos)?
                }
            };
            out.push(v);
        }
        Ok(te(TK::ArrayLit(out), ty.clone()))
    }

    // ---------- statements ----------
    fn new_target(&mut self, is_loop: bool, is_switch: bool) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.breaks.push(false);
        self.continues.push(false);
        let label = self.pending_label.take();
        self.targets.push(Target {
            id,
            label,
            is_loop,
            is_switch,
        });
        id
    }

    fn block(&mut self, stmts: &[Stmt], out: &mut Vec<TS>) -> Result<bool> {
        let mut completes = true;
        for s in stmts {
            if !completes {
                return self.err(s.pos, "unreachable statement");
            }
            completes = self.stmt(s, out)?;
        }
        Ok(completes)
    }

    fn sub_stmt(&mut self, s: &Stmt) -> Result<(TS, bool)> {
        self.push_scope();
        let mut v = Vec::new();
        let c = self.stmt(s, &mut v);
        self.pop_scope();
        let c = c?;
        Ok((if v.len() == 1 { v.pop().unwrap() } else { TS::Block(v) }, c))
    }

    fn stmt(&mut self, s: &Stmt, out: &mut Vec<TS>) -> Result<bool> {
        let pos = s.pos;
        if !matches!(s.kind, StmtKind::Block(_) | StmtKind::Empty | StmtKind::Labeled(..)) {
            out.push(TS::Line(pos.line));
        }
        if self.pending_label.is_some()
            && !matches!(
                s.kind,
                StmtKind::While(..) | StmtKind::DoWhile(..) | StmtKind::For { .. } | StmtKind::ForEach { .. }
            )
        {
            // Labeled non-loop statement: `break label` exits it.
            let id = self.new_target(false, false);
            let r = self.sub_stmt(s);
            self.targets.pop();
            let (body, c) = r?;
            out.push(TS::Labeled {
                body: Box::new(body),
                id,
            });
            return Ok(c || self.breaks[id as usize]);
        }
        match &s.kind {
            StmtKind::Empty => Ok(true),
            StmtKind::Block(v) => {
                self.push_scope();
                let mut inner = Vec::new();
                let c = self.block(v, &mut inner);
                self.pop_scope();
                let c = c?;
                out.push(TS::Block(inner));
                Ok(c)
            }
            StmtKind::LocalVar(ty, decls) => {
                for d in decls {
                    let mut dty = match ty {
                        AstType::Var => None,
                        t => Some(self.resolve_type(t)?),
                    };
                    if let Some(t) = dty.as_mut() {
                        if *t == Type::Void {
                            return self.err(d.pos, "'void' type not allowed here");
                        }
                        for _ in 0..d.extra_dims {
                            *t = Type::array_of(t.clone());
                        }
                    }
                    let value = match &d.init {
                        None => {
                            if dty.is_none() {
                                return self.err(d.pos, "cannot infer type for local variable (variable initializer required)");
                            }
                            None
                        }
                        Some(init) => Some(match (&init.kind, &dty) {
                            (ExprKind::ArrayInit(elems), Some(t)) => self.array_init(elems, t, init.pos)?,
                            (ExprKind::ArrayInit(_), None) => {
                                return self.err(init.pos, "cannot infer type for local variable (array initializer needs an explicit target-type)")
                            }
                            (_, Some(t)) => {
                                let v = self.expr(init)?;
                                self.coerce(v, t, init.pos)?
                            }
                            (_, None) => {
                                let v = self.expr(init)?;
                                if v.ty == Type::Null || v.ty == Type::Void {
                                    return self.err(init.pos, format!("cannot infer type for local variable {}", d.name));
                                }
                                v
                            }
                        }),
                    };
                    let ty = dty.unwrap_or_else(|| value.as_ref().unwrap().ty.clone());
                    let slot = self.declare_local(&d.name, ty.clone(), d.pos)?;
                    if let Some(v) = value {
                        out.push(TS::Expr(te(
                            TK::Assign(Box::new(LV::Local(slot, ty.clone())), Box::new(v)),
                            ty,
                        )));
                    }
                }
                Ok(true)
            }
            StmtKind::Expr(e) => {
                let v = self.expr(e)?;
                out.push(TS::Expr(v));
                Ok(true)
            }
            StmtKind::If(c, t, e) => {
                let c = self.cond_expr(c)?;
                let (tb, tc) = self.sub_stmt(t)?;
                let (eb, ec) = match e {
                    Some(e) => {
                        let (b, c) = self.sub_stmt(e)?;
                        (Some(Box::new(b)), c)
                    }
                    None => (None, true),
                };
                out.push(TS::If(c, Box::new(tb), eb));
                Ok(tc || ec)
            }
            StmtKind::While(c, body) => {
                let id = self.new_target(true, false);
                let c = self.cond_expr(c)?;
                let (b, _) = self.sub_stmt(body)?;
                self.targets.pop();
                let forever = const_bool(&c) == Some(true);
                out.push(TS::While {
                    cond: c,
                    body: Box::new(b),
                    id,
                });
                Ok(!forever || self.breaks[id as usize])
            }
            StmtKind::DoWhile(body, c) => {
                let id = self.new_target(true, false);
                let (b, bc) = self.sub_stmt(body)?;
                let c = self.cond_expr(c)?;
                self.targets.pop();
                let forever = const_bool(&c) == Some(true);
                out.push(TS::DoWhile {
                    body: Box::new(b),
                    cond: c,
                    id,
                });
                Ok(((bc || self.continues[id as usize]) && !forever) || self.breaks[id as usize])
            }
            StmtKind::For {
                init,
                cond,
                update,
                body,
            } => {
                self.push_scope();
                let label = self.pending_label.take();
                let r = (|| -> Result<bool> {
                    let mut pre = Vec::new();
                    for s in init {
                        self.stmt(s, &mut pre)?;
                    }
                    self.pending_label = label;
                    let id = self.new_target(true, false);
                    let c = match cond {
                        Some(c) => Some(self.cond_expr(c)?),
                        None => None,
                    };
                    let mut ups = Vec::new();
                    for u in update {
                        ups.push(self.expr(u)?);
                    }
                    let (b, _) = self.sub_stmt(body)?;
                    self.targets.pop();
                    let forever = match &c {
                        None => true,
                        Some(c) => const_bool(c) == Some(true),
                    };
                    pre.push(TS::For {
                        cond: c,
                        update: ups,
                        body: Box::new(b),
                        id,
                    });
                    out.push(TS::Block(pre));
                    Ok(!forever || self.breaks[id as usize])
                })();
                self.pop_scope();
                r
            }
            StmtKind::ForEach { ty, name, iter, body } => {
                self.push_scope();
                let r = (|| -> Result<bool> {
                    let it = self.expr(iter)?;
                    let Type::Array(elem) = it.ty.clone() else {
                        return self.err(
                            iter.pos,
                            format!(
                                "for-each loop requires an array, but found {} (Iterable is not supported)",
                                it.ty
                            ),
                        );
                    };
                    let arr_ty = it.ty.clone();
                    let arr = self.temp(arr_ty.clone(), pos)?;
                    let idx = self.temp(Type::Int, pos)?;
                    let vty = match ty {
                        AstType::Var => (*elem).clone(),
                        t => self.resolve_type(t)?,
                    };
                    let id = self.new_target(true, false);
                    let mut pre = vec![
                        TS::Expr(te(
                            TK::Assign(Box::new(LV::Local(arr, arr_ty.clone())), Box::new(it)),
                            arr_ty.clone(),
                        )),
                        TS::Expr(te(
                            TK::Assign(Box::new(LV::Local(idx, Type::Int)), Box::new(te(TK::Int(0), Type::Int))),
                            Type::Int,
                        )),
                    ];
                    self.push_scope();
                    let var = self.declare_local(name, vty.clone(), pos)?;
                    let load = te(
                        TK::ArrayLoad {
                            arr: Box::new(te(TK::Local(arr), arr_ty.clone())),
                            idx: Box::new(te(TK::Local(idx), Type::Int)),
                        },
                        (*elem).clone(),
                    );
                    let load = self.coerce(load, &vty, pos);
                    let res = load.and_then(|load| {
                        let (b, _) = self.sub_stmt(body)?;
                        Ok((load, b))
                    });
                    self.pop_scope();
                    let (load, b) = res?;
                    self.targets.pop();
                    let cond = te(
                        TK::Cmp(
                            BinOp::Lt,
                            Box::new(te(TK::Local(idx), Type::Int)),
                            Box::new(te(
                                TK::ArrayLen(Box::new(te(TK::Local(arr), arr_ty.clone()))),
                                Type::Int,
                            )),
                        ),
                        Type::Boolean,
                    );
                    let update = te(
                        TK::IncDec {
                            target: Box::new(LV::Local(idx, Type::Int)),
                            pre: true,
                            inc: true,
                        },
                        Type::Int,
                    );
                    let body = TS::Block(vec![
                        TS::Expr(te(
                            TK::Assign(Box::new(LV::Local(var, vty.clone())), Box::new(load)),
                            vty,
                        )),
                        b,
                    ]);
                    pre.push(TS::For {
                        cond: Some(cond),
                        update: vec![update],
                        body: Box::new(body),
                        id,
                    });
                    out.push(TS::Block(pre));
                    Ok(true)
                })();
                self.pop_scope();
                r
            }
            StmtKind::Labeled(label, body) => {
                if self.targets.iter().any(|t| t.label.as_deref() == Some(label)) {
                    return self.err(pos, format!("label {} already in use", label));
                }
                self.pending_label = Some(label.clone());
                let r = self.stmt(body, out);
                self.pending_label = None;
                r
            }
            StmtKind::Return(e) => {
                match (e, &self.ret) {
                    (None, Type::Void) => out.push(TS::Return(None)),
                    (Some(e), Type::Void) => return self.err(e.pos, "incompatible types: unexpected return value"),
                    (None, _) => return self.err(pos, "missing return value"),
                    (Some(e), rt) => {
                        let rt = rt.clone();
                        let v = self.expr(e)?;
                        let v = self.coerce(v, &rt, e.pos)?;
                        out.push(TS::Return(Some(v)));
                    }
                }
                Ok(false)
            }
            StmtKind::Break(label) => {
                let t = match label {
                    None => self.targets.iter().rev().find(|t| t.is_loop || t.is_switch),
                    Some(l) => self.targets.iter().rev().find(|t| t.label.as_deref() == Some(l)),
                };
                let Some(t) = t else {
                    return match label {
                        None => self.err(pos, "break outside switch or loop"),
                        Some(l) => self.err(pos, format!("undefined label: {}", l)),
                    };
                };
                let id = t.id;
                self.breaks[id as usize] = true;
                out.push(TS::Break(id));
                Ok(false)
            }
            StmtKind::Continue(label) => {
                let t = match label {
                    None => self.targets.iter().rev().find(|t| t.is_loop),
                    Some(l) => self.targets.iter().rev().find(|t| t.label.as_deref() == Some(l)),
                };
                let Some(t) = t else {
                    return match label {
                        None => self.err(pos, "continue outside of loop"),
                        Some(l) => self.err(pos, format!("undefined label: {}", l)),
                    };
                };
                if !t.is_loop {
                    return self.err(pos, format!("not a loop label: {}", label.as_deref().unwrap_or("")));
                }
                let id = t.id;
                self.continues[id as usize] = true;
                out.push(TS::Continue(id));
                Ok(false)
            }
            StmtKind::Throw(e) => {
                let v = self.expr(e)?;
                if !self.t.ref_assignable(&v.ty, &Type::class("java/lang/Throwable")) || v.ty == Type::Null {
                    return self.err(
                        e.pos,
                        format!("incompatible types: {} cannot be converted to Throwable", v.ty),
                    );
                }
                out.push(TS::Throw(v));
                Ok(false)
            }
            StmtKind::Switch { scrut, cases, arrow } => self.switch(scrut, cases, *arrow, pos, out),
            StmtKind::Try { body, catches, finally } => {
                if finally.is_some() {
                    return self.err(pos, "finally blocks are not supported yet");
                }
                self.push_scope();
                let mut tb = Vec::new();
                let r = self.block(body, &mut tb);
                self.pop_scope();
                let mut any = r?;
                let mut arms = Vec::new();
                for c in catches {
                    let mut classes = Vec::new();
                    let mut ty = Type::class("java/lang/Throwable");
                    for t in &c.types {
                        let ct = self.resolve_type(t)?;
                        if !self.t.ref_assignable(&ct, &Type::class("java/lang/Throwable")) {
                            return self.err(
                                c.pos,
                                format!("incompatible types: {} cannot be converted to Throwable", ct),
                            );
                        }
                        if c.types.len() == 1 {
                            ty = ct.clone();
                        }
                        classes.push(ct.class_ref_name());
                    }
                    self.push_scope();
                    let r = (|| -> Result<(u16, Vec<TS>, bool)> {
                        let slot = self.declare_local(&c.name, ty, c.pos)?;
                        let mut cb = Vec::new();
                        let cc = self.block(&c.body, &mut cb)?;
                        Ok((slot, cb, cc))
                    })();
                    self.pop_scope();
                    let (slot, cb, cc) = r?;
                    any |= cc;
                    arms.push(CatchArm {
                        classes,
                        slot,
                        body: cb,
                        line: c.pos.line,
                    });
                }
                out.push(TS::Try {
                    body: tb,
                    catches: arms,
                });
                Ok(any)
            }
            StmtKind::CtorCall { .. } => self.err(pos, "call to this/super must be first statement in constructor"),
        }
    }

    fn switch(&mut self, scrut: &Expr, cases: &[SwitchCase], arrow: bool, pos: Pos, out: &mut Vec<TS>) -> Result<bool> {
        let sv = self.expr(scrut)?;
        let is_string = sv.ty.is_string();
        let label_ty = if sv.ty == Type::Char { Type::Char } else { Type::Int };
        if !is_string && !matches!(sv.ty, Type::Int | Type::Char) {
            return self.err(
                scrut.pos,
                format!("switch on {} is not supported (use int, char or String)", sv.ty),
            );
        }
        self.push_scope();
        let r = (|| -> Result<bool> {
            let mut pre = Vec::new();
            // For strings: compute an index first, then switch on it.
            let mut str_labels: Vec<String> = Vec::new();
            let scrut_te = if is_string {
                let tmp = self.temp(Type::string(), pos)?;
                let idx = self.temp(Type::Int, pos)?;
                pre.push(TS::Expr(te(
                    TK::Assign(Box::new(LV::Local(tmp, Type::string())), Box::new(sv)),
                    Type::string(),
                )));
                pre.push(TS::Expr(te(
                    TK::Assign(
                        Box::new(LV::Local(idx, Type::Int)),
                        Box::new(te(TK::Int(-1), Type::Int)),
                    ),
                    Type::Int,
                )));
                for c in cases {
                    for l in &c.labels {
                        let v = self.expr(l)?;
                        let TK::Str(s) = v.kind else {
                            return self.err(l.pos, "constant string expression required");
                        };
                        if str_labels.contains(&s) {
                            return self.err(l.pos, "duplicate case label");
                        }
                        str_labels.push(s);
                    }
                }
                // if/else chain assigning idx
                let mut chain: Option<TS> = None;
                for (i, s) in str_labels.iter().enumerate().rev() {
                    let test = te(
                        TK::Call {
                            kind: Invoke::Virtual,
                            owner: STRING.into(),
                            name: "equals".into(),
                            desc: "(Ljava/lang/Object;)Z".into(),
                            recv: Some(Box::new(te(TK::Local(tmp), Type::string()))),
                            args: vec![te(TK::Str(s.clone()), Type::string())],
                        },
                        Type::Boolean,
                    );
                    let set = TS::Expr(te(
                        TK::Assign(
                            Box::new(LV::Local(idx, Type::Int)),
                            Box::new(te(TK::Int(i as i32), Type::Int)),
                        ),
                        Type::Int,
                    ));
                    chain = Some(TS::If(test, Box::new(set), chain.map(Box::new)));
                }
                if let Some(c) = chain {
                    pre.push(c);
                }
                te(TK::Local(idx), Type::Int)
            } else {
                convert(sv, &Type::Int)
            };

            let id = self.new_target(false, true);
            let mut arms = Vec::new();
            let mut seen: Vec<i32> = Vec::new();
            let mut has_default = false;
            let mut completes = true; // of the last colon-style group
            let mut any_arrow_completes = false;
            let mut str_i = 0usize;
            for (ci, c) in cases.iter().enumerate() {
                let mut values = Vec::new();
                if c.is_default {
                    if has_default {
                        return self.err(c.pos, "duplicate default label");
                    }
                    has_default = true;
                }
                for l in &c.labels {
                    let v = if is_string {
                        let v = str_i as i32;
                        str_i += 1;
                        v
                    } else {
                        let v = self.expr(l)?;
                        match self.coerce(v, &label_ty, l.pos).map(|v| v.kind) {
                            Ok(TK::Int(x)) => x,
                            _ => return self.err(l.pos, "constant expression required"),
                        }
                    };
                    if seen.contains(&v) {
                        return self.err(l.pos, "duplicate case label");
                    }
                    seen.push(v);
                    values.push(v);
                }
                let mut body = Vec::new();
                if arrow {
                    self.push_scope();
                    let r = self.block(&c.body, &mut body);
                    self.pop_scope();
                    if r? {
                        any_arrow_completes = true;
                        body.push(TS::Break(id));
                    }
                } else {
                    let _ = ci;
                    completes = self.block(&c.body, &mut body)?;
                }
                arms.push(SwitchArm {
                    values,
                    is_default: c.is_default,
                    body,
                });
            }
            self.targets.pop();
            pre.push(TS::Switch {
                scrut: scrut_te,
                arms,
                id,
            });
            out.push(TS::Block(pre));
            let brk = self.breaks[id as usize];
            Ok(if arrow {
                any_arrow_completes || !has_default || brk
            } else {
                completes || !has_default || brk || cases.is_empty()
            })
        })();
        self.pop_scope();
        r
    }

    // ---------- members ----------
    fn ctor_body(&mut self, decl: &ClassDecl, body: &[Stmt], pos: Pos) -> Result<(Vec<TS>, bool)> {
        let t = self.t;
        let mut out = vec![TS::Line(pos.line)];
        let (first, rest) = match body.first() {
            Some(Stmt {
                kind: StmtKind::CtorCall { is_super, args },
                pos,
            }) => (Some((*is_super, args, *pos)), &body[1..]),
            _ => (None, body),
        };
        let mut run_inits = true;
        // Arguments of this(...)/super(...) are evaluated in a static-like context.
        match first {
            Some((false, args, cpos)) => {
                out.push(TS::Line(cpos.line));
                run_inits = false;
                let mut targs = Vec::new();
                for a in args {
                    targs.push(self.expr(a)?);
                }
                let cands = t.find_methods(&self.cls.name, "<init>");
                let (_, m, args) = self.select(cands, targs, "<init>", cpos)?;
                out.push(TS::Expr(te(
                    TK::Call {
                        kind: Invoke::Special,
                        owner: self.cls.name.clone(),
                        name: "<init>".into(),
                        desc: m.desc.clone(),
                        recv: Some(Box::new(self.this_te())),
                        args,
                    },
                    Type::Void,
                )));
            }
            _ => {
                let (args, cpos): (&[Expr], Pos) = match first {
                    Some((_, args, p)) => (args.as_slice(), p),
                    None => (&[], pos),
                };
                if first.is_some() {
                    out.push(TS::Line(cpos.line));
                }
                let mut targs = Vec::new();
                for a in args {
                    targs.push(self.expr(a)?);
                }
                let sup = self.cls.super_name.clone().unwrap_or_else(|| OBJECT.into());
                let cands = t.find_methods(&sup, "<init>");
                if cands.is_empty() {
                    return self.err(
                        cpos,
                        format!("jolt does not know the constructors of {}", Type::class(&sup)),
                    );
                }
                let (_, m, args) = match self.select(cands, targs, "<init>", cpos) {
                    Ok(x) => x,
                    Err(mut e) if first.is_none() => {
                        e.msg = format!("implicit super constructor call failed: {}", e.msg);
                        return Err(e);
                    }
                    Err(e) => return Err(e),
                };
                self.check_private(&sup, m.flags, &format!("{}()", Type::class(&sup)), cpos)?;
                out.push(TS::Expr(te(
                    TK::Call {
                        kind: Invoke::Special,
                        owner: sup,
                        name: "<init>".into(),
                        desc: m.desc.clone(),
                        recv: Some(Box::new(self.this_te())),
                        args,
                    },
                    Type::Void,
                )));
            }
        }
        if run_inits {
            for mem in &decl.members {
                match mem {
                    Member::Field(f) if f.mods & m::STATIC == 0 => {
                        if let Some(init) = &f.init {
                            out.push(TS::Line(f.pos.line));
                            let (owner, fi) = t.find_field(&self.cls.name, &f.name).unwrap();
                            let v = match &init.kind {
                                ExprKind::ArrayInit(el) => self.array_init(el, &fi.ty, init.pos)?,
                                _ => {
                                    let v = self.expr(init)?;
                                    self.coerce(v, &fi.ty, init.pos)?
                                }
                            };
                            let lv = LV::Field {
                                obj: self.this_te(),
                                owner,
                                name: f.name.clone(),
                                ty: fi.ty.clone(),
                            };
                            out.push(TS::Expr(te(TK::Assign(Box::new(lv), Box::new(v)), fi.ty.clone())));
                        }
                    }
                    Member::InstanceInit(b, p) => {
                        self.push_scope();
                        let mut v = Vec::new();
                        let r = self.block(b, &mut v);
                        self.pop_scope();
                        if !r? {
                            return self.err(*p, "initializer must be able to complete normally");
                        }
                        out.push(TS::Block(v));
                    }
                    _ => {}
                }
            }
        }
        let c = self.block(rest, &mut out)?;
        Ok((out, c))
    }

    fn method(&mut self, decl: &ClassDecl, md: &MethodDecl, info: &MethodInfo) -> Result<TMethod> {
        self.reset_locals(info.is_static);
        self.ret = info.ret.clone();
        self.targets.clear();
        for (p, ty) in md.params.iter().zip(&info.params) {
            self.declare_local(&p.name, ty.clone(), p.pos)?;
        }
        let body = match &md.body {
            None => None,
            Some(b) => {
                let (mut out, completes) = if md.is_ctor {
                    self.ctor_body(decl, b, md.pos)?
                } else {
                    let mut out = vec![];
                    let c = self.block(b, &mut out)?;
                    (out, c)
                };
                if completes {
                    if info.ret != Type::Void {
                        return self.err(md.end_pos, "missing return statement");
                    }
                    out.push(TS::Line(md.end_pos.line));
                    out.push(TS::Return(None));
                }
                Some(out)
            }
        };
        Ok(TMethod {
            name: md.name.clone(),
            desc: info.desc.clone(),
            flags: self.member_flags(info.flags),
            body,
            max_locals: self.max_slot as u16,
            start_line: md.pos.line,
        })
    }

    fn member_flags(&self, flags: u16) -> u16 {
        if self.cls.in_nest {
            flags & !ACC_PRIVATE
        } else {
            flags
        }
    }
}

/// Evaluate `static final` constant initializers so references can be folded.
pub fn compute_constants(units: &[CompilationUnit], t: &mut ClassTable) {
    let decls = declare::walk(units);
    for _round in 0..4 {
        let mut updates: Vec<(String, String, Const)> = Vec::new();
        for (d, name, file) in &decls {
            let Some(ci) = t.get(name) else { continue };
            for mem in &d.members {
                let Member::Field(f) = mem else { continue };
                if f.mods & (m::STATIC | m::FINAL) != (m::STATIC | m::FINAL) {
                    continue;
                }
                let Some(init) = &f.init else { continue };
                let Some(fi) = ci.fields.iter().find(|x| x.name == f.name) else {
                    continue;
                };
                if fi.constant.is_some() || !(fi.ty.is_primitive() || fi.ty.is_string()) {
                    continue;
                }
                let mut ck = Checker::new(t, ci, *file);
                ck.is_static = true;
                let Ok(v) = ck.expr(init) else { continue };
                let Ok(v) = ck.coerce(v, &fi.ty, init.pos) else {
                    continue;
                };
                let c = match v.kind {
                    TK::Int(x) => Const::Int(x),
                    TK::Long(x) => Const::Long(x),
                    TK::Double(x) => Const::Double(x),
                    TK::Str(s) => Const::Str(s),
                    _ => continue,
                };
                updates.push((name.clone(), f.name.clone(), c));
            }
        }
        if updates.is_empty() {
            break;
        }
        for (cls, field, c) in updates {
            if let Some(ci) = t.classes.get_mut(&cls) {
                if let Some(f) = ci.fields.iter_mut().find(|f| f.name == field) {
                    f.constant = Some(c);
                }
            }
        }
    }
}

/// Type-check one class and lower it to typed IR.
pub fn check_class(
    t: &ClassTable,
    d: &ClassDecl,
    name: &str,
    file: usize,
    source_file: &str,
) -> std::result::Result<TClass, Vec<Diag>> {
    let ci = t.get(name).unwrap();
    let mut ck = Checker::new(t, ci, file);
    let mut diags = Vec::new();
    let mut methods = Vec::new();
    let mut fields = Vec::new();

    // Abstract-method implementation check for concrete classes.
    if !ci.is_abstract {
        let mut abstracts: Vec<(String, &MethodInfo)> = Vec::new();
        let mut stack: Vec<String> = Vec::new();
        let mut cur = ci.super_name.clone();
        stack.extend(ci.interfaces.iter().cloned());
        while let Some(c) = cur {
            if let Some(sc) = t.get(&c) {
                for m in &sc.methods {
                    if m.flags & ACC_ABSTRACT != 0 {
                        abstracts.push((c.clone(), m));
                    }
                }
                stack.extend(sc.interfaces.iter().cloned());
                cur = sc.super_name.clone();
            } else {
                break;
            }
        }
        let mut seen = Vec::new();
        while let Some(i) = stack.pop() {
            if seen.contains(&i) {
                continue;
            }
            seen.push(i.clone());
            if let Some(ic) = t.get(&i) {
                for m in &ic.methods {
                    if m.flags & ACC_ABSTRACT != 0 {
                        abstracts.push((i.clone(), m));
                    }
                }
                stack.extend(ic.interfaces.iter().cloned());
            }
        }
        for (owner, am) in abstracts {
            let impls = t.find_methods(name, &am.name);
            let found = impls
                .iter()
                .find(|(_, m)| m.params == am.params && m.flags & ACC_ABSTRACT == 0);
            match found {
                None => {
                    // JDK abstract methods are not fully modelled, so only user ones are enforced.
                    if t.get(&owner).map_or(false, |c| c.user) {
                        diags.push(Diag {
                            file,
                            pos: d.pos,
                            msg: format!(
                                "{} is not abstract and does not override abstract method {}({}) in {}",
                                d.name,
                                am.name,
                                am.params.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(","),
                                Type::class(&owner)
                            ),
                        });
                    }
                }
                Some((_, m)) => {
                    if t.get(&owner).map_or(false, |c| c.is_interface) && m.flags & ACC_PUBLIC == 0 {
                        let pos = d
                            .members
                            .iter()
                            .find_map(|mm| match mm {
                                Member::Method(md) if md.name == am.name => Some(md.pos),
                                _ => None,
                            })
                            .unwrap_or(d.pos);
                        diags.push(Diag {
                            file,
                            pos,
                            msg: format!("{}() in {} cannot implement {}() in {}: attempting to assign weaker access privileges; was public", am.name, d.name, am.name, Type::class(&owner)),
                        });
                    }
                }
            }
        }
    }

    for f in &ci.fields {
        fields.push(TField {
            name: f.name.clone(),
            desc: f.ty.descriptor(),
            flags: ck.member_flags(f.flags),
        });
    }

    let mut has_ctor = false;
    // The declaration pass registered methods in member order.
    let mut mi = 0usize;
    for mem in &d.members {
        if let Member::Method(md) = mem {
            has_ctor |= md.is_ctor;
            let info = &ci.methods[mi];
            mi += 1;
            match ck.method(d, md, info) {
                Ok(m) => methods.push(m),
                Err(e) => diags.push(e),
            }
        }
    }
    if !has_ctor && !d.is_interface {
        let info = ci.methods.iter().find(|m| m.name == "<init>").unwrap();
        let md = MethodDecl {
            mods: 0,
            ret: AstType::Prim(Type::Void),
            name: "<init>".into(),
            params: vec![],
            varargs: false,
            body: Some(vec![]),
            pos: d.pos,
            is_ctor: true,
            end_pos: d.pos,
        };
        match ck.method(d, &md, info) {
            Ok(m) => methods.push(m),
            Err(e) => diags.push(e),
        }
    }

    // Static initializer.
    ck.reset_locals(true);
    ck.ret = Type::Void;
    let mut clinit = Vec::new();
    let mut clinit_ok = true;
    for mem in &d.members {
        match mem {
            Member::Field(f) if f.mods & m::STATIC != 0 => {
                if let Some(init) = &f.init {
                    let fi = ci.fields.iter().find(|x| x.name == f.name).unwrap();
                    clinit.push(TS::Line(f.pos.line));
                    let v = match &init.kind {
                        ExprKind::ArrayInit(el) => ck.array_init(el, &fi.ty, init.pos),
                        _ => ck.expr(init).and_then(|v| ck.coerce(v, &fi.ty, init.pos)),
                    };
                    match v {
                        Ok(v) => {
                            let lv = LV::Static {
                                owner: name.to_string(),
                                name: f.name.clone(),
                                ty: fi.ty.clone(),
                            };
                            clinit.push(TS::Expr(te(TK::Assign(Box::new(lv), Box::new(v)), fi.ty.clone())));
                        }
                        Err(e) => {
                            diags.push(e);
                            clinit_ok = false;
                        }
                    }
                }
            }
            Member::StaticInit(b, p) => {
                ck.push_scope();
                let mut v = Vec::new();
                let r = ck.block(b, &mut v);
                ck.pop_scope();
                match r {
                    Ok(true) => clinit.push(TS::Block(v)),
                    Ok(false) => diags.push(Diag {
                        file,
                        pos: *p,
                        msg: "initializer must be able to complete normally".into(),
                    }),
                    Err(e) => {
                        diags.push(e);
                        clinit_ok = false;
                    }
                }
            }
            Member::InstanceInit(_, p) if d.is_interface => {
                diags.push(Diag {
                    file,
                    pos: *p,
                    msg: "interfaces cannot have initializers".into(),
                });
            }
            _ => {}
        }
    }
    if clinit_ok && !clinit.is_empty() {
        clinit.push(TS::Return(None));
        methods.push(TMethod {
            name: "<clinit>".into(),
            desc: "()V".into(),
            flags: ACC_STATIC,
            body: Some(clinit),
            max_locals: ck.max_slot as u16,
            start_line: d.pos.line,
        });
    }

    if !diags.is_empty() {
        return Err(diags);
    }

    let mut flags: u16 = 0;
    if d.mods & (m::PUBLIC | m::PROTECTED) != 0 {
        flags |= ACC_PUBLIC;
    }
    if d.mods & m::FINAL != 0 {
        flags |= 0x0010;
    }
    if d.is_interface {
        flags |= 0x0200 | ACC_ABSTRACT;
    } else {
        flags |= 0x0020; // ACC_SUPER
        if ci.is_abstract {
            flags |= ACC_ABSTRACT;
        }
    }
    Ok(TClass {
        name: name.to_string(),
        super_name: ci.super_name.clone().unwrap_or_else(|| OBJECT.into()),
        interfaces: ci.interfaces.clone(),
        flags,
        fields,
        methods,
        source_file: source_file.to_string(),
    })
}
