//! A small backtracking regex engine covering the java.util.regex features
//! that String.split / replaceAll / matches are typically used with:
//! literals, `.`, classes `[a-z]`, `[^...]`, `\d \s \w` (and negations),
//! `\b`, anchors, groups (capturing and `(?:...)`), alternation and the
//! quantifiers `* + ? {n} {n,} {n,m}` (greedy, lazy `?`, possessive treated as greedy).

#[derive(Debug, Clone)]
enum Node {
    Empty,
    Char(u16),
    Any,
    Class(Vec<(u32, u32)>, bool),
    Bol,
    Eol,
    WordB(bool),
    Group(Box<Node>, Option<usize>),
    Alt(Vec<Node>),
    Seq(Vec<Node>),
    Rep(Box<Node>, u32, u32, bool),
}

pub struct Regex {
    root: Node,
    pub groups: usize,
    literal: Option<Vec<u16>>,
}

pub type Caps = Vec<Option<(usize, usize)>>;

struct P<'a> {
    s: &'a [u16],
    i: usize,
    groups: usize,
}

const DIGIT: &[(u32, u32)] = &[(48, 57)];
const SPACE: &[(u32, u32)] = &[(9, 13), (32, 32)];
const WORD: &[(u32, u32)] = &[(48, 57), (65, 90), (95, 95), (97, 122)];

