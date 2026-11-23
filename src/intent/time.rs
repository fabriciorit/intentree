//! Time literal: deadlines and periods with a checked number and unit.
//!
//! Form: `"<number> <unit>"` or a range `"<number> <unit> to <number> <unit>"`.
//! Units: ms, s, min, h, d. The canonical form is the English one (`to`, number and unit
//! separated by a space); `a` in place of `to` and a number glued to the unit (`30s`) are also read.
//! Inside the quotes there is nothing to disambiguate: no unit is called `a` or `to`.

use super::json::number;

/// Accepted units and how many seconds each is worth.
pub const UNITS: [(&str, f64); 5] = [("ms", 0.001), ("s", 1.0), ("min", 60.0), ("h", 3600.0), ("d", 86400.0)];

/// A value: number and unit.
fn value(t: &str) -> Result<(f64, &'static str), String> {
    let t = t.trim();
    let end = t.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(t.len());
    let (num, unit) = (&t[..end], t[end..].trim());
    let ok_num = !num.is_empty() && !num.starts_with('.') && !num.ends_with('.') && num.matches('.').count() <= 1;
    let x: f64 = match num.parse() {
        Ok(x) if ok_num => x,
        _ => return Err(format!("expected a number and a unit in '{t}'")),
    };
    match UNITS.iter().find(|(u, _)| *u == unit) {
        Some((u, _)) => Ok((x, u)),
        None if unit.is_empty() => Err(format!("missing unit in '{t}' (use ms, s, min, h or d)")),
        None => Err(format!("invalid time unit '{unit}' (use ms, s, min, h or d)")),
    }
}

fn seconds(x: f64, u: &str) -> f64 {
    let factor = UNITS.iter().find(|(v, _)| *v == u).map(|(_, f)| *f).unwrap_or(1.0);
    x * factor
}

/// Reads the literal: returns the canonical form and the seconds (one value, or start and end).
pub fn read_text(lit: &str) -> Result<(String, Vec<f64>), String> {
    let parts: Vec<&str> = lit.split_whitespace().collect();
    // range separator: the word `to` or `a`, alone
    let cut_at = parts.iter().position(|p| *p == "to" || *p == "a");
    let values: Vec<(f64, &str)> = match cut_at {
        None => vec![value(lit)?],
        Some(i) => vec![value(&parts[..i].join(" "))?, value(&parts[i + 1..].join(" "))?],
    };
    let canon: Vec<String> = values.iter().map(|(x, u)| format!("{} {u}", number(*x))).collect();
    let seg: Vec<f64> = values.iter().map(|(x, u)| seconds(*x, u)).collect();
    if seg.len() == 2 && seg[0] > seg[1] {
        return Err(format!("reversed time range: '{lit}' (the start is after the end)"));
    }
    Ok((canon.join(" to "), seg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_of_time() {
        assert_eq!(read_text("30 s"), Ok(("30 s".into(), vec![30.0])));
        assert_eq!(read_text("30s"), Ok(("30 s".into(), vec![30.0])));
        assert_eq!(read_text("1 s a 60 s"), Ok(("1 s to 60 s".into(), vec![1.0, 60.0])));
        assert_eq!(read_text("500 ms to 2 min"), Ok(("500 ms to 2 min".into(), vec![0.5, 120.0])));
        assert_eq!(read_text("1.5 h"), Ok(("1.5 h".into(), vec![5400.0])));
        assert_eq!(read_text("2 d"), Ok(("2 d".into(), vec![172800.0])));
        assert!(read_text("5 segundos").unwrap_err().contains("invalid time unit 'segundos'"));
        assert!(read_text("5").unwrap_err().contains("missing unit"));
        assert!(read_text("ttl s").is_err());
        assert!(read_text("60 s to 1 s").unwrap_err().contains("reversed"));
        assert!(read_text("1 s to").is_err());
        assert!(read_text("1. s").is_err());
    }
}
