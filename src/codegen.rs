//! Bytecode generation from the typed IR.

use crate::fxhash::FxMap;

use crate::ast::BinOp;
use crate::check::const_bool;
use crate::classfile::*;
use crate::tir::*;
use crate::types::Type;

mod op {
    pub const ACONST_NULL: u8 = 1;
    pub const ICONST_0: u8 = 3;
    pub const LCONST_0: u8 = 9;
    pub const DCONST_0: u8 = 14;
    pub const BIPUSH: u8 = 16;
    pub const SIPUSH: u8 = 17;
    pub const LDC: u8 = 18;
    pub const LDC_W: u8 = 19;
    pub const LDC2_W: u8 = 20;
    pub const POP: u8 = 87;
    pub const POP2: u8 = 88;
    pub const DUP: u8 = 89;
    pub const DUP_X1: u8 = 90;
    pub const DUP_X2: u8 = 91;
    pub const DUP2: u8 = 92;
    pub const DUP2_X1: u8 = 93;
    pub const DUP2_X2: u8 = 94;
    pub const SWAP: u8 = 95;
    pub const IINC: u8 = 132;
    pub const I2L: u8 = 133;
    pub const I2D: u8 = 135;
    pub const L2I: u8 = 136;
    pub const L2D: u8 = 138;
    pub const D2I: u8 = 142;
    pub const D2L: u8 = 143;
    pub const I2C: u8 = 146;
    pub const LCMP: u8 = 148;
    pub const DCMPL: u8 = 151;
    pub const DCMPG: u8 = 152;
    pub const IFEQ: u8 = 153;
    pub const IFNE: u8 = 154;
    pub const IF_ICMPEQ: u8 = 159;
    pub const IF_ACMPEQ: u8 = 165;
    pub const IF_ACMPNE: u8 = 166;
    pub const GOTO: u8 = 167;
    pub const TABLESWITCH: u8 = 170;
    pub const LOOKUPSWITCH: u8 = 171;
    pub const RETURN: u8 = 177;
    pub const GETSTATIC: u8 = 178;
    pub const PUTSTATIC: u8 = 179;
    pub const GETFIELD: u8 = 180;
    pub const PUTFIELD: u8 = 181;
    pub const INVOKEVIRTUAL: u8 = 182;
    pub const INVOKESPECIAL: u8 = 183;
    pub const INVOKESTATIC: u8 = 184;
    pub const INVOKEINTERFACE: u8 = 185;
    pub const NEW: u8 = 187;
    pub const NEWARRAY: u8 = 188;
    pub const ANEWARRAY: u8 = 189;
    pub const ARRAYLENGTH: u8 = 190;
    pub const ATHROW: u8 = 191;
    pub const CHECKCAST: u8 = 192;
    pub const INSTANCEOF: u8 = 193;
    pub const WIDE: u8 = 196;
    pub const MULTIANEWARRAY: u8 = 197;
    pub const IFNULL: u8 = 198;
    pub const IFNONNULL: u8 = 199;
}

/// 0 = int-like, 1 = long, 2 = double, 3 = reference
fn kind(t: &Type) -> usize {
    match t {
        Type::Boolean | Type::Char | Type::Int => 0,
        Type::Long => 1,
        Type::Double => 2,
        _ => 3,
    }
}

#[derive(Clone, Copy, Default)]
struct Label {
    pos: Option<u32>,
    stack: Option<i32>,
    used: bool,
}

struct Fixup {
    instr: usize,
    at: usize,
    label: usize,
    wide: bool,
}

struct Gen<'a> {
    cp: &'a mut ConstPool,
    code: Vec<u8>,
    stack: i32,
    max_stack: i32,
    labels: Vec<Label>,
    fixups: Vec<Fixup>,
    targets: FxMap<u32, (usize, usize)>,
    exc: Vec<(usize, usize, usize, u16)>,
    lines: Vec<(u16, u16)>,
    reachable: bool,
}

fn cmp_index(op: BinOp) -> u8 {
    match op {
        BinOp::Eq => 0,
        BinOp::Ne => 1,
        BinOp::Lt => 2,
        BinOp::Ge => 3,
        BinOp::Gt => 4,
        BinOp::Le => 5,
        _ => unreachable!(),
    }
}

fn negate(op: BinOp) -> BinOp {
    match op {
        BinOp::Eq => BinOp::Ne,
        BinOp::Ne => BinOp::Eq,
        BinOp::Lt => BinOp::Ge,
        BinOp::Ge => BinOp::Lt,
        BinOp::Gt => BinOp::Le,
        BinOp::Le => BinOp::Gt,
        _ => unreachable!(),
    }
}

fn args_slots(desc: &str) -> (i32, i32) {
    let (params, ret) = crate::types::parse_method_descriptor(desc).unwrap();
    (params.iter().map(|p| p.slots()).sum(), ret.slots())
}

