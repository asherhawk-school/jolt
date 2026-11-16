//! Source positions and compiler diagnostics.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Debug)]
pub struct Diag {
    pub file: usize,
    pub pos: Pos,
    pub msg: String,
}

pub type Result<T> = std::result::Result<T, Diag>;

pub fn err<T>(file: usize, pos: Pos, msg: impl Into<String>) -> Result<T> {
    Err(Diag {
        file,
        pos,
        msg: msg.into(),
    })
}

/// Render a diagnostic javac-style, with the offending source line and a caret.
pub fn render(d: &Diag, path: &str, src: &str) -> String {
    let mut out = format!("{}:{}:{}: error: {}\n", path, d.pos.line, d.pos.col, d.msg);
    if d.pos.line > 0 {
        if let Some(line) = src.lines().nth(d.pos.line as usize - 1) {
            out.push_str(line);
            out.push('\n');
            let mut caret = String::new();
            for (i, ch) in line.chars().enumerate() {
                if i + 1 >= d.pos.col as usize {
                    break;
                }
                caret.push(if ch == '\t' { '\t' } else { ' ' });
            }
            caret.push('^');
            out.push_str(&caret);
            out.push('\n');
        }
    }
    out
}
