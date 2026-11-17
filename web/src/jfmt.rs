//! Java-compatible number formatting.
//!
//! Java prints doubles from their shortest round-trip decimal digits, and
//! `String.format("%.2f")` rounds those digits HALF_UP. Both differ from Rust's
//! defaults, so they are reimplemented here.

/// Shortest round-trip digits of a finite, non-zero |d|: (digits, exponent)
/// meaning d = 0.d1d2d3... × 10^exponent.
fn digits(d: f64) -> (Vec<u8>, i32) {
    let s = format!("{:e}", d.abs());
    let (mant, exp) = s.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let mut ds: Vec<u8> = mant.bytes().filter(|b| b.is_ascii_digit()).collect();
    let mut exp: i32 = exp;
    if ds.len() == 1 {
        // Java always shows two significant digits (d.d) and picks the
        // two-digit value closest to the exact one, e.g. 4.9E-324 not 5.0E-324.
        let two = format!("{:.1e}", d.abs());
        if two.parse::<f64>().ok() == Some(d.abs()) {
            let (m2, e2) = two.split_once('e').unwrap();
            let d2: Vec<u8> = m2.bytes().filter(|b| b.is_ascii_digit()).collect();
            if d2.len() == 2 && d2[1] != b'0' {
                ds = d2;
                exp = e2.parse().unwrap();
            }
        }
    }
    (ds, exp + 1)
}