fn append_desc(t: &Type) -> &'static str {
    match t {
        Type::Boolean => "(Z)Ljava/lang/StringBuilder;",
        Type::Char => "(C)Ljava/lang/StringBuilder;",
        Type::Int => "(I)Ljava/lang/StringBuilder;",
        Type::Long => "(J)Ljava/lang/StringBuilder;",
        Type::Double => "(D)Ljava/lang/StringBuilder;",
        t if t.is_string() => "(Ljava/lang/String;)Ljava/lang/StringBuilder;",
        _ => "(Ljava/lang/Object;)Ljava/lang/StringBuilder;",
    }
}

const SB: &str = "java/lang/StringBuilder";

impl<'a> Gen<'a> {
    fn new(cp: &'a mut ConstPool) -> Self {
        Gen {
            cp,
            code: Vec::with_capacity(256),
            stack: 0,
            max_stack: 0,
            labels: Vec::new(),
            fixups: Vec::new(),
            targets: FxMap::default(),
            exc: Vec::new(),
            lines: Vec::new(),
            reachable: true,
        }
    }

    // ---------- low level ----------
    fn adjust(&mut self, d: i32) {
        self.stack += d;
        if self.stack > self.max_stack {
            self.max_stack = self.stack;
        }
    }
    fn op(&mut self, o: u8, d: i32) {
        self.code.push(o);
        self.adjust(d);
    }
    fn op_u8(&mut self, o: u8, x: u8, d: i32) {
        self.code.push(o);
        self.code.push(x);
        self.adjust(d);
    }
    fn op_u16(&mut self, o: u8, x: u16, d: i32) {
        self.code.push(o);
        self.code.extend_from_slice(&x.to_be_bytes());
        self.adjust(d);
    }
    fn new_label(&mut self) -> usize {
        self.labels.push(Label::default());
        self.labels.len() - 1
    }
    fn jump(&mut self, o: u8, l: usize) {
        let pop = match o {
            op::GOTO => 0,
            153..=158 | op::IFNULL | op::IFNONNULL => 1,
            _ => 2,
        };
        self.stack -= pop;
        let instr = self.code.len();
        self.code.push(o);
        self.code.extend_from_slice(&[0, 0]);
        self.fixups.push(Fixup {
            instr,
            at: instr + 1,
            label: l,
            wide: false,
        });
        let lab = &mut self.labels[l];
        lab.used = true;
        if lab.stack.is_none() {
            lab.stack = Some(self.stack);
        }
        if o == op::GOTO {
            self.reachable = false;
        }
    }
    fn place(&mut self, l: usize) {
        let lab = &mut self.labels[l];
        lab.pos = Some(self.code.len() as u32);
        if lab.used {
            if !self.reachable {
                self.stack = lab.stack.unwrap_or(0);
            }
            self.reachable = true;
        } else if self.reachable {
            lab.stack = Some(self.stack);
        }
    }
    /// Record a position without affecting reachability.
    fn mark(&mut self, l: usize) {
        self.labels[l].pos = Some(self.code.len() as u32);
    }

    fn line(&mut self, line: u32) {
        let pc = self.code.len() as u16;
        let line = line.min(65535) as u16;
        if let Some(last) = self.lines.last_mut() {
            if last.0 == pc {
                last.1 = line;
                return;
            }
            if last.1 == line {
                return;
            }
        }
        self.lines.push((pc, line));
    }

    fn ldc(&mut self, idx: u16, d: i32) {
        if idx < 256 {
            self.op_u8(op::LDC, idx as u8, d);
        } else {
            self.op_u16(op::LDC_W, idx, d);
        }
    }

    fn iconst(&mut self, v: i32) {
        match v {
            -1..=5 => self.op((op::ICONST_0 as i32 + v) as u8, 1),
            -128..=127 => self.op_u8(op::BIPUSH, v as i8 as u8, 1),
            -32768..=32767 => self.op_u16(op::SIPUSH, v as i16 as u16, 1),
            _ => {
                let i = self.cp.int(v);
                self.ldc(i, 1);
            }
        }
    }

    fn load(&mut self, t: &Type, slot: u16) {
        let k = kind(t);
        let base = [21u8, 22, 24, 25][k];
        let short = [26u8, 30, 38, 42][k];
        self.local_op(base, short, slot, t.slots());
    }
    fn store(&mut self, t: &Type, slot: u16) {
        let k = kind(t);
        let base = [54u8, 55, 57, 58][k];
        let short = [59u8, 63, 71, 75][k];
        self.local_op(base, short, slot, -t.slots());
    }
    fn local_op(&mut self, base: u8, short: u8, slot: u16, d: i32) {
        if slot <= 3 {
            self.op(short + slot as u8, d);
        } else if slot <= 255 {
            self.op_u8(base, slot as u8, d);
        } else {
            self.code.push(op::WIDE);
            self.op_u16(base, slot, d);
        }
    }

    fn array_load(&mut self, elem: &Type) {
        let o = match elem {
            Type::Boolean => 51,
            Type::Char => 52,
            Type::Int => 46,
            Type::Long => 47,
            Type::Double => 49,
            _ => 50,
        };
        self.op(o, -2 + elem.slots());
    }
    fn array_store(&mut self, elem: &Type) {
        let o = match elem {
            Type::Boolean => 84,
            Type::Char => 85,
            Type::Int => 79,
            Type::Long => 80,
            Type::Double => 82,
            _ => 83,
        };
        self.op(o, -2 - elem.slots());
    }

