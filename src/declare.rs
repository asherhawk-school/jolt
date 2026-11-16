//! Declaration pass: registers user classes, their supertypes and members in
//! the class table so that bodies can be checked in any order (or in parallel).

use crate::ast::modifier as m;
use crate::ast::*;
use crate::classes::*;
use crate::diag::{err, Diag, Result};
use crate::types::{method_descriptor, Type};

/// Every user class declaration paired with its internal name and file index.
pub fn walk(units: &[CompilationUnit]) -> Vec<(&ClassDecl, String, usize)> {
    fn rec<'u>(d: &'u ClassDecl, name: String, file: usize, out: &mut Vec<(&'u ClassDecl, String, usize)>) {
        for n in &d.nested {
            rec(n, format!("{}${}", name, n.name), file, out);
        }
        out.push((d, name, file));
    }
    let mut out = Vec::new();
    for u in units {
        for c in &u.classes {
            let name = match &u.package {
                Some(p) => format!("{}/{}", p, c.name),
                None => c.name.clone(),
            };
            rec(c, name, u.file, &mut out);
        }
    }
    // Outer classes before nested ones keeps output order stable and natural.
    out.sort_by_key(|(_, n, f)| (*f, n.matches('$').count()));
    out
}

fn package_of(name: &str) -> &str {
    name.rsplit_once('/').map(|x| x.0).unwrap_or("")
}

fn simple_of(name: &str) -> &str {
    let s = name.rsplit('/').next().unwrap_or(name);
    s.rsplit('$').next().unwrap_or(s)
}

pub fn top_level(t: &ClassTable, name: &str) -> String {
    let mut cur = name.to_string();
    while let Some(o) = t.get(&cur).and_then(|c| c.outer.clone()) {
        cur = o;
    }
    cur
}

/// Resolve a simple class name as seen from inside class `ctx`.
pub fn lookup_class(t: &ClassTable, name: &str, ctx: &str) -> Option<String> {
    let mut cur = Some(ctx.to_string());
    while let Some(c) = cur {
        let ci = t.get(&c)?;
        if ci.user && simple_of(&c) == name {
            return Some(c);
        }
        if let Some((_, n)) = ci.nested.iter().find(|(s, _)| s == name) {
            return Some(n.clone());
        }
        // nested classes inherited from user superclasses
        let mut sup = ci.super_name.clone();
        while let Some(s) = sup {
            match t.get(&s) {
                Some(si) if si.user => {
                    if let Some((_, n)) = si.nested.iter().find(|(sn, _)| sn == name) {
                        return Some(n.clone());
                    }
                    sup = si.super_name.clone();
                }
                _ => break,
            }
        }
        cur = ci.outer.clone();
    }
    let cands = t.by_simple.get(name)?;
    let pkg = package_of(ctx);
    cands
        .iter()
        .find(|c| t.get(c).map_or(false, |ci| ci.user) && package_of(c) == pkg)
        .or_else(|| cands.iter().find(|c| t.get(c).map_or(false, |ci| ci.user)))
        .or_else(|| cands.first())
        .cloned()
}

pub fn resolve_class(t: &ClassTable, parts: &[String], ctx: &str) -> Option<String> {
    if let Some(mut cur) = lookup_class(t, &parts[0], ctx) {
        for p in &parts[1..] {
            let ci = t.get(&cur)?;
            cur = ci.nested.iter().find(|(s, _)| s == p)?.1.clone();
        }
        return Some(cur);
    }
    // fully-qualified name, possibly followed by nested class names
    for split in (1..=parts.len()).rev() {
        let base = parts[..split].join("/");
        if t.get(&base).is_some() {
            let mut cur = base;
            for p in &parts[split..] {
                let ci = t.get(&cur)?;
                cur = ci.nested.iter().find(|(s, _)| s == p)?.1.clone();
            }
            return Some(cur);
        }
    }
    None
}

