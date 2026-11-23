//! Globs: `*`, `**`, `?`, `{a,b}` over paths with `/`.

pub struct Glob {
    alts: Vec<Vec<String>>,
}

pub fn has_wild(p: &str) -> bool {
    p.contains(['*', '?', '{'])
}

impl Glob {
    pub fn new(p: &str) -> Glob {
        let alts = expand(p).iter().map(|a| a.split('/').map(String::from).collect()).collect();
        Glob { alts }
    }

    pub fn matches(&self, path: &str) -> bool {
        let segs: Vec<&str> = path.split('/').collect();
        self.alts.iter().any(|a| {
            let p: Vec<&str> = a.iter().map(|s| s.as_str()).collect();
            segs_match(&p, &segs)
        })
    }
}

pub fn expand(p: &str) -> Vec<String> {
    let Some(i) = p.find('{') else { return vec![p.to_string()] };
    let (mut depth, mut end) = (0, None);
    let mut cuts = vec![i];
    for (j, c) in p[i..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i + j);
                    break;
                }
            }
            ',' if depth == 1 => cuts.push(i + j),
            _ => {}
        }
    }
    let Some(end) = end else { return vec![p.to_string()] };
    cuts.push(end);
    let (pre, suf) = (&p[..i], &p[end + 1..]);
    let mut out = Vec::new();
    for w in cuts.windows(2) {
        out.extend(expand(&format!("{}{}{}", pre, &p[w[0] + 1..w[1]], suf)));
    }
    out
}

fn segs_match(p: &[&str], s: &[&str]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(&"**") => (0..=s.len()).any(|k| segs_match(&p[1..], &s[k..])),
        Some(seg) => !s.is_empty() && seg_match(seg, s[0]) && segs_match(&p[1..], &s[1..]),
    }
}

fn seg_match(pat: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pat.chars().collect(), text.chars().collect());
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == '?' || p[i] == t[j]) && p[i] != '*' {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    p[i..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(g: &str, p: &str) -> bool {
        Glob::new(g).matches(p)
    }

    #[test]
    fn literals_and_simple_wildcards() {
        assert!(m("a/b.py", "a/b.py"));
        assert!(!m("a/b.py", "a/c.py"));
        assert!(m("src/*.rs", "src/x.rs"));
        assert!(!m("src/*.rs", "src/a/x.rs"));
        assert!(m("*", "x"));
        assert!(!m("*", "a/x"));
        assert!(m("a*c", "abc") && m("a*c", "ac") && !m("a*c", "ab"));
        assert!(m("a?c", "abc") && !m("a?c", "ac") && !m("a?c", "a/c"));
        assert!(m("*a*b*", "xxaxxbxx") && !m("*a*b*", "xbxa"));
    }

    #[test]
    fn double_asterisk() {
        assert!(m("a/**", "a/b") && m("a/**", "a/b/c/d"));
        assert!(!m("a/**", "b/a/x"));
        assert!(m("**/x.py", "x.py") && m("**/x.py", "a/b/x.py"));
        assert!(!m("**/x.py", "a/y.py"));
        assert!(m("a/**/z", "a/z") && m("a/**/z", "a/b/c/z"));
        assert!(m("**", "any/thing"));
        assert!(m("**/*.md", "README.md") && m("**/*.md", "d/e/R.md"));
    }

    #[test]
    fn keys() {
        assert!(m("src/{a,b}.py", "src/a.py") && m("src/{a,b}.py", "src/b.py"));
        assert!(!m("src/{a,b}.py", "src/c.py"));
        assert!(m("{src,lib}/**", "lib/x/y"));
        assert!(m("{a/b,c}/*.rs", "a/b/x.rs") && m("{a/b,c}/*.rs", "c/x.rs"));
        assert!(m("x.{r*,py}", "x.rs") && m("x.{r*,py}", "x.py"));
        assert!(m("{a,{b,c}}", "c"));
        assert!(m("a{,b}", "a") && m("a{,b}", "ab"));
        assert!(m("a{b", "a{b")); // an unclosed brace is literal
    }
}