    fn pop(&mut self, t: &Type) {
        match t.slots() {
            0 => {}
            1 => self.op(op::POP, -1),
            _ => self.op(op::POP2, -2),
        }
    }
    fn dup(&mut self, t: &Type) {
        if t.slots() == 2 {
            self.op(op::DUP2, 2)
        } else {
            self.op(op::DUP, 1)
        }
    }

    fn convert(&mut self, from: &Type, to: &Type) {
        use Type::*;
        match (from, to) {
            (a, b) if a == b => {}
            (Char | Int | Boolean, Long) => self.op(op::I2L, 1),
            (Char | Int, Double) => self.op(op::I2D, 1),
            (Char, Int) => {}
            (Int, Char) => self.op(op::I2C, 0),
            (Long, Int) => self.op(op::L2I, -1),
            (Long, Double) => self.op(op::L2D, 0),
            (Long, Char) => {
                self.op(op::L2I, -1);
                self.op(op::I2C, 0);
            }
            (Double, Int) => self.op(op::D2I, -1),
            (Double, Long) => self.op(op::D2L, 0),
            (Double, Char) => {
                self.op(op::D2I, -1);
                self.op(op::I2C, 0);
            }
            _ => {}
        }
    }

    fn arith_op(&mut self, o: BinOp, t: &Type) {
        let k = match t {
            Type::Long => 1,
            Type::Double => 3,
            _ => 0,
        };
        let (code, d): (u8, i32) = match o {
            BinOp::Add => (96 + k, if k == 0 { -1 } else { -2 }),
            BinOp::Sub => (100 + k, if k == 0 { -1 } else { -2 }),
            BinOp::Mul => (104 + k, if k == 0 { -1 } else { -2 }),
            BinOp::Div => (108 + k, if k == 0 { -1 } else { -2 }),
            BinOp::Rem => (112 + k, if k == 0 { -1 } else { -2 }),
            BinOp::Shl => (120 + k, -1),
            BinOp::Shr => (122 + k, -1),
            BinOp::UShr => (124 + k, -1),
            BinOp::BitAnd => (126 + k, if k == 0 { -1 } else { -2 }),
            BinOp::BitOr => (128 + k, if k == 0 { -1 } else { -2 }),
            BinOp::BitXor => (130 + k, if k == 0 { -1 } else { -2 }),
            _ => unreachable!(),
        };
        self.op(code, d);
    }

    fn invoke(&mut self, kind: Invoke, owner: &str, name: &str, desc: &str) {
        let (a, r) = args_slots(desc);
        let recv = if kind == Invoke::Static { 0 } else { 1 };
        let d = -a - recv + r;
        match kind {
            Invoke::Static => {
                let i = self.cp.method(owner, name, desc);
                self.op_u16(op::INVOKESTATIC, i, d);
            }
            Invoke::Virtual => {
                let i = self.cp.method(owner, name, desc);
                self.op_u16(op::INVOKEVIRTUAL, i, d);
            }
            Invoke::Special => {
                let i = self.cp.method(owner, name, desc);
                self.op_u16(op::INVOKESPECIAL, i, d);
            }
            Invoke::Interface => {
                let i = self.cp.imethod(owner, name, desc);
                self.op_u16(op::INVOKEINTERFACE, i, d);
                self.code.push((a + 1) as u8);
                self.code.push(0);
            }
        }
    }