fn negate(r: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut lo = 0u32;
    let mut sorted = r.to_vec();
    sorted.sort();
    for (a, b) in sorted {
        if a > lo {
            out.push((lo, a - 1));
        }
        lo = lo.max(b + 1);
    }
    if lo <= 0xFFFF {
        out.push((lo, 0xFFFF));
    }
    out
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<u16> {
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c as u16) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn err<T>(&self, m: &str) -> Result<T, String> {
        Err(format!("{} near index {}", m, self.i))
    }

    fn alt(&mut self) -> Result<Node, String> {
        let mut alts = vec![self.seq()?];
        while self.eat('|') {
            alts.push(self.seq()?);
        }
        Ok(if alts.len() == 1 {
            alts.pop().unwrap()
        } else {
            Node::Alt(alts)
        })
    }

    fn seq(&mut self) -> Result<Node, String> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == b'|' as u16 || c == b')' as u16 {
                break;
            }
            let atom = self.atom()?;
            items.push(self.quant(atom)?);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap(),
            _ => Node::Seq(items),
        })
    }

    fn number(&mut self) -> Option<u32> {
        let start = self.i;
        let mut v: u32 = 0;
        while let Some(c) = self.peek() {
            if (48..=57).contains(&c) {
                v = v.saturating_mul(10).saturating_add((c - 48) as u32);
                self.i += 1;
            } else {
                break;
            }
        }
        if self.i == start {
            None
        } else {
            Some(v)
        }
    }

    fn quant(&mut self, atom: Node) -> Result<Node, String> {
        let (min, max) = match self.peek().map(|c| c as u8 as char) {
            Some('*') => {
                self.i += 1;
                (0, u32::MAX)
            }
            Some('+') => {
                self.i += 1;
                (1, u32::MAX)
            }
            Some('?') => {
                self.i += 1;
                (0, 1)
            }
            Some('{') => {
                let save = self.i;
                self.i += 1;
                match self.number() {
                    Some(lo) => {
                        let hi = if self.eat(',') {
                            self.number().unwrap_or(u32::MAX)
                        } else {
                            lo
                        };
                        if !self.eat('}') {
                            return self.err("Unclosed counted closure");
                        }
                        (lo, hi)
                    }
                    None => {
                        self.i = save;
                        return Ok(atom);
                    }
                }
            }
            _ => return Ok(atom),
        };
        let greedy = if self.eat('?') {
            false
        } else {
            self.eat('+');
            true
        };
        if matches!(atom, Node::Bol | Node::Eol) {
            return self.err("Dangling meta character");
        }
        Ok(Node::Rep(Box::new(atom), min, max, greedy))
    }

    fn escape_class(&mut self, c: u16) -> Option<(Vec<(u32, u32)>, bool)> {
        Some(match c as u8 {
            b'd' => (DIGIT.to_vec(), false),
            b'D' => (DIGIT.to_vec(), true),
            b's' => (SPACE.to_vec(), false),
            b'S' => (SPACE.to_vec(), true),
            b'w' => (WORD.to_vec(), false),
            b'W' => (WORD.to_vec(), true),
            _ => return None,
        })
    }

    fn escape_char(c: u16) -> u16 {
        match c as u8 {
            b't' => 9,
            b'n' => 10,
            b'r' => 13,
            b'f' => 12,
            b'0' => 0,
            b'e' => 27,
            _ => c,
        }
    }

    fn atom(&mut self) -> Result<Node, String> {
        let c = self.peek().unwrap();
        self.i += 1;
        Ok(match c as u8 as char {
            '.' if c < 128 => Node::Any,
            '^' if c < 128 => Node::Bol,
            '$' if c < 128 => Node::Eol,
            '(' if c < 128 => {
                let mut cap = true;
                if self.eat('?') {
                    if !self.eat(':') {
                        return self.err("Unsupported group construct");
                    }
                    cap = false;
                }
                let idx = if cap {
                    self.groups += 1;
                    Some(self.groups)
                } else {
                    None
                };
                let inner = self.alt()?;
                if !self.eat(')') {
                    return self.err("Unclosed group");
                }
                Node::Group(Box::new(inner), idx)
            }
            '[' if c < 128 => self.class()?,
            '*' | '+' | '?' if c < 128 => return self.err("Dangling meta character"),
            '\\' if c < 128 => {
                let Some(e) = self.peek() else {
                    return self.err("Unexpected internal error");
                };
                self.i += 1;
                if let Some((r, neg)) = self.escape_class(e) {
                    Node::Class(r, neg)
                } else if e == b'b' as u16 {
                    Node::WordB(true)
                } else if e == b'B' as u16 {
                    Node::WordB(false)
                } else {
                    Node::Char(Self::escape_char(e))
                }
            }
            _ => Node::Char(c),
        })
    }

    fn class(&mut self) -> Result<Node, String> {
        let neg = self.eat('^');
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = self.peek() else {
                return self.err("Unclosed character class");
            };
            if c == b']' as u16 && !first {
                self.i += 1;
                break;
            }
            first = false;
            self.i += 1;
            let lo = if c == b'\\' as u16 {
                let Some(e) = self.peek() else {
                    return self.err("Unclosed character class");
                };
                self.i += 1;
                if let Some((r, n)) = self.escape_class(e) {
                    if n {
                        ranges.extend(negate(&r));
                    } else {
                        ranges.extend(r);
                    }
                    continue;
                }
                Self::escape_char(e)
            } else if c == b'[' as u16 {
                return self.err("Nested character classes are not supported");
            } else {
                c
            };
            if self.peek() == Some(b'-' as u16) && self.s.get(self.i + 1).map_or(false, |&n| n != b']' as u16) {
                self.i += 1;
                let mut hi = self.peek().unwrap();
                self.i += 1;
                if hi == b'\\' as u16 {
                    hi = Self::escape_char(self.peek().unwrap_or(0));
                    self.i += 1;
                }
                if hi < lo {
                    return self.err("Illegal character range");
                }
                ranges.push((lo as u32, hi as u32));
            } else {
                ranges.push((lo as u32, lo as u32));
            }
        }
        Ok(Node::Class(ranges, neg))
    }
}

fn is_word(s: &[u16], i: usize) -> bool {
    s.get(i)
        .map_or(false, |&c| WORD.iter().any(|&(a, b)| (a..=b).contains(&(c as u32))))
}

type K<'k> = &'k mut dyn FnMut(usize, &mut Caps) -> bool;

impl Regex {
    pub fn new(pattern: &[u16]) -> Result<Regex, String> {
        let meta = |c: u16| c < 128 && b"\\^$.|?*+()[]{}".contains(&(c as u8));
        if !pattern.iter().any(|&c| meta(c)) {
            return Ok(Regex {
                root: Node::Empty,
                groups: 0,
                literal: Some(pattern.to_vec()),
            });
        }
        let mut p = P {
            s: pattern,
            i: 0,
            groups: 0,
        };
        let root = p.alt()?;
        if p.i < pattern.len() {
            return p.err("Unmatched closing ')'");
        }
        Ok(Regex {
            root,
            groups: p.groups,
            literal: None,
        })
    }