/// `Double.toString(d)`.
pub fn double_to_string(d: f64) -> String {
    if d.is_nan() {
        return "NaN".into();
    }
    if d.is_infinite() {
        return if d > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if d == 0.0 {
        return if d.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    let (ds, p) = digits(d);
    let mut out = String::new();
    if d < 0.0 {
        out.push('-');
    }
    let a = d.abs();
    if (1e-3..1e7).contains(&a) {
        if p > 0 {
            let p = p as usize;
            for i in 0..p {
                out.push(*ds.get(i).unwrap_or(&b'0') as char);
            }
            out.push('.');
            if ds.len() > p {
                out.extend(ds[p..].iter().map(|&b| b as char));
            } else {
                out.push('0');
            }
        } else {
            out.push_str("0.");
            for _ in 0..(-p) {
                out.push('0');
            }
            out.extend(ds.iter().map(|&b| b as char));
        }
    } else {
        out.push(ds[0] as char);
        out.push('.');
        if ds.len() > 1 {
            out.extend(ds[1..].iter().map(|&b| b as char));
        } else {
            out.push('0');
        }
        out.push('E');
        out.push_str(&(p - 1).to_string());
    }
    out
}

/// Round digit string `ds` (value 0.ds × 10^p) HALF_UP to keep `keep` digits.
/// Returns new digits (exactly `keep` long, or keep+1 on carry) and new p.
fn round_half_up(ds: &[u8], p: i32, keep: i32) -> (Vec<u8>, i32) {
    if keep < 0 {
        return (vec![], p);
    }
    let keep = keep as usize;
    if keep >= ds.len() {
        let mut v = ds.to_vec();
        v.resize(keep, b'0');
        return (v, p);
    }
    let mut v = ds[..keep].to_vec();
    if ds[keep] >= b'5' {
        let mut i = keep;
        loop {
            if i == 0 {
                v.insert(0, b'1');
                return (v, p + 1);
            }
            i -= 1;
            if v[i] == b'9' {
                v[i] = b'0';
            } else {
                v[i] += 1;
                break;
            }
        }
    }
    (v, p)
}

/// `%.{prec}f` without sign handling; `a` must be finite and >= 0.
pub fn fixed(a: f64, prec: usize) -> String {
    if a == 0.0 {
        return if prec == 0 {
            "0".into()
        } else {
            format!("0.{}", "0".repeat(prec))
        };
    }
    let (ds, p) = digits(a);
    let (ds, p) = round_half_up(&ds, p, p + prec as i32);
    // ds now holds every digit down to 10^-prec (may be empty -> zero)
    let total = (p + prec as i32).max(0) as usize;
    let mut all = ds;
    all.resize(total, b'0');
    let int_len = p.max(0) as usize;
    let mut out = String::new();
    if int_len == 0 {
        out.push('0');
    } else {
        out.extend(all[..int_len].iter().map(|&b| b as char));
    }
    if prec > 0 {
        out.push('.');
        if p < 0 {
            for _ in 0..((-p) as usize).min(prec) {
                out.push('0');
            }
            let rest = prec - ((-p) as usize).min(prec);
            out.extend(all.iter().take(rest).map(|&b| b as char));
        } else {
            out.extend(all[int_len..].iter().map(|&b| b as char));
        }
    }
    out
}

/// `%.{prec}e` without sign handling; `a` must be finite and >= 0.
pub fn sci(a: f64, prec: usize, upper: bool) -> String {
    let (ds, exp) = if a == 0.0 {
        (vec![b'0'; prec + 1], 0)
    } else {
        let (ds, p) = digits(a);
        let (ds, p) = round_half_up(&ds, p, (prec + 1) as i32);
        (ds, p - 1)
    };
    let mut out = String::new();
    out.push(ds[0] as char);
    if prec > 0 {
        out.push('.');
        out.extend(ds[1..=prec].iter().map(|&b| b as char));
    }
    out.push(if upper { 'E' } else { 'e' });
    out.push(if exp < 0 { '-' } else { '+' });
    out.push_str(&format!("{:02}", exp.abs()));
    out
}

/// `%g` (Java never strips trailing zeros).
pub fn general(a: f64, prec: usize, upper: bool) -> String {
    let prec = if prec == 0 { 1 } else { prec };
    if a == 0.0 {
        return fixed(a, prec - 1);
    }
    let (ds, p) = digits(a);
    let (_, p2) = round_half_up(&ds, p, prec as i32);
    let x = p2 - 1;
    if a >= 1e-4 && x < prec as i32 {
        fixed(a, (prec as i32 - 1 - x).max(0) as usize)
    } else {
        sci(a, prec - 1, upper)
    }
}

/// Insert grouping commas into a run of integer digits.
pub fn group(int_digits: &str) -> String {
    let n = int_digits.len();
    let mut out = String::with_capacity(n + n / 3);
    for (i, c) in int_digits.chars().enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn radix(mut v: u64, radix: u32) -> String {
    if v == 0 {
        return "0".into();
    }
    let mut buf = Vec::new();
    while v > 0 {
        buf.push(std::char::from_digit((v % radix as u64) as u32, radix).unwrap());
        v /= radix as u64;
    }
    buf.iter().rev().collect()
}

pub fn signed_radix(v: i64, r: u32) -> String {
    if v < 0 {
        format!("-{}", radix(v.unsigned_abs(), r))
    } else {
        radix(v as u64, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn doubles() {
        assert_eq!(double_to_string(1.0), "1.0");
        assert_eq!(double_to_string(100.0), "100.0");
        assert_eq!(double_to_string(1e10), "1.0E10");
        assert_eq!(double_to_string(1.5e-7), "1.5E-7");
        assert_eq!(double_to_string(0.001), "0.001");
        assert_eq!(double_to_string(1234567.0), "1234567.0");
        assert_eq!(double_to_string(12345678.0), "1.2345678E7");
        assert_eq!(double_to_string(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(double_to_string(-2.5), "-2.5");
    }
    #[test]
    fn formats() {
        assert_eq!(fixed(3.14159, 3), "3.142");
        assert_eq!(fixed(0.125, 2), "0.13");
        assert_eq!(fixed(2.675, 2), "2.68");
        assert_eq!(fixed(0.0004, 2), "0.00");
        assert_eq!(fixed(9.999, 2), "10.00");
        assert_eq!(fixed(0.05, 1), "0.1");
        assert_eq!(fixed(123.0, 0), "123");
        assert_eq!(sci(12.3456, 6, false), "1.234560e+01");
        assert_eq!(group("1234567"), "1,234,567");
    }
}