    // ---------- expressions ----------
    fn expr(&mut self, e: &TE) {
        match &e.kind {
            TK::Int(v) => self.iconst(*v),
            TK::Long(v) => {
                if *v == 0 || *v == 1 {
                    self.op(op::LCONST_0 + *v as u8, 2);
                } else {
                    let i = self.cp.long(*v);
                    self.op_u16(op::LDC2_W, i, 2);
                }
            }
            TK::Double(v) => {
                if v.to_bits() == 0 {
                    self.op(op::DCONST_0, 2);
                } else if *v == 1.0 {
                    self.op(op::DCONST_0 + 1, 2);
                } else {
                    let i = self.cp.double(*v);
                    self.op_u16(op::LDC2_W, i, 2);
                }
            }
            TK::Str(s) => {
                let i = self.cp.string(s);
                self.ldc(i, 1);
            }
            TK::Null => self.op(op::ACONST_NULL, 1),
            TK::Local(s) => self.load(&e.ty, *s),
            TK::This => self.op(42, 1),
            TK::GetStatic { owner, name } => {
                let i = self.cp.field(owner, name, &e.ty.descriptor());
                self.op_u16(op::GETSTATIC, i, e.ty.slots());
            }
            TK::GetField { obj, owner, name } => {
                self.expr(obj);
                let i = self.cp.field(owner, name, &e.ty.descriptor());
                self.op_u16(op::GETFIELD, i, e.ty.slots() - 1);
            }
            TK::ArrayLoad { arr, idx } => {
                self.expr(arr);
                self.expr(idx);
                self.array_load(&e.ty);
            }
            TK::ArrayLen(a) => {
                self.expr(a);
                self.op(op::ARRAYLENGTH, 0);
            }
            TK::Call {
                kind,
                owner,
                name,
                desc,
                recv,
                args,
            } => {
                if let Some(r) = recv {
                    self.expr(r);
                }
                for a in args {
                    self.expr(a);
                }
                self.invoke(*kind, owner, name, desc);
            }
            TK::New { class, desc, args } => {
                let c = self.cp.class(class);
                self.op_u16(op::NEW, c, 1);
                self.op(op::DUP, 1);
                for a in args {
                    self.expr(a);
                }
                self.invoke(Invoke::Special, class, "<init>", desc);
            }
            TK::NewArray(dims) => {
                for d in dims {
                    self.expr(d);
                }
                if dims.len() == 1 {
                    let Type::Array(elem) = &e.ty else { unreachable!() };
                    self.new_array(elem);
                } else {
                    let c = self.cp.class(&e.ty.class_ref_name());
                    self.op_u16(op::MULTIANEWARRAY, c, 1 - dims.len() as i32);
                    self.code.push(dims.len() as u8);
                }
            }
            TK::ArrayLit(elems) => {
                let Type::Array(elem) = &e.ty else { unreachable!() };
                self.iconst(elems.len() as i32);
                self.new_array(elem);
                for (i, x) in elems.iter().enumerate() {
                    self.op(op::DUP, 1);
                    self.iconst(i as i32);
                    self.expr(x);
                    self.array_store(elem);
                }
            }
            TK::Neg(x) => {
                self.expr(x);
                let o = match e.ty {
                    Type::Long => 117,
                    Type::Double => 119,
                    _ => 116,
                };
                self.op(o, 0);
            }
            TK::Arith(o, l, r) => {
                self.expr(l);
                self.expr(r);
                self.arith_op(*o, &e.ty);
            }
            TK::Cmp(..) | TK::And(..) | TK::Or(..) | TK::Not(..) => self.bool_value(e),
            TK::InstanceOf(x, t) => {
                self.expr(x);
                let c = self.cp.class(&t.class_ref_name());
                self.op_u16(op::INSTANCEOF, c, 0);
            }
            TK::Concat(parts) => {
                self.concat_start(parts.first());
                let skip = usize::from(matches!(parts.first(), Some(TE { kind: TK::Str(_), .. })));
                for p in &parts[skip..] {
                    self.append(p);
                }
                self.sb_to_string();
            }
            TK::Assign(..) | TK::CompoundAssign { .. } | TK::StrAppendAssign { .. } | TK::IncDec { .. } => {
                self.assign_like(e, true);
            }
            TK::Cond(c, a, b) => {
                let else_l = self.new_label();
                let end = self.new_label();
                self.cond(c, false, else_l);
                self.expr(a);
                self.jump(op::GOTO, end);
                self.place(else_l);
                self.expr(b);
                self.place(end);
            }
            TK::Convert(x) => {
                self.expr(x);
                self.convert(&x.ty, &e.ty);
            }
            TK::CheckCast(x) => {
                self.expr(x);
                let c = self.cp.class(&e.ty.class_ref_name());
                self.op_u16(op::CHECKCAST, c, 0);
            }
        }
    }

    fn new_array(&mut self, elem: &Type) {
        let atype = match elem {
            Type::Boolean => 4,
            Type::Char => 5,
            Type::Double => 7,
            Type::Int => 10,
            Type::Long => 11,
            _ => 0,
        };
        if atype != 0 {
            self.op_u8(op::NEWARRAY, atype, 0);
        } else {
            let c = self.cp.class(&elem.class_ref_name());
            self.op_u16(op::ANEWARRAY, c, 0);
        }
    }

    fn concat_start(&mut self, first: Option<&TE>) {
        let c = self.cp.class(SB);
        self.op_u16(op::NEW, c, 1);
        self.op(op::DUP, 1);
        if let Some(TE { kind: TK::Str(s), .. }) = first {
            let i = self.cp.string(s);
            self.ldc(i, 1);
            self.invoke(Invoke::Special, SB, "<init>", "(Ljava/lang/String;)V");
        } else {
            self.invoke(Invoke::Special, SB, "<init>", "()V");
        }
    }
    fn append(&mut self, p: &TE) {
        self.expr(p);
        self.invoke(Invoke::Virtual, SB, "append", append_desc(&p.ty));
    }
    fn sb_to_string(&mut self) {
        self.invoke(Invoke::Virtual, SB, "toString", "()Ljava/lang/String;");
    }

    fn bool_value(&mut self, e: &TE) {
        let f = self.new_label();
        let end = self.new_label();
        self.cond(e, false, f);
        self.iconst(1);
        self.jump(op::GOTO, end);
        self.place(f);
        self.iconst(0);
        self.place(end);
    }