    fn m(&self, n: &Node, s: &[u16], i: usize, caps: &mut Caps, k: K) -> bool {
        match n {
            Node::Empty => k(i, caps),
            Node::Char(c) => s.get(i) == Some(c) && k(i + 1, caps),
            Node::Any => i < s.len() && s[i] != 10 && s[i] != 13 && k(i + 1, caps),
            Node::Class(r, neg) => {
                i < s.len() && (r.iter().any(|&(a, b)| (a..=b).contains(&(s[i] as u32))) != *neg) && k(i + 1, caps)
            }
            Node::Bol => i == 0 && k(i, caps),
            Node::Eol => i == s.len() && k(i, caps),
            Node::WordB(want) => {
                let at = i > 0 && is_word(s, i - 1) != is_word(s, i) || i == 0 && is_word(s, 0);
                at == *want && k(i, caps)
            }
            Node::Group(inner, idx) => self.m(inner, s, i, caps, &mut |j, c: &mut Caps| match idx {
                Some(g) => {
                    let old = c[*g];
                    c[*g] = Some((i, j));
                    if k(j, c) {
                        true
                    } else {
                        c[*g] = old;
                        false
                    }
                }
                None => k(j, c),
            }),
            Node::Alt(alts) => alts.iter().any(|a| self.m(a, s, i, caps, k)),
            Node::Seq(items) => self.seq(items, s, i, caps, k),
            Node::Rep(inner, min, max, greedy) => self.rep(inner, *min, *max, *greedy, 0, s, i, caps, k),
        }
    }