pub fn resolve_type(t: &ClassTable, ty: &AstType, ctx: &str, file: usize) -> Result<Type> {
    match ty {
        AstType::Prim(p) => Ok(p.clone()),
        AstType::Array(e) => {
            let et = resolve_type(t, e, ctx, file)?;
            if et == Type::Void {
                return err(file, ty.pos().unwrap_or_default(), "'void' type not allowed here");
            }
            Ok(Type::Array(Box::new(et)))
        }
        AstType::Var => err(file, Default::default(), "'var' is not allowed here"),
        AstType::Named(parts, pos) => match resolve_class(t, parts, ctx) {
            Some(c) => Ok(Type::class(&c)),
            None => {
                let shown = parts.join(".");
                let hint = match shown.as_str() {
                    "ArrayList" | "List" | "HashMap" | "Map" | "HashSet" | "Set" | "LinkedList" | "Deque"
                    | "ArrayDeque" | "Queue" | "PriorityQueue" | "TreeMap" | "Iterator" | "Optional" | "Stream" => {
                        " (collections need generics, which jolt does not support yet)"
                    }
                    "float" | "Float" | "byte" | "Byte" | "short" | "Short" => " (not supported)",
                    _ => "",
                };
                err(file, *pos, format!("cannot find symbol: class {}{}", shown, hint))
            }
        },
    }
}

/// Register all user classes and their members. Returns diagnostics.
pub fn declare(units: &[CompilationUnit], t: &mut ClassTable) -> Vec<Diag> {
    let mut diags = Vec::new();
    let decls = walk(units);

    // Pass A: class skeletons.
    for u in units {
        for c in &u.classes {
            add_skeleton(c, None, u.package.as_deref().unwrap_or(""), u.file, t, &mut diags);
        }
    }
    if !diags.is_empty() {
        return diags;
    }

    // Pass B: supertypes and members (computed against an immutable table).
    let mut updates = Vec::new();
    for (d, name, file) in &decls {
        match members(d, name, *file, t) {
            Ok(u) => updates.push((name.clone(), u)),
            Err(e) => diags.push(e),
        }
    }
    for (name, (sup, ifaces, fields, methods)) in updates {
        let ci = t.classes.get_mut(&name).unwrap();
        ci.super_name = sup;
        ci.interfaces = ifaces;
        ci.fields = fields;
        ci.methods = methods;
    }

    // Pass C: inheritance sanity checks.
    for (d, name, file) in &decls {
        let mut cur = t.get(name).and_then(|c| c.super_name.clone());
        let mut steps = 0;
        while let Some(c) = cur {
            steps += 1;
            if c == *name || steps > 1000 {
                diags.push(Diag {
                    file: *file,
                    pos: d.pos,
                    msg: format!("cyclic inheritance involving {}", d.name),
                });
                break;
            }
            cur = t.get(&c).and_then(|c| c.super_name.clone());
        }
    }
    diags
}

fn add_skeleton(d: &ClassDecl, outer: Option<&str>, pkg: &str, file: usize, t: &mut ClassTable, diags: &mut Vec<Diag>) {
    let name = match outer {
        Some(o) => format!("{}${}", o, d.name),
        None if pkg.is_empty() => d.name.clone(),
        None => format!("{}/{}", pkg, d.name),
    };
    if t.get(&name).map_or(false, |c| c.user) {
        diags.push(Diag {
            file,
            pos: d.pos,
            msg: format!("duplicate class: {}", d.name),
        });
        return;
    }
    if outer.is_some() && !d.is_interface && d.mods & m::STATIC == 0 {
        // Inner (non-static) classes are treated as static nested classes.
        // Accessing the enclosing instance is reported later with a clear error.
    }
    let ci = ClassInfo {
        name: name.clone(),
        is_interface: d.is_interface,
        is_abstract: d.is_interface || d.mods & m::ABSTRACT != 0,
        user: true,
        outer: outer.map(|s| s.to_string()),
        nested: d
            .nested
            .iter()
            .map(|n| (n.name.clone(), format!("{}${}", name, n.name)))
            .collect(),
        in_nest: outer.is_some() || !d.nested.is_empty(),
        ..Default::default()
    };
    t.classes.insert(name.clone(), ci);
    if outer.is_none() {
        t.by_simple.entry(d.name.clone()).or_default().insert(0, name.clone());
    }
    for n in &d.nested {
        add_skeleton(n, Some(&name), pkg, file, t, diags);
    }
}

type Members = (Option<String>, Vec<String>, Vec<FieldInfo>, Vec<MethodInfo>);