    /// Jump to `target` if `e` evaluates to `jump_if`; fall through otherwise.
    fn cond(&mut self, e: &TE, jump_if: bool, target: usize) {
        if let Some(b) = const_bool(e) {
            if b == jump_if {
                self.jump(op::GOTO, target);
            }
            return;
        }
        match &e.kind {
            TK::Not(x) => self.cond(x, !jump_if, target),
            TK::And(a, b) => {
                if jump_if {
                    let skip = self.new_label();
                    self.cond(a, false, skip);
                    self.cond(b, true, target);
                    self.place(skip);
                } else {
                    self.cond(a, false, target);
                    self.cond(b, false, target);
                }
            }
            TK::Or(a, b) => {
                if jump_if {
                    self.cond(a, true, target);
                    self.cond(b, true, target);
                } else {
                    let skip = self.new_label();
                    self.cond(a, true, skip);
                    self.cond(b, false, target);
                    self.place(skip);
                }
            }
            TK::Cmp(o, l, r) => {
                let o = if jump_if { *o } else { negate(*o) };
                match kind(&l.ty) {
                    0 => {
                        if matches!(r.kind, TK::Int(0)) {
                            self.expr(l);
                            self.jump(op::IFEQ + cmp_index(o), target);
                        } else {
                            self.expr(l);
                            self.expr(r);
                            self.jump(op::IF_ICMPEQ + cmp_index(o), target);
                        }
                    }
                    1 => {
                        self.expr(l);
                        self.expr(r);
                        self.op(op::LCMP, -3);
                        self.jump(op::IFEQ + cmp_index(o), target);
                    }
                    2 => {
                        self.expr(l);
                        self.expr(r);
                        // NaN must make <, <=, >, >= false: pick the compare
                        // variant whose NaN result fails the original test.
                        let orig = if jump_if { o } else { negate(o) };
                        let c = if matches!(orig, BinOp::Lt | BinOp::Le) {
                            op::DCMPG
                        } else {
                            op::DCMPL
                        };
                        self.op(c, -3);
                        self.jump(op::IFEQ + cmp_index(o), target);
                    }
                    _ => {
                        let eq = o == BinOp::Eq;
                        if matches!(r.kind, TK::Null) {
                            self.expr(l);
                            self.jump(if eq { op::IFNULL } else { op::IFNONNULL }, target);
                        } else if matches!(l.kind, TK::Null) {
                            self.expr(r);
                            self.jump(if eq { op::IFNULL } else { op::IFNONNULL }, target);
                        } else {
                            self.expr(l);
                            self.expr(r);
                            self.jump(if eq { op::IF_ACMPEQ } else { op::IF_ACMPNE }, target);
                        }
                    }
                }
            }
            _ => {
                self.expr(e);
                self.jump(if jump_if { op::IFNE } else { op::IFEQ }, target);
            }
        }
    }

    // ---------- assignments ----------
    /// Push the object/array+index part of an lvalue.
    fn lv_prepare(&mut self, lv: &LV) {
        match lv {
            LV::Field { obj, .. } => self.expr(obj),
            LV::Array { arr, idx, .. } => {
                self.expr(arr);
                self.expr(idx);
            }
            _ => {}
        }
    }
    fn lv_dup_prepared(&mut self, lv: &LV) {
        match lv {
            LV::Field { .. } => self.op(op::DUP, 1),
            LV::Array { .. } => self.op(op::DUP2, 2),
            _ => {}
        }
    }
    fn lv_load(&mut self, lv: &LV) {
        match lv {
            LV::Local(s, t) => self.load(t, *s),
            LV::Static { owner, name, ty } => {
                let i = self.cp.field(owner, name, &ty.descriptor());
                self.op_u16(op::GETSTATIC, i, ty.slots());
            }
            LV::Field { owner, name, ty, .. } => {
                let i = self.cp.field(owner, name, &ty.descriptor());
                self.op_u16(op::GETFIELD, i, ty.slots() - 1);
            }
            LV::Array { ty, .. } => self.array_load(ty),
        }
    }
    /// Duplicate the value on top of the stack beneath the prepared lvalue parts.
    fn lv_dup_value(&mut self, lv: &LV) {
        let wide = lv.ty().slots() == 2;
        match lv {
            LV::Local(..) | LV::Static { .. } => self.dup(lv.ty()),
            LV::Field { .. } => {
                if wide {
                    self.op(op::DUP2_X1, 2)
                } else {
                    self.op(op::DUP_X1, 1)
                }
            }
            LV::Array { .. } => {
                if wide {
                    self.op(op::DUP2_X2, 2)
                } else {
                    self.op(op::DUP_X2, 1)
                }
            }
        }
    }
    fn lv_store(&mut self, lv: &LV) {
        match lv {
            LV::Local(s, t) => self.store(t, *s),
            LV::Static { owner, name, ty } => {
                let i = self.cp.field(owner, name, &ty.descriptor());
                self.op_u16(op::PUTSTATIC, i, -ty.slots());
            }
            LV::Field { owner, name, ty, .. } => {
                let i = self.cp.field(owner, name, &ty.descriptor());
                self.op_u16(op::PUTFIELD, i, -ty.slots() - 1);
            }
            LV::Array { ty, .. } => self.array_store(ty),
        }
    }