    fn seq(&self, items: &[Node], s: &[u16], i: usize, caps: &mut Caps, k: K) -> bool {
        match items.split_first() {
            None => k(i, caps),
            Some((first, rest)) => self.m(first, s, i, caps, &mut |j, c: &mut Caps| self.seq(rest, s, j, c, k)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rep(
        &self,
        n: &Node,
        min: u32,
        max: u32,
        greedy: bool,
        count: u32,
        s: &[u16],
        i: usize,
        caps: &mut Caps,
        k: K,
    ) -> bool {
        if greedy {
            if count < max
                && self.m(n, s, i, caps, &mut |j, c: &mut Caps| {
                    (j != i || count < min) && self.rep(n, min, max, greedy, count + 1, s, j, c, k)
                })
            {
                return true;
            }
            count >= min && k(i, caps)
        } else {
            if count >= min && k(i, caps) {
                return true;
            }
            count < max
                && self.m(n, s, i, caps, &mut |j, c: &mut Caps| {
                    (j != i || count < min) && self.rep(n, min, max, greedy, count + 1, s, j, c, k)
                })
        }
    }

    /// Leftmost match starting at or after `from`: (start, end, captures).
    pub fn find(&self, s: &[u16], from: usize) -> Option<(usize, usize, Caps)> {
        if let Some(lit) = &self.literal {
            if lit.is_empty() {
                return (from <= s.len()).then(|| (from, from, vec![None]));
            }
            if lit.len() > s.len() {
                return None;
            }
            return (from..=s.len() - lit.len())
                .find(|&i| s[i..].starts_with(lit))
                .map(|i| (i, i + lit.len(), vec![None]));
        }
        for start in from..=s.len() {
            let mut caps: Caps = vec![None; self.groups + 1];
            let mut found: Option<(usize, Caps)> = None;
            if self.m(&self.root, s, start, &mut caps, &mut |j, c: &mut Caps| {
                found = Some((j, c.clone()));
                true
            }) {
                let (e, mut caps) = found.unwrap();
                caps[0] = Some((start, e));
                return Some((start, e, caps));
            }
        }
        None
    }

    pub fn matches(&self, s: &[u16]) -> bool {
        if let Some(lit) = &self.literal {
            return lit == s;
        }
        let mut caps: Caps = vec![None; self.groups + 1];
        self.m(&self.root, s, 0, &mut caps, &mut |j, _c: &mut Caps| j == s.len())
    }

    /// `String.split(regex, 0)` semantics.
    pub fn split(&self, s: &[u16]) -> Vec<Vec<u16>> {
        let mut out = Vec::new();
        let mut index = 0;
        let mut from = 0;
        while from <= s.len() {
            let Some((st, en, _)) = self.find(s, from) else { break };
            if en == st {
                // zero-width match: never split at the very start, step past it
                if st == 0 || st >= s.len() {
                    from = st + 1;
                    continue;
                }
            }
            if !(index == 0 && st == 0 && en == 0) {
                out.push(s[index..st].to_vec());
                index = en;
            }
            from = if en > st { en } else { en + 1 };
        }
        if index == 0 && out.is_empty() {
            return vec![s.to_vec()];
        }
        out.push(s[index..].to_vec());
        while out.len() > 0 && out.last().unwrap().is_empty() {
            out.pop();
        }
        out
    }

    /// `String.replaceAll(regex, replacement)` semantics ($n group references, \ escapes).
    pub fn replace_all(&self, s: &[u16], rep: &[u16]) -> Result<Vec<u16>, String> {
        let mut out = Vec::new();
        let mut last = 0;
        let mut from = 0;
        while from <= s.len() {
            let Some((st, en, caps)) = self.find(s, from) else {
                break;
            };
            out.extend_from_slice(&s[last..st]);
            let mut i = 0;
            while i < rep.len() {
                let c = rep[i];
                if c == b'\\' as u16 {
                    i += 1;
                    if i >= rep.len() {
                        return Err("character to be escaped is missing".into());
                    }
                    out.push(rep[i]);
                } else if c == b'$' as u16 {
                    i += 1;
                    let mut g: usize = 0;
                    let start = i;
                    while i < rep.len() && (48..=57).contains(&rep[i]) {
                        let ng = g * 10 + (rep[i] - 48) as usize;
                        if ng > self.groups && i > start {
                            break;
                        }
                        g = ng;
                        i += 1;
                    }
                    if i == start {
                        return Err("Illegal group reference".into());
                    }
                    if g > self.groups {
                        return Err(format!("No group {}", g));
                    }
                    if let Some(Some((a, b))) = caps.get(g) {
                        out.extend_from_slice(&s[*a..*b]);
                    }
                    continue;
                } else {
                    out.push(c);
                }
                i += 1;
            }
            last = en;
            if en == st {
                if st < s.len() {
                    out.push(s[st]);
                }
                last = st + 1;
                from = st + 1;
            } else {
                from = en;
            }
        }
        if last <= s.len() {
            out.extend_from_slice(&s[last.min(s.len())..]);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }
    fn split(re: &str, s: &str) -> Vec<String> {
        Regex::new(&u(re))
            .unwrap()
            .split(&u(s))
            .iter()
            .map(|v| String::from_utf16_lossy(v))
            .collect()
    }
    #[test]
    fn splits() {
        assert_eq!(split(",", "one,two,,three"), vec!["one", "two", "", "three"]);
        assert_eq!(split(",", "a,b,,"), vec!["a", "b"]);
        assert_eq!(split("\\s+", "  hello   world "), vec!["", "hello", "world"]);
        assert_eq!(split("", "abc"), vec!["a", "b", "c"]);
        assert_eq!(split("[,;]\\s*", "a, b;c"), vec!["a", "b", "c"]);
        assert_eq!(split("x", "abc"), vec!["abc"]);
    }
    #[test]
    fn replace_and_match() {
        let r = Regex::new(&u("(\\w+)@(\\w+)")).unwrap();
        assert_eq!(
            String::from_utf16_lossy(&r.replace_all(&u("me@home you@work"), &u("$2:$1")).unwrap()),
            "home:me work:you"
        );
        assert!(Regex::new(&u("[a-z]+\\d{2,3}")).unwrap().matches(&u("abc12")));
        assert!(!Regex::new(&u("[a-z]+\\d{2,3}")).unwrap().matches(&u("abc1234")));
        assert!(Regex::new(&u("a|bc*")).unwrap().matches(&u("bccc")));
        let r = Regex::new(&u("\\s+")).unwrap();
        assert_eq!(
            String::from_utf16_lossy(&r.replace_all(&u("a  b   c"), &u(" ")).unwrap()),
            "a b c"
        );
    }
}
