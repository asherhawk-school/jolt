//! Recursive-descent parser with precedence climbing for expressions.

use crate::ast::modifier as m;
use crate::ast::*;
use crate::diag::{err, Pos, Result};
use crate::lexer::{Tok, Token};
use crate::types::Type;

pub struct Parser<'a> {
    toks: &'a [Token],
    i: usize,
    file: usize,
}

pub fn parse(toks: &[Token], file: usize) -> Result<CompilationUnit> {
    let mut p = Parser { toks, i: 0, file };
    p.compilation_unit()
}

fn bin_prec(t: &Tok) -> Option<(BinOp, u8)> {
    let Tok::Punct(p) = t else { return None };
    Some(match *p {
        "||" => (BinOp::Or, 1),
        "&&" => (BinOp::And, 2),
        "|" => (BinOp::BitOr, 3),
        "^" => (BinOp::BitXor, 4),
        "&" => (BinOp::BitAnd, 5),
        "==" => (BinOp::Eq, 6),
        "!=" => (BinOp::Ne, 6),
        "<" => (BinOp::Lt, 7),
        ">" => (BinOp::Gt, 7),
        "<=" => (BinOp::Le, 7),
        ">=" => (BinOp::Ge, 7),
        "<<" => (BinOp::Shl, 8),
        ">>" => (BinOp::Shr, 8),
        ">>>" => (BinOp::UShr, 8),
        "+" => (BinOp::Add, 9),
        "-" => (BinOp::Sub, 9),
        "*" => (BinOp::Mul, 10),
        "/" => (BinOp::Div, 10),
        "%" => (BinOp::Rem, 10),
        _ => return None,
    })
}

fn assign_op(t: &Tok) -> Option<Option<BinOp>> {
    let Tok::Punct(p) = t else { return None };
    Some(match *p {
        "=" => None,
        "+=" => Some(BinOp::Add),
        "-=" => Some(BinOp::Sub),
        "*=" => Some(BinOp::Mul),
        "/=" => Some(BinOp::Div),
        "%=" => Some(BinOp::Rem),
        "&=" => Some(BinOp::BitAnd),
        "|=" => Some(BinOp::BitOr),
        "^=" => Some(BinOp::BitXor),
        "<<=" => Some(BinOp::Shl),
        ">>=" => Some(BinOp::Shr),
        ">>>=" => Some(BinOp::UShr),
        _ => return None,
    })
}