    fn assign_like(&mut self, e: &TE, need: bool) {
        match &e.kind {
            TK::Assign(lv, v) => {
                self.lv_prepare(lv);
                self.expr(v);
                if need {
                    self.lv_dup_value(lv);
                }
                self.lv_store(lv);
            }
            TK::CompoundAssign {
                target,
                op: o,
                value,
                op_ty,
            } => {
                let lt = target.ty().clone();
                // iinc fast path: int local += constant
                if let (LV::Local(s, Type::Int), TK::Int(c), BinOp::Add | BinOp::Sub) = (&**target, &value.kind, o) {
                    let c = if *o == BinOp::Sub { c.wrapping_neg() } else { *c };
                    if (-32768..=32767).contains(&c) {
                        self.iinc(*s, c);
                        if need {
                            self.load(&Type::Int, *s);
                        }
                        return;
                    }
                }
                self.lv_prepare(target);
                self.lv_dup_prepared(target);
                self.lv_load(target);
                self.convert(&lt, op_ty);
                self.expr(value);
                self.arith_op(*o, op_ty);
                self.convert(op_ty, &lt);
                if need {
                    self.lv_dup_value(target);
                }
                self.lv_store(target);
            }
            TK::StrAppendAssign { target, parts } => {
                self.lv_prepare(target);
                self.lv_dup_prepared(target);
                self.lv_load(target);
                let c = self.cp.class(SB);
                self.op_u16(op::NEW, c, 1);
                self.op(op::DUP, 1);
                self.invoke(Invoke::Special, SB, "<init>", "()V");
                self.op(op::SWAP, 0);
                self.invoke(
                    Invoke::Virtual,
                    SB,
                    "append",
                    "(Ljava/lang/String;)Ljava/lang/StringBuilder;",
                );
                for p in parts {
                    self.append(p);
                }
                self.sb_to_string();
                if need {
                    self.lv_dup_value(target);
                }
                self.lv_store(target);
            }
            TK::IncDec { target, pre, inc } => {
                let t = target.ty().clone();
                if let LV::Local(s, Type::Int) = &**target {
                    let s = *s;
                    if need && !*pre {
                        self.load(&Type::Int, s);
                    }
                    self.iinc(s, if *inc { 1 } else { -1 });
                    if need && *pre {
                        self.load(&Type::Int, s);
                    }
                    return;
                }
                self.lv_prepare(target);
                self.lv_dup_prepared(target);
                self.lv_load(target);
                if need && !*pre {
                    self.lv_dup_value(target);
                }
                let op_ty = if t == Type::Char { Type::Int } else { t.clone() };
                match op_ty {
                    Type::Long => self.op(op::LCONST_0 + 1, 2),
                    Type::Double => self.op(op::DCONST_0 + 1, 2),
                    _ => self.iconst(1),
                }
                self.arith_op(if *inc { BinOp::Add } else { BinOp::Sub }, &op_ty);
                self.convert(&op_ty, &t);
                if need && *pre {
                    self.lv_dup_value(target);
                }
                self.lv_store(target);
            }
            _ => unreachable!(),
        }
    }

    fn iinc(&mut self, slot: u16, c: i32) {
        if slot <= 255 && (-128..=127).contains(&c) {
            self.code.extend_from_slice(&[op::IINC, slot as u8, c as i8 as u8]);
        } else {
            self.code.push(op::WIDE);
            self.code.push(op::IINC);
            self.code.extend_from_slice(&slot.to_be_bytes());
            self.code.extend_from_slice(&(c as i16).to_be_bytes());
        }
    }

    fn discard(&mut self, e: &TE) {
        match &e.kind {
            TK::Assign(..) | TK::CompoundAssign { .. } | TK::StrAppendAssign { .. } | TK::IncDec { .. } => {
                self.assign_like(e, false)
            }
            _ => {
                self.expr(e);
                self.pop(&e.ty);
            }
        }
    }