fn members(d: &ClassDecl, name: &str, file: usize, t: &ClassTable) -> Result<Members> {
    let mut sup = Some("java/lang/Object".to_string());
    if let Some(e) = &d.extends {
        let ty = resolve_type(t, e, name, file)?;
        let Type::Class(c) = ty else {
            return err(file, d.pos, "cannot inherit from a non-class type");
        };
        let ci = t.get(&c).unwrap();
        if ci.is_interface {
            return err(
                file,
                e.pos().unwrap_or(d.pos),
                format!("no interface expected here: {}", simple_of(&c)),
            );
        }
        sup = Some(c.to_string());
    }
    let mut ifaces = Vec::new();
    for i in &d.implements {
        let ty = resolve_type(t, i, name, file)?;
        let Type::Class(c) = ty else {
            return err(file, d.pos, "interface expected here");
        };
        if !t.get(&c).unwrap().is_interface {
            return err(
                file,
                i.pos().unwrap_or(d.pos),
                format!("interface expected here: {}", simple_of(&c)),
            );
        }
        ifaces.push(c.to_string());
    }

    let mut fields: Vec<FieldInfo> = Vec::new();
    let mut methods: Vec<MethodInfo> = Vec::new();
    let mut has_ctor = false;
    for mem in &d.members {
        match mem {
            Member::Field(f) => {
                let ty = resolve_type(t, &f.ty, name, file)?;
                if ty == Type::Void {
                    return err(file, f.pos, "'void' type not allowed here");
                }
                if fields.iter().any(|x| x.name == f.name) {
                    return err(
                        file,
                        f.pos,
                        format!("variable {} is already defined in class {}", f.name, d.name),
                    );
                }
                fields.push(FieldInfo {
                    name: f.name.clone(),
                    ty,
                    is_static: f.mods & m::STATIC != 0,
                    flags: f.mods & 0x00df,
                    constant: None,
                });
            }
            Member::Method(md) => {
                if md.mods & m::DEFAULT != 0 {
                    return err(file, md.pos, "default interface methods are not supported");
                }
                if d.is_interface && md.mods & m::STATIC != 0 {
                    return err(file, md.pos, "static interface methods are not supported");
                }
                if d.is_interface && md.mods & m::PRIVATE != 0 {
                    return err(file, md.pos, "private interface methods are not supported");
                }
                if md.is_ctor {
                    if d.is_interface {
                        return err(file, md.pos, "interfaces cannot have constructors");
                    }
                    has_ctor = true;
                }
                let mut params = Vec::new();
                for p in &md.params {
                    params.push(resolve_type(t, &p.ty, name, file)?);
                }
                let ret = resolve_type(t, &md.ret, name, file)?;
                let desc = method_descriptor(&params, &ret);
                if methods.iter().any(|x| x.name == md.name && x.params == params) {
                    let what = if md.is_ctor { d.name.clone() } else { md.name.clone() };
                    return err(
                        file,
                        md.pos,
                        format!("method {} is already defined in class {}", what, d.name),
                    );
                }
                let is_static = md.mods & m::STATIC != 0;
                if md.body.is_none() && md.mods & (m::ABSTRACT | m::NATIVE) == 0 {
                    return err(file, md.pos, "missing method body, or declare abstract");
                }
                if md.body.is_some() && md.mods & m::ABSTRACT != 0 {
                    return err(
                        file,
                        md.pos,
                        if d.is_interface {
                            "interface abstract methods cannot have body"
                        } else {
                            "abstract methods cannot have a body"
                        },
                    );
                }
                if md.mods & m::ABSTRACT != 0 && !d.is_interface && d.mods & m::ABSTRACT == 0 {
                    return err(
                        file,
                        md.pos,
                        format!("{} is not abstract and declares abstract method {}", d.name, md.name),
                    );
                }
                let mut flags = md.mods & 0x0d3f;
                if md.varargs {
                    flags |= ACC_VARARGS;
                }
                methods.push(MethodInfo {
                    name: md.name.clone(),
                    params,
                    ret,
                    is_static,
                    flags,
                    varargs: md.varargs,
                    desc,
                });
            }
            _ => {}
        }
    }
    if !has_ctor && !d.is_interface {
        methods.push(MethodInfo {
            name: "<init>".into(),
            params: vec![],
            ret: Type::Void,
            is_static: false,
            flags: if d.mods & m::PUBLIC != 0 { ACC_PUBLIC } else { 0 },
            varargs: false,
            desc: "()V".into(),
        });
    }
    if d.is_interface {
        sup = Some("java/lang/Object".into());
    }
    Ok((sup, ifaces, fields, methods))
}