impl<'a> Parser<'a> {
    // ---------- token helpers ----------
    fn peek(&self) -> &Tok {
        &self.toks[self.i].tok
    }
    fn peek_at(&self, n: usize) -> &Tok {
        let j = (self.i + n).min(self.toks.len() - 1);
        &self.toks[j].tok
    }
    fn pos(&self) -> Pos {
        self.toks[self.i].pos
    }
    fn bump(&mut self) -> Tok {
        let t = self.toks[self.i].tok.clone();
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        t
    }
    fn is_p(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::Punct(q) if *q == p)
    }
    fn is_p_at(&self, n: usize, p: &str) -> bool {
        matches!(self.peek_at(n), Tok::Punct(q) if *q == p)
    }
    fn is_kw(&self, k: &str) -> bool {
        matches!(self.peek(), Tok::Kw(q) if *q == k)
    }
    fn eat_p(&mut self, p: &str) -> bool {
        if self.is_p(p) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn eat_kw(&mut self, k: &str) -> bool {
        if self.is_kw(k) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn fail<T>(&self, msg: impl Into<String>) -> Result<T> {
        err(self.file, self.pos(), msg)
    }
    fn describe(&self) -> String {
        match self.peek() {
            Tok::Ident(s) => format!("'{}'", s),
            Tok::Kw(k) => format!("'{}'", k),
            Tok::Punct(p) => format!("'{}'", p),
            Tok::Eof => "end of file".to_string(),
            _ => "literal".to_string(),
        }
    }
    fn expect_p(&mut self, p: &str) -> Result<()> {
        if self.eat_p(p) {
            Ok(())
        } else if matches!(p, ";" | ")" | "]") && self.i > 0 {
            // Like javac: point just past the previous token.
            err(self.file, self.toks[self.i - 1].end, format!("'{}' expected", p))
        } else {
            self.fail(format!("'{}' expected but found {}", p, self.describe()))
        }
    }
    fn ident(&mut self) -> Result<String> {
        match self.peek() {
            Tok::Ident(s) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            _ => self.fail(format!("<identifier> expected but found {}", self.describe())),
        }
    }

    // ---------- declarations ----------
    fn compilation_unit(&mut self) -> Result<CompilationUnit> {
        let mut package = None;
        self.skip_annotations()?;
        if self.eat_kw("package") {
            package = Some(self.qualified_name()?.join("/"));
            self.expect_p(";")?;
        }
        while self.eat_kw("import") {
            self.eat_kw("static");
            self.ident()?;
            while self.eat_p(".") {
                if !self.eat_p("*") {
                    self.ident()?;
                }
            }
            self.expect_p(";")?;
        }
        let mut classes = Vec::new();
        loop {
            if self.eat_p(";") {
                continue;
            }
            if matches!(self.peek(), Tok::Eof) {
                break;
            }
            let mods = self.modifiers()?;
            classes.push(self.type_decl(mods)?);
        }
        Ok(CompilationUnit {
            file: self.file,
            package,
            classes,
        })
    }

    fn qualified_name(&mut self) -> Result<Vec<String>> {
        let mut v = vec![self.ident()?];
        while self.is_p(".") && matches!(self.peek_at(1), Tok::Ident(_)) {
            self.i += 1;
            v.push(self.ident()?);
        }
        Ok(v)
    }

    fn skip_annotations(&mut self) -> Result<()> {
        while self.is_p("@") && !matches!(self.peek_at(1), Tok::Kw("interface")) {
            self.i += 1;
            self.qualified_name()?;
            if self.is_p("(") {
                let mut depth = 0;
                loop {
                    if self.is_p("(") {
                        depth += 1;
                    } else if self.is_p(")") {
                        depth -= 1;
                        if depth == 0 {
                            self.i += 1;
                            break;
                        }
                    } else if matches!(self.peek(), Tok::Eof) {
                        return self.fail("unclosed annotation");
                    }
                    self.i += 1;
                }
            }
        }
        Ok(())
    }

    fn modifiers(&mut self) -> Result<u16> {
        let mut mods = 0u16;
        loop {
            self.skip_annotations()?;
            let bit = match self.peek() {
                Tok::Kw("public") => m::PUBLIC,
                Tok::Kw("private") => m::PRIVATE,
                Tok::Kw("protected") => m::PROTECTED,
                Tok::Kw("static") => m::STATIC,
                Tok::Kw("final") => m::FINAL,
                Tok::Kw("abstract") => m::ABSTRACT,
                Tok::Kw("native") => m::NATIVE,
                Tok::Kw("synchronized") if !self.is_p_at(1, "(") => m::SYNCHRONIZED,
                Tok::Kw("transient") => m::TRANSIENT,
                Tok::Kw("volatile") => m::VOLATILE,
                Tok::Kw("strictfp") => m::STRICT,
                Tok::Kw("default") if !self.is_p_at(1, ":") && !self.is_p_at(1, "->") => m::DEFAULT,
                Tok::Ident(s)
                    if (s == "sealed" || s == "non")
                        && matches!(
                            self.peek_at(1),
                            Tok::Kw("class") | Tok::Kw("interface") | Tok::Kw("abstract")
                        ) =>
                {
                    return self.fail("sealed classes are not supported");
                }
                _ => return Ok(mods),
            };
            mods |= bit;
            self.i += 1;
        }
    }

    fn type_decl(&mut self, mods: u16) -> Result<ClassDecl> {
        let pos = self.pos();
        let is_interface = if self.eat_kw("class") {
            false
        } else if self.eat_kw("interface") {
            true
        } else if self.is_kw("enum") {
            return self.fail("enums are not supported");
        } else if matches!(self.peek(), Tok::Ident(s) if s == "record") {
            return self.fail("records are not supported");
        } else if self.is_p("@") {
            return self.fail("annotation type declarations are not supported");
        } else {
            return self.fail(format!("class or interface expected but found {}", self.describe()));
        };
        let name = self.ident()?;
        if self.is_p("<") {
            return self.fail("generic classes are not supported");
        }
        let mut extends = None;
        let mut implements = Vec::new();
        if self.eat_kw("extends") {
            if is_interface {
                implements.push(self.parse_type()?);
                while self.eat_p(",") {
                    implements.push(self.parse_type()?);
                }
            } else {
                extends = Some(self.parse_type()?);
            }
        }
        if self.eat_kw("implements") {
            if is_interface {
                return self.fail("interfaces use 'extends', not 'implements'");
            }
            implements.push(self.parse_type()?);
            while self.eat_p(",") {
                implements.push(self.parse_type()?);
            }
        }
        if matches!(self.peek(), Tok::Ident(s) if s == "permits") {
            return self.fail("sealed classes are not supported");
        }
        self.expect_p("{")?;
        let mut members = Vec::new();
        let mut nested = Vec::new();
        while !self.eat_p("}") {
            if matches!(self.peek(), Tok::Eof) {
                return self.fail("reached end of file while parsing class body");
            }
            if self.eat_p(";") {
                continue;
            }
            let mpos = self.pos();
            let mut mods = self.modifiers()?;
            if is_interface {
                // interface members are implicitly public; fields static final; methods abstract
                mods |= m::PUBLIC;
            }
            if self.is_kw("class")
                || self.is_kw("interface")
                || self.is_kw("enum")
                || matches!(self.peek(), Tok::Ident(s) if s == "record" && matches!(self.peek_at(1), Tok::Ident(_)))
            {
                let mut nm = mods;
                if is_interface {
                    nm |= m::STATIC;
                }
                nested.push(self.type_decl(nm)?);
                continue;
            }
            if self.is_p("{") {
                self.i += 1;
                let body = self.block_rest()?;
                if mods & m::STATIC != 0 {
                    members.push(Member::StaticInit(body, mpos));
                } else {
                    members.push(Member::InstanceInit(body, mpos));
                }
                continue;
            }
            if self.is_p("<") {
                return self.fail("generic methods are not supported");
            }
            // constructor?
            if matches!(self.peek(), Tok::Ident(s) if *s == name) && self.is_p_at(1, "(") {
                let pos = self.pos();
                self.i += 1;
                let mut md = self.method_rest(mods, AstType::Prim(Type::Void), "<init>".into(), pos)?;
                md.is_ctor = true;
                members.push(Member::Method(md));
                continue;
            }
            let ty = self.parse_type_or_void()?;
            let pos = self.pos();
            let mname = self.ident()?;
            if self.is_p("(") {
                if is_interface && mods & (m::STATIC | m::DEFAULT | m::PRIVATE) == 0 {
                    mods |= m::ABSTRACT;
                }
                members.push(Member::Method(self.method_rest(mods, ty, mname, pos)?));
            } else {
                if is_interface {
                    mods |= m::STATIC | m::FINAL;
                }
                let mut first = Some((mname, pos));
                loop {
                    let (fname, fpos) = match first.take() {
                        Some(x) => x,
                        None => {
                            let p = self.pos();
                            (self.ident()?, p)
                        }
                    };
                    let mut fty = ty.clone();
                    while self.eat_p("[") {
                        self.expect_p("]")?;
                        fty = AstType::Array(Box::new(fty));
                    }
                    let init = if self.eat_p("=") { Some(self.var_init()?) } else { None };
                    members.push(Member::Field(FieldDecl {
                        mods,
                        ty: fty,
                        name: fname,
                        init,
                        pos: fpos,
                    }));
                    if !self.eat_p(",") {
                        break;
                    }
                }
                self.expect_p(";")?;
            }
        }
        Ok(ClassDecl {
            name,
            pos,
            mods,
            is_interface,
            extends,
            implements,
            members,
            nested,
        })
    }

    fn method_rest(&mut self, mods: u16, ret: AstType, name: String, pos: Pos) -> Result<MethodDecl> {
        self.expect_p("(")?;
        let mut params = Vec::new();
        let mut varargs = false;
        if !self.eat_p(")") {
            loop {
                self.modifiers()?;
                let mut ty = self.parse_type()?;
                if self.eat_p("...") {
                    ty = AstType::Array(Box::new(ty));
                    varargs = true;
                }
                let ppos = self.pos();
                let pname = self.ident()?;
                while self.eat_p("[") {
                    self.expect_p("]")?;
                    ty = AstType::Array(Box::new(ty));
                }
                params.push(Param {
                    ty,
                    name: pname,
                    pos: ppos,
                });
                if self.eat_p(")") {
                    break;
                }
                if varargs {
                    return self.fail("varargs parameter must be the last parameter");
                }
                self.expect_p(",")?;
            }
        }
        let mut ret = ret;
        while self.eat_p("[") {
            self.expect_p("]")?;
            ret = AstType::Array(Box::new(ret));
        }
        if self.eat_kw("throws") {
            self.parse_type()?;
            while self.eat_p(",") {
                self.parse_type()?;
            }
        }
        let mut end_pos = self.pos();
        let body = if self.eat_p(";") {
            None
        } else {
            self.expect_p("{")?;
            let b = self.block_rest()?;
            end_pos = self.toks[self.i - 1].pos;
            Some(b)
        };
        Ok(MethodDecl {
            mods,
            ret,
            name,
            params,
            varargs,
            body,
            pos,
            is_ctor: false,
            end_pos,
        })
    }

    // ---------- types ----------
    fn prim_type(&mut self) -> Result<Option<Type>> {
        let t = match self.peek() {
            Tok::Kw("boolean") => Type::Boolean,
            Tok::Kw("char") => Type::Char,
            Tok::Kw("int") => Type::Int,
            Tok::Kw("long") => Type::Long,
            Tok::Kw("double") => Type::Double,
            Tok::Kw(k @ ("byte" | "short" | "float")) => {
                return self.fail(format!("type '{}' is not supported (use int, long or double)", k))
            }
            _ => return Ok(None),
        };
        self.i += 1;
        Ok(Some(t))
    }

    fn parse_type_or_void(&mut self) -> Result<AstType> {
        if self.eat_kw("void") {
            return Ok(AstType::Prim(Type::Void));
        }
        self.parse_type()
    }

    fn parse_type(&mut self) -> Result<AstType> {
        self.skip_annotations()?;
        let mut t = if let Some(p) = self.prim_type()? {
            AstType::Prim(p)
        } else if matches!(self.peek(), Tok::Ident(_)) {
            let pos = self.pos();
            let name = self.qualified_name()?;
            if self.is_p("<") {
                return self.fail("generics are not supported");
            }
            AstType::Named(name, pos)
        } else {
            return self.fail(format!("type expected but found {}", self.describe()));
        };
        while self.is_p("[") && self.is_p_at(1, "]") {
            self.i += 2;
            t = AstType::Array(Box::new(t));
        }
        Ok(t)
    }

    /// Does a local variable declaration start here? (Type followed by identifier.)
    fn looks_like_decl(&self) -> bool {
        let mut j = self.i;
        let tok = |j: usize| &self.toks[j.min(self.toks.len() - 1)].tok;
        match tok(j) {
            Tok::Kw("final") | Tok::Punct("@") => return true,
            Tok::Kw("boolean" | "char" | "int" | "long" | "double" | "byte" | "short" | "float") => {
                return !matches!(tok(j + 1), Tok::Punct("."))
            }
            Tok::Ident(s) if s == "var" && matches!(tok(j + 1), Tok::Ident(_)) => return true,
            Tok::Ident(_) => {}
            _ => return false,
        }
        j += 1;
        while matches!(tok(j), Tok::Punct(".")) && matches!(tok(j + 1), Tok::Ident(_)) {
            j += 2;
        }
        if matches!(tok(j), Tok::Punct("<")) {
            // Could be `a < b` or a generic type; generic declarations look like
            // `Name<...> ident`. We only flag it when it's clearly a type.
            let mut depth = 0;
            let mut k = j;
            loop {
                match tok(k) {
                    Tok::Punct("<") => depth += 1,
                    Tok::Punct(">") => depth -= 1,
                    Tok::Punct(">>") => depth -= 2,
                    Tok::Punct(">>>") => depth -= 3,
                    Tok::Ident(_)
                    | Tok::Punct(",")
                    | Tok::Punct(".")
                    | Tok::Punct("?")
                    | Tok::Kw("extends")
                    | Tok::Kw("super")
                    | Tok::Punct("[")
                    | Tok::Punct("]") => {}
                    _ => return false,
                }
                k += 1;
                if depth <= 0 {
                    break;
                }
            }
            return matches!(tok(k), Tok::Ident(_));
        }
        while matches!(tok(j), Tok::Punct("[")) && matches!(tok(j + 1), Tok::Punct("]")) {
            j += 2;
        }
        matches!(tok(j), Tok::Ident(_))
    }

    // ---------- statements ----------
    fn block_rest(&mut self) -> Result<Vec<Stmt>> {
        let mut v = Vec::new();
        while !self.eat_p("}") {
            if matches!(self.peek(), Tok::Eof) {
                return self.fail("reached end of file while parsing");
            }
            v.push(self.statement()?);
        }
        Ok(v)
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect_p("{")?;
        self.block_rest()
    }

    fn statement(&mut self) -> Result<Stmt> {
        let pos = self.pos();
        let kind = match self.peek().clone() {
            Tok::Punct("{") => {
                self.i += 1;
                StmtKind::Block(self.block_rest()?)
            }
            Tok::Punct(";") => {
                self.i += 1;
                StmtKind::Empty
            }
            Tok::Kw("if") => {
                self.i += 1;
                self.expect_p("(")?;
                let c = self.expr()?;
                self.expect_p(")")?;
                let t = self.statement()?;
                let e = if self.eat_kw("else") {
                    Some(Box::new(self.statement()?))
                } else {
                    None
                };
                StmtKind::If(c, Box::new(t), e)
            }
            Tok::Kw("while") => {
                self.i += 1;
                self.expect_p("(")?;
                let c = self.expr()?;
                self.expect_p(")")?;
                StmtKind::While(c, Box::new(self.statement()?))
            }
            Tok::Kw("do") => {
                self.i += 1;
                let body = self.statement()?;
                if !self.eat_kw("while") {
                    return self.fail("'while' expected");
                }
                self.expect_p("(")?;
                let c = self.expr()?;
                self.expect_p(")")?;
                self.expect_p(";")?;
                StmtKind::DoWhile(Box::new(body), c)
            }
            Tok::Kw("for") => {
                self.i += 1;
                self.for_stmt()?
            }
            Tok::Kw("return") => {
                self.i += 1;
                let e = if self.is_p(";") { None } else { Some(self.expr()?) };
                self.expect_p(";")?;
                StmtKind::Return(e)
            }
            Tok::Kw("break") => {
                self.i += 1;
                let l = if let Tok::Ident(s) = self.peek() {
                    let s = s.clone();
                    self.i += 1;
                    Some(s)
                } else {
                    None
                };
                self.expect_p(";")?;
                StmtKind::Break(l)
            }
            Tok::Kw("continue") => {
                self.i += 1;
                let l = if let Tok::Ident(s) = self.peek() {
                    let s = s.clone();
                    self.i += 1;
                    Some(s)
                } else {
                    None
                };
                self.expect_p(";")?;
                StmtKind::Continue(l)
            }
            Tok::Kw("throw") => {
                self.i += 1;
                let e = self.expr()?;
                self.expect_p(";")?;
                StmtKind::Throw(e)
            }
            Tok::Kw("switch") => {
                self.i += 1;
                self.switch_stmt()?
            }
            Tok::Kw("try") => {
                self.i += 1;
                if self.is_p("(") {
                    return self.fail("try-with-resources is not supported");
                }
                let body = self.block()?;
                let mut catches = Vec::new();
                while self.is_kw("catch") {
                    let cpos = self.pos();
                    self.i += 1;
                    self.expect_p("(")?;
                    self.modifiers()?;
                    let mut types = vec![self.parse_type()?];
                    while self.eat_p("|") {
                        types.push(self.parse_type()?);
                    }
                    let name = self.ident()?;
                    self.expect_p(")")?;
                    let cbody = self.block()?;
                    catches.push(Catch {
                        types,
                        name,
                        body: cbody,
                        pos: cpos,
                    });
                }
                let finally = if self.eat_kw("finally") {
                    Some(self.block()?)
                } else {
                    None
                };
                if catches.is_empty() && finally.is_none() {
                    return err(self.file, pos, "'try' without 'catch' or 'finally'");
                }
                StmtKind::Try { body, catches, finally }
            }
            Tok::Kw("synchronized") => return self.fail("synchronized blocks are not supported"),
            Tok::Kw("assert") => return self.fail("assert is not supported"),
            Tok::Kw("class") | Tok::Kw("interface") | Tok::Kw("enum") => {
                return self.fail("local classes are not supported")
            }
            Tok::Kw(k @ ("this" | "super")) if self.is_p_at(1, "(") => {
                self.i += 2;
                let args = self.args_rest()?;
                self.expect_p(";")?;
                StmtKind::CtorCall {
                    is_super: k == "super",
                    args,
                }
            }
            Tok::Ident(label) if self.is_p_at(1, ":") => {
                self.i += 2;
                StmtKind::Labeled(label, Box::new(self.statement()?))
            }
            Tok::Ident(s) if s == "yield" && !self.is_p_at(1, "=") && !self.is_p_at(1, "(") => {
                return self.fail("switch expressions (yield) are not supported")
            }
            _ => {
                if self.looks_like_decl() {
                    let s = self.local_var_decl()?;
                    self.expect_p(";")?;
                    s
                } else {
                    let e = self.expr()?;
                    self.check_stmt_expr(&e)?;
                    self.expect_p(";")?;
                    StmtKind::Expr(e)
                }
            }
        };
        Ok(Stmt { kind, pos })
    }

    fn check_stmt_expr(&self, e: &Expr) -> Result<()> {
        match e.kind {
            ExprKind::Assign { .. } | ExprKind::IncDec { .. } | ExprKind::Call { .. } | ExprKind::New(..) => Ok(()),
            _ => err(self.file, e.pos, "not a statement"),
        }
    }

    fn local_var_decl(&mut self) -> Result<StmtKind> {
        self.modifiers()?;
        let ty = if matches!(self.peek(), Tok::Ident(s) if s == "var") && matches!(self.peek_at(1), Tok::Ident(_)) {
            self.i += 1;
            AstType::Var
        } else {
            self.parse_type()?
        };
        self.local_var_rest(ty)
    }

    fn local_var_rest(&mut self, ty: AstType) -> Result<StmtKind> {
        let mut decls = Vec::new();
        loop {
            let pos = self.pos();
            let name = self.ident()?;
            let mut extra_dims = 0;
            while self.eat_p("[") {
                self.expect_p("]")?;
                extra_dims += 1;
            }
            let init = if self.eat_p("=") { Some(self.var_init()?) } else { None };
            decls.push(VarDeclarator {
                name,
                extra_dims,
                init,
                pos,
            });
            if !self.eat_p(",") {
                break;
            }
        }
        Ok(StmtKind::LocalVar(ty, decls))
    }

    fn var_init(&mut self) -> Result<Expr> {
        if self.is_p("{") {
            let pos = self.pos();
            self.i += 1;
            Ok(Expr {
                kind: ExprKind::ArrayInit(self.array_init_rest()?),
                pos,
            })
        } else {
            self.expr()
        }
    }

    fn array_init_rest(&mut self) -> Result<Vec<Expr>> {
        let mut v = Vec::new();
        loop {
            if self.eat_p("}") {
                break;
            }
            v.push(self.var_init()?);
            if !self.eat_p(",") {
                self.expect_p("}")?;
                break;
            }
        }
        Ok(v)
    }

    fn for_stmt(&mut self) -> Result<StmtKind> {
        self.expect_p("(")?;
        let mut init = Vec::new();
        if !self.is_p(";") {
            if self.looks_like_decl() {
                let save = self.i;
                self.modifiers()?;
                let ty =
                    if matches!(self.peek(), Tok::Ident(s) if s == "var") && matches!(self.peek_at(1), Tok::Ident(_)) {
                        self.i += 1;
                        AstType::Var
                    } else {
                        self.parse_type()?
                    };
                if matches!(self.peek(), Tok::Ident(_)) && self.is_p_at(1, ":") {
                    let name = self.ident()?;
                    self.i += 1;
                    let iter = self.expr()?;
                    self.expect_p(")")?;
                    let body = self.statement()?;
                    return Ok(StmtKind::ForEach {
                        ty,
                        name,
                        iter,
                        body: Box::new(body),
                    });
                }
                self.i = save;
                let pos = self.pos();
                let kind = self.local_var_decl()?;
                init.push(Stmt { kind, pos });
            } else {
                loop {
                    let e = self.expr()?;
                    self.check_stmt_expr(&e)?;
                    let pos = e.pos;
                    init.push(Stmt {
                        kind: StmtKind::Expr(e),
                        pos,
                    });
                    if !self.eat_p(",") {
                        break;
                    }
                }
            }
        }
        self.expect_p(";")?;
        let cond = if self.is_p(";") { None } else { Some(self.expr()?) };
        self.expect_p(";")?;
        let mut update = Vec::new();
        if !self.is_p(")") {
            loop {
                let e = self.expr()?;
                self.check_stmt_expr(&e)?;
                update.push(e);
                if !self.eat_p(",") {
                    break;
                }
            }
        }
        self.expect_p(")")?;
        let body = self.statement()?;
        Ok(StmtKind::For {
            init,
            cond,
            update,
            body: Box::new(body),
        })
    }

    fn switch_stmt(&mut self) -> Result<StmtKind> {
        self.expect_p("(")?;
        let scrut = self.expr()?;
        self.expect_p(")")?;
        self.expect_p("{")?;
        let mut cases: Vec<SwitchCase> = Vec::new();
        let mut arrow: Option<bool> = None;
        while !self.eat_p("}") {
            let pos = self.pos();
            let mut labels = Vec::new();
            let mut is_default = false;
            if self.eat_kw("default") {
                is_default = true;
            } else if self.eat_kw("case") {
                loop {
                    labels.push(self.ternary()?);
                    if !self.eat_p(",") {
                        break;
                    }
                }
            } else {
                return self.fail(format!(
                    "'case', 'default', or '}}' expected but found {}",
                    self.describe()
                ));
            }
            let is_arrow = if self.eat_p("->") {
                true
            } else {
                self.expect_p(":")?;
                false
            };
            match arrow {
                None => arrow = Some(is_arrow),
                Some(a) if a != is_arrow => return err(self.file, pos, "different case kinds used in the switch"),
                _ => {}
            }
            let mut body = Vec::new();
            if is_arrow {
                if self.is_p("{") {
                    body.push(self.statement()?);
                } else if self.is_kw("throw") {
                    body.push(self.statement()?);
                } else {
                    let spos = self.pos();
                    let e = self.expr()?;
                    self.check_stmt_expr(&e)?;
                    self.expect_p(";")?;
                    body.push(Stmt {
                        kind: StmtKind::Expr(e),
                        pos: spos,
                    });
                }
            } else {
                while !self.is_kw("case") && !self.is_kw("default") && !self.is_p("}") {
                    if matches!(self.peek(), Tok::Eof) {
                        return self.fail("reached end of file while parsing switch");
                    }
                    body.push(self.statement()?);
                }
            }
            cases.push(SwitchCase {
                labels,
                is_default,
                body,
                pos,
            });
        }
        Ok(StmtKind::Switch {
            scrut,
            cases,
            arrow: arrow.unwrap_or(false),
        })
    }

    // ---------- expressions ----------
    pub fn expr(&mut self) -> Result<Expr> {
        let lhs = self.ternary()?;
        if let Some(op) = assign_op(self.peek()) {
            let pos = self.pos();
            self.i += 1;
            match lhs.kind {
                ExprKind::Name(_) | ExprKind::Field(..) | ExprKind::Index(..) => {}
                _ => return err(self.file, lhs.pos, "unexpected type: required variable, found value"),
            }
            let rhs = self.expr()?;
            return Ok(Expr {
                pos,
                kind: ExprKind::Assign {
                    op,
                    target: Box::new(lhs),
                    value: Box::new(rhs),
                },
            });
        }
        if self.is_p("->") {
            return self.fail("lambda expressions are not supported");
        }
        Ok(lhs)
    }

    fn ternary(&mut self) -> Result<Expr> {
        let c = self.binary(1)?;
        if self.is_p("?") {
            let pos = self.pos();
            self.i += 1;
            let a = self.expr()?;
            self.expect_p(":")?;
            let b = self.ternary()?;
            return Ok(Expr {
                pos,
                kind: ExprKind::Cond(Box::new(c), Box::new(a), Box::new(b)),
            });
        }
        Ok(c)
    }

    fn binary(&mut self, min_prec: u8) -> Result<Expr> {
        let mut lhs = self.unary()?;
        loop {
            if self.is_kw("instanceof") && 7 >= min_prec {
                let pos = self.pos();
                self.i += 1;
                self.eat_kw("final");
                let ty = self.parse_type()?;
                if matches!(self.peek(), Tok::Ident(_)) {
                    return self.fail("pattern matching for instanceof is not supported");
                }
                lhs = Expr {
                    pos,
                    kind: ExprKind::InstanceOf(Box::new(lhs), ty),
                };
                continue;
            }
            let Some((op, prec)) = bin_prec(self.peek()) else { break };
            if prec < min_prec {
                break;
            }
            let pos = self.pos();
            self.i += 1;
            let rhs = self.binary(prec + 1)?;
            lhs = Expr {
                pos,
                kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr> {
        let pos = self.pos();
        let op = match self.peek() {
            Tok::Punct("-") => Some(UnOp::Neg),
            Tok::Punct("+") => Some(UnOp::Plus),
            Tok::Punct("!") => Some(UnOp::Not),
            Tok::Punct("~") => Some(UnOp::BitNot),
            _ => None,
        };
        if let Some(op) = op {
            self.i += 1;
            let e = self.unary()?;
            if op == UnOp::Neg {
                // Fold negative literals so that -2147483648 and -9223372036854775808L work.
                if let ExprKind::Lit(lit) = &e.kind {
                    let folded = match lit {
                        Lit::Int(v) => Some(Lit::Int(-v)),
                        Lit::Long(v) => Some(Lit::Long(v.wrapping_neg())),
                        Lit::Double(v) => Some(Lit::Double(-v)),
                        _ => None,
                    };
                    if let Some(l) = folded {
                        return Ok(Expr {
                            pos,
                            kind: ExprKind::Lit(l),
                        });
                    }
                }
            }
            return Ok(Expr {
                pos,
                kind: ExprKind::Unary(op, Box::new(e)),
            });
        }
        if self.is_p("++") || self.is_p("--") {
            let inc = self.is_p("++");
            self.i += 1;
            let e = self.unary()?;
            return Ok(Expr {
                pos,
                kind: ExprKind::IncDec {
                    pre: true,
                    inc,
                    target: Box::new(e),
                },
            });
        }
        if self.is_p("(") && self.is_cast() {
            self.i += 1;
            let ty = self.parse_type()?;
            self.expect_p(")")?;
            let e = self.unary()?;
            return Ok(Expr {
                pos,
                kind: ExprKind::Cast(ty, Box::new(e)),
            });
        }
        let prim = self.primary()?;
        self.postfix(prim)
    }

    fn is_cast(&self) -> bool {
        // self.peek() == "("
        let tok = |j: usize| &self.toks[j.min(self.toks.len() - 1)].tok;
        let mut j = self.i + 1;
        match tok(j) {
            Tok::Kw("boolean" | "char" | "int" | "long" | "double" | "byte" | "short" | "float") => return true,
            Tok::Ident(_) => {}
            _ => return false,
        }
        j += 1;
        while matches!(tok(j), Tok::Punct(".")) && matches!(tok(j + 1), Tok::Ident(_)) {
            j += 2;
        }
        while matches!(tok(j), Tok::Punct("[")) && matches!(tok(j + 1), Tok::Punct("]")) {
            j += 2;
        }
        if !matches!(tok(j), Tok::Punct(")")) {
            return false;
        }
        matches!(
            tok(j + 1),
            Tok::Ident(_)
                | Tok::Int(_)
                | Tok::Long(_)
                | Tok::Double(_)
                | Tok::Char(_)
                | Tok::Str(_)
                | Tok::Punct("(")
                | Tok::Punct("!")
                | Tok::Punct("~")
                | Tok::Kw("this" | "super" | "new" | "true" | "false" | "null")
        )
    }

    fn args_rest(&mut self) -> Result<Vec<Expr>> {
        // after "("
        let mut args = Vec::new();
        if self.eat_p(")") {
            return Ok(args);
        }
        loop {
            args.push(self.expr()?);
            if self.eat_p(")") {
                return Ok(args);
            }
            self.expect_p(",")?;
        }
    }

    fn primary(&mut self) -> Result<Expr> {
        let pos = self.pos();
        let kind = match self.bump() {
            Tok::Int(v) => ExprKind::Lit(Lit::Int(v)),
            Tok::Long(v) => ExprKind::Lit(Lit::Long(v)),
            Tok::Double(v) => ExprKind::Lit(Lit::Double(v)),
            Tok::Char(v) => ExprKind::Lit(Lit::Char(v)),
            Tok::Str(s) => ExprKind::Lit(Lit::Str(s)),
            Tok::Kw("true") => ExprKind::Lit(Lit::Bool(true)),
            Tok::Kw("false") => ExprKind::Lit(Lit::Bool(false)),
            Tok::Kw("null") => ExprKind::Lit(Lit::Null),
            Tok::Kw("this") => ExprKind::This,
            Tok::Kw("super") => {
                if !self.is_p(".") {
                    return err(self.file, pos, "'.' expected after 'super'");
                }
                ExprKind::Super
            }
            Tok::Punct("(") => {
                let e = self.expr()?;
                self.expect_p(")")?;
                return Ok(e);
            }
            Tok::Kw("new") => return self.creation(pos),
            Tok::Kw("switch") => return err(self.file, pos, "switch expressions are not supported"),
            Tok::Ident(name) => {
                if self.is_p("->") {
                    return err(self.file, pos, "lambda expressions are not supported");
                }
                if self.eat_p("(") {
                    let args = self.args_rest()?;
                    ExprKind::Call {
                        target: None,
                        name,
                        args,
                    }
                } else {
                    ExprKind::Name(name)
                }
            }
            Tok::Kw(k @ ("boolean" | "char" | "int" | "long" | "double" | "void"))
                if self.is_p(".") || self.is_p("[") =>
            {
                return err(
                    self.file,
                    pos,
                    format!("class literals ({}.class) are not supported", k),
                );
            }
            _ => {
                self.i -= 1;
                return self.fail(format!("illegal start of expression: {}", self.describe()));
            }
        };
        Ok(Expr { kind, pos })
    }

    fn creation(&mut self, pos: Pos) -> Result<Expr> {
        if self.is_p("<") {
            return self.fail("generics are not supported");
        }
        let base = if let Some(p) = self.prim_type()? {
            AstType::Prim(p)
        } else {
            let tpos = self.pos();
            let name = self.qualified_name()?;
            if self.is_p("<") {
                return self.fail("generics are not supported");
            }
            AstType::Named(name, tpos)
        };
        if self.eat_p("(") {
            if matches!(base, AstType::Prim(_)) {
                return err(self.file, pos, "'[' expected");
            }
            let args = self.args_rest()?;
            if self.is_p("{") {
                return self.fail("anonymous classes are not supported");
            }
            return Ok(Expr {
                pos,
                kind: ExprKind::New(base, args),
            });
        }
        if !self.is_p("[") {
            return self.fail("'(' or '[' expected");
        }
        let mut dims = Vec::new();
        let mut extra = 0;
        while self.is_p("[") {
            if self.is_p_at(1, "]") {
                self.i += 2;
                extra += 1;
            } else {
                if extra > 0 {
                    return self.fail("array dimension missing");
                }
                self.i += 1;
                dims.push(self.expr()?);
                self.expect_p("]")?;
            }
        }
        let init = if self.is_p("{") {
            if !dims.is_empty() {
                return self.fail("array creation with both dimension expression and initialization is illegal");
            }
            self.i += 1;
            Some(self.array_init_rest()?)
        } else {
            if dims.is_empty() {
                return self.fail("array dimension missing");
            }
            None
        };
        Ok(Expr {
            pos,
            kind: ExprKind::NewArray {
                elem: base,
                dims,
                extra_dims: extra,
                init,
            },
        })
    }

    fn postfix(&mut self, mut e: Expr) -> Result<Expr> {
        loop {
            let pos = self.pos();
            if self.eat_p(".") {
                if self.is_p("<") {
                    return self.fail("generic method calls are not supported");
                }
                if self.is_kw("class") {
                    return self.fail("class literals are not supported");
                }
                if self.is_kw("new") {
                    return self.fail("inner class creation is not supported");
                }
                if self.is_kw("this") {
                    return self.fail("qualified 'this' is not supported");
                }
                let npos = self.pos();
                let name = self.ident()?;
                if self.eat_p("(") {
                    let args = self.args_rest()?;
                    e = Expr {
                        pos: npos,
                        kind: ExprKind::Call {
                            target: Some(Box::new(e)),
                            name,
                            args,
                        },
                    };
                } else {
                    e = Expr {
                        pos: npos,
                        kind: ExprKind::Field(Box::new(e), name),
                    };
                }
            } else if self.is_p("[") {
                self.i += 1;
                let idx = self.expr()?;
                self.expect_p("]")?;
                e = Expr {
                    pos,
                    kind: ExprKind::Index(Box::new(e), Box::new(idx)),
                };
            } else if self.is_p("++") || self.is_p("--") {
                let inc = self.is_p("++");
                self.i += 1;
                e = Expr {
                    pos,
                    kind: ExprKind::IncDec {
                        pre: false,
                        inc,
                        target: Box::new(e),
                    },
                };
            } else if self.is_p("::") {
                return self.fail("method references are not supported");
            } else {
                return Ok(e);
            }
        }
    }
}