    // ---------- statements ----------
    fn stmts(&mut self, v: &[TS]) {
        for s in v {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &TS) {
        if !self.reachable {
            return;
        }
        match s {
            TS::Line(l) => self.line(*l),
            TS::Expr(e) => self.discard(e),
            TS::Block(v) => self.stmts(v),
            TS::If(c, t, e) => {
                if let Some(b) = const_bool(c) {
                    if b {
                        self.stmt(t);
                    } else if let Some(e) = e {
                        self.stmt(e);
                    }
                    return;
                }
                let else_l = self.new_label();
                self.cond(c, false, else_l);
                self.stmt(t);
                match e {
                    Some(e) => {
                        let end = self.new_label();
                        if self.reachable {
                            self.jump(op::GOTO, end);
                        }
                        self.place(else_l);
                        self.stmt(e);
                        self.place(end);
                    }
                    None => self.place(else_l),
                }
            }
            TS::While { cond, body, id } => {
                if const_bool(cond) == Some(false) {
                    return;
                }
                let start = self.new_label();
                let end = self.new_label();
                self.targets.insert(*id, (end, start));
                self.place(start);
                self.cond(cond, false, end);
                self.stmt(body);
                if self.reachable {
                    self.jump(op::GOTO, start);
                }
                self.place(end);
            }
            TS::DoWhile { body, cond, id } => {
                let start = self.new_label();
                let cont = self.new_label();
                let end = self.new_label();
                self.targets.insert(*id, (end, cont));
                self.place(start);
                self.stmt(body);
                self.place(cont);
                if self.reachable {
                    self.cond(cond, true, start);
                }
                self.place(end);
            }
            TS::For { cond, update, body, id } => {
                let start = self.new_label();
                let cont = self.new_label();
                let end = self.new_label();
                self.targets.insert(*id, (end, cont));
                if let Some(c) = cond {
                    if const_bool(c) == Some(false) {
                        return;
                    }
                }
                self.place(start);
                if let Some(c) = cond {
                    self.cond(c, false, end);
                }
                self.stmt(body);
                self.place(cont);
                if self.reachable {
                    for u in update {
                        self.discard(u);
                    }
                    self.jump(op::GOTO, start);
                }
                self.place(end);
            }
            TS::Labeled { body, id } => {
                let end = self.new_label();
                self.targets.insert(*id, (end, end));
                self.stmt(body);
                self.place(end);
            }
            TS::Return(e) => {
                match e {
                    Some(e) => {
                        self.expr(e);
                        let o = match kind(&e.ty) {
                            0 => 172,
                            1 => 173,
                            2 => 175,
                            _ => 176,
                        };
                        // The returned expression may have a narrower static type
                        // (e.g. null); the opcode depends only on the kind.
                        self.op(o, -e.ty.slots());
                    }
                    None => self.op(op::RETURN, 0),
                }
                self.reachable = false;
            }
            TS::Break(id) => {
                let l = self.targets[id].0;
                self.jump(op::GOTO, l);
            }
            TS::Continue(id) => {
                let l = self.targets[id].1;
                self.jump(op::GOTO, l);
            }
            TS::Throw(e) => {
                self.expr(e);
                self.op(op::ATHROW, -1);
                self.reachable = false;
            }
            TS::Switch { scrut, arms, id } => self.switch(scrut, arms, *id),
            TS::Try { body, catches } => {
                let start = self.new_label();
                let end = self.new_label();
                let after = self.new_label();
                self.mark(start);
                let pc0 = self.code.len();
                self.stmts(body);
                self.mark(end);
                let empty = self.code.len() == pc0;
                if self.reachable {
                    self.jump(op::GOTO, after);
                }
                if !empty {
                    for c in catches {
                        let h = self.new_label();
                        self.labels[h].pos = Some(self.code.len() as u32);
                        self.reachable = true;
                        self.stack = 1;
                        self.adjust(0);
                        for cls in &c.classes {
                            let ci = self.cp.class(cls);
                            self.exc.push((start, end, h, ci));
                        }
                        self.line(c.line);
                        self.store(&Type::object(), c.slot);
                        self.stmts(&c.body);
                        if self.reachable {
                            self.jump(op::GOTO, after);
                        }
                    }
                }
                self.place(after);
            }
        }
    }

    fn switch(&mut self, scrut: &TE, arms: &[SwitchArm], id: u32) {
        let end = self.new_label();
        self.targets.insert(id, (end, end));
        self.expr(scrut);
        let arm_labels: Vec<usize> = arms.iter().map(|_| self.new_label()).collect();
        let default = arms
            .iter()
            .position(|a| a.is_default)
            .map(|i| arm_labels[i])
            .unwrap_or(end);
        let mut pairs: Vec<(i32, usize)> = Vec::new();
        for (i, a) in arms.iter().enumerate() {
            for v in &a.values {
                pairs.push((*v, arm_labels[i]));
            }
        }
        pairs.sort_by_key(|p| p.0);

        let instr = self.code.len();
        self.stack -= 1;
        let use_table = if let (Some(lo), Some(hi)) = (pairs.first(), pairs.last()) {
            let range = hi.0 as i64 - lo.0 as i64 + 1;
            let n = pairs.len() as i64;
            4 + range + 3 * 3 <= 3 + 2 * n + 3 * n
        } else {
            false
        };
        self.code
            .push(if use_table { op::TABLESWITCH } else { op::LOOKUPSWITCH });
        while self.code.len() % 4 != 0 {
            self.code.push(0);
        }
        let wide_ref = |g: &mut Self, l: usize| {
            let at = g.code.len();
            g.code.extend_from_slice(&[0, 0, 0, 0]);
            g.fixups.push(Fixup {
                instr,
                at,
                label: l,
                wide: true,
            });
            let lab = &mut g.labels[l];
            lab.used = true;
            if lab.stack.is_none() {
                lab.stack = Some(g.stack);
            }
        };
        wide_ref(self, default);
        if use_table {
            let lo = pairs.first().unwrap().0;
            let hi = pairs.last().unwrap().0;
            self.code.extend_from_slice(&lo.to_be_bytes());
            self.code.extend_from_slice(&hi.to_be_bytes());
            let mut it = pairs.iter().peekable();
            let mut v = lo as i64;
            while v <= hi as i64 {
                let l = match it.peek() {
                    Some(p) if p.0 as i64 == v => {
                        let l = p.1;
                        it.next();
                        l
                    }
                    _ => default,
                };
                wide_ref(self, l);
                v += 1;
            }
        } else {
            self.code.extend_from_slice(&(pairs.len() as i32).to_be_bytes());
            for (v, l) in &pairs {
                self.code.extend_from_slice(&v.to_be_bytes());
                wide_ref(self, *l);
            }
        }
        self.reachable = false;
        for (i, a) in arms.iter().enumerate() {
            self.place(arm_labels[i]);
            self.stmts(&a.body);
        }
        self.place(end);
    }

    fn finish(&mut self) -> Result<(), String> {
        for f in &self.fixups {
            let target = self.labels[f.label].pos.ok_or("internal error: unplaced label")? as i64;
            let off = target - f.instr as i64;
            if f.wide {
                self.code[f.at..f.at + 4].copy_from_slice(&(off as i32).to_be_bytes());
            } else {
                if off < i16::MIN as i64 || off > i16::MAX as i64 {
                    return Err("code too large: branch offset exceeds 32KB".into());
                }
                self.code[f.at..f.at + 2].copy_from_slice(&(off as i16).to_be_bytes());
            }
        }
        if self.code.len() > 65535 {
            return Err("code too large: method exceeds 64KB of bytecode".into());
        }
        Ok(())
    }
}

fn code_attr(cp: &mut ConstPool, m: &TMethod, body: &[TS], ret: &Type) -> Result<Vec<u8>, String> {
    let code_name = cp.utf8("Code");
    let lnt_name = cp.utf8("LineNumberTable");
    let mut g = Gen::new(cp);
    g.line(m.start_line);
    g.stmts(body);
    if g.reachable {
        // Unreachable per the checker; emitted only to keep the verifier happy.
        match kind(ret) {
            _ if *ret == Type::Void => g.op(op::RETURN, 0),
            0 => {
                g.iconst(0);
                g.op(172, -1)
            }
            1 => {
                g.op(op::LCONST_0, 2);
                g.op(173, -2)
            }
            2 => {
                g.op(op::DCONST_0, 2);
                g.op(175, -2)
            }
            _ => {
                g.op(op::ACONST_NULL, 1);
                g.op(176, -1)
            }
        }
    }
    g.finish()?;

    let mut a = Vec::with_capacity(g.code.len() + 64);
    a.extend_from_slice(&code_name.to_be_bytes());
    a.extend_from_slice(&[0, 0, 0, 0]); // length placeholder
    a.extend_from_slice(&(g.max_stack.max(0) as u16).to_be_bytes());
    a.extend_from_slice(&m.max_locals.to_be_bytes());
    a.extend_from_slice(&(g.code.len() as u32).to_be_bytes());
    a.extend_from_slice(&g.code);
    a.extend_from_slice(&(g.exc.len() as u16).to_be_bytes());
    for (s, e, h, c) in &g.exc {
        for l in [s, e, h] {
            a.extend_from_slice(&(g.labels[*l].pos.unwrap() as u16).to_be_bytes());
        }
        a.extend_from_slice(&c.to_be_bytes());
    }
    // attributes: LineNumberTable
    a.extend_from_slice(&1u16.to_be_bytes());
    a.extend_from_slice(&lnt_name.to_be_bytes());
    a.extend_from_slice(&(2 + 4 * g.lines.len() as u32).to_be_bytes());
    a.extend_from_slice(&(g.lines.len() as u16).to_be_bytes());
    for (pc, line) in &g.lines {
        a.extend_from_slice(&pc.to_be_bytes());
        a.extend_from_slice(&line.to_be_bytes());
    }
    let len = (a.len() - 6) as u32;
    a[2..6].copy_from_slice(&len.to_be_bytes());
    Ok(a)
}

/// Generate a complete class file.
pub fn gen_class(c: &TClass) -> Result<Vec<u8>, String> {
    let mut cp = ConstPool::new();
    let this = cp.class(&c.name);
    let sup = cp.class(&c.super_name);
    let interfaces = c.interfaces.iter().map(|i| cp.class(i)).collect();
    let mut fields = Vec::new();
    for f in &c.fields {
        fields.push(MemberOut {
            flags: f.flags,
            name: cp.utf8(&f.name),
            desc: cp.utf8(&f.desc),
            attrs: vec![],
        });
    }
    let mut methods = Vec::new();
    for m in &c.methods {
        let name = cp.utf8(&m.name);
        let desc = cp.utf8(&m.desc);
        let mut attrs = vec![];
        if let Some(body) = &m.body {
            let (_, ret) = crate::types::parse_method_descriptor(&m.desc).unwrap();
            attrs.push(code_attr(&mut cp, m, body, &ret).map_err(|e| format!("{}.{}: {}", c.name, m.name, e))?);
        }
        methods.push(MemberOut {
            flags: m.flags,
            name,
            desc,
            attrs,
        });
    }
    let sf_name = cp.utf8("SourceFile");
    let sf = cp.utf8(&c.source_file);
    let mut sf_attr = Vec::new();
    sf_attr.extend_from_slice(&sf_name.to_be_bytes());
    sf_attr.extend_from_slice(&2u32.to_be_bytes());
    sf_attr.extend_from_slice(&sf.to_be_bytes());
    if cp.count() > 65535 {
        return Err(format!("{}: too many constants", c.name));
    }
    Ok(serialize(
        &cp,
        &ClassOut {
            flags: c.flags,
            this,
            sup,
            interfaces,
            fields,
            methods,
            attrs: vec![sf_attr],
        },
    ))
}
