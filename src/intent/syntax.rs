//! Lexicon tokens (longest-sequence matching) and the regular syntax of `.intent`.

use super::json::repr_text;
use super::lexicon::{Lexicon, Tok};

/// Noun phrase: (kind, name).
pub type Np = (String, String);

/// The other side of a comparison: a number or a noun phrase.
#[derive(Clone, Debug)]
pub enum ValueQ {
    Num(f64),
    Np(Np),
}

#[derive(Clone, Debug)]
pub enum Clause {
    /// comparison (lexicon op, or the copula's `==`/`!=`) against a number or another value
    Cmp { subject: Np, op: String, value: ValueQ },
    Adj { subject: Np, adj: String },
    /// declared absence: `The field "x" has no limit.`
    Abs { subject: Np, absence: String },
    Rel { subject: Np, verb: String, mode: Option<String>, obj: Vec<Np>, literals: Option<Vec<String>>, roles: Vec<(String, Np)> },
}

#[derive(Clone, Debug)]
pub enum Sentence {
    Simple(Clause),
    Cond { cond: String, when: Clause, then: Clause },
    /// quantified: `Every strategy in the registry "R" receives ...`; the subject of `o` is
    /// the marker (kind, "") and applies to every member of kind `kind` contained in `scope`
    All { kind: String, scope: Np, o: Clause },
}

/// Splits text into lines at every Unicode line boundary (\n, \r, \r\n, VT, FF, FS, GS, RS, NEL, LS, PS); no trailing empty line.
pub fn split_lines(t: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = t.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let line_break = matches!(c, '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}');
        if line_break {
            out.push(&t[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r' {
                if let Some(&(j, '\n')) = it.peek() {
                    it.next();
                    end = j + 1;
                }
            }
            start = end;
        }
    }
    if start < t.len() {
        out.push(&t[start..]);
    }
    out
}

/// Whitespace for trimming: Unicode whitespace plus the separators U+001C to U+001F.
fn blank(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

pub fn trim_blank(t: &str) -> &str {
    t.trim_matches(blank)
}

/// Splits the anchors that come after the period: `Sentence. ^file#symbol ^other`.
/// The cut is at the first `^` outside quotes; what remains must be only anchors (`^` followed
/// by a path, with an optional `#symbol`, no spaces). Returns the sentence (without the anchors) and the
/// anchors without the `^`, in written order.
pub fn split_anchors(line: &str) -> Result<(&str, Vec<String>), String> {
    let mut inside = false;
    let mut cut_at = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => inside = !inside,
            '^' if !inside => {
                cut_at = Some(i);
                break;
            }
            _ => {}
        }
    }
    let Some(i) = cut_at else { return Ok((line, Vec::new())) };
    let mut anchors = Vec::new();
    for t in line[i..].split(blank).filter(|t| !t.is_empty()) {
        match t.strip_prefix('^') {
            Some(a) if !a.is_empty() && !a.starts_with('#') && !a.ends_with('#') => anchors.push(a.to_string()),
            _ => return Err(format!("malformed anchor: {} (use ^file or ^file#symbol, separated by spaces, after the period)", repr_text(t))),
        }
    }
    Ok((trim_blank(&line[..i]), anchors))
}

enum RawMatch {
    Token(Tok),
    W(String),
}

/// A match of the pattern `"([^"]*)"|(\d+(?:\.\d+)?)|([,.])|([A-Za-z][A-Za-z/_-]*)` at `i`.
fn matches_words(b: &[u8], s: &str, i: usize) -> Option<(usize, RawMatch)> {
    let c = b[i];
    if c == b'"' {
        let end = s[i + 1..].find('"')? + i + 1;
        return Some((end + 1, RawMatch::Token(Tok::Name(s[i + 1..end].to_string()))));
    }
    if c.is_ascii_digit() {
        let mut j = i;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j + 1 < b.len() && b[j] == b'.' && b[j + 1].is_ascii_digit() {
            j += 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
        }
        let x: f64 = s[i..j].parse().ok()?;
        return Some((j, RawMatch::Token(Tok::Num(x))));
    }
    if c == b',' {
        return Some((i + 1, RawMatch::Token(Tok::Comma)));
    }
    if c == b'.' {
        return Some((i + 1, RawMatch::Token(Tok::End)));
    }
    if c.is_ascii_alphabetic() {
        let mut j = i + 1;
        while j < b.len() && (b[j].is_ascii_alphabetic() || matches!(b[j], b'/' | b'_' | b'-')) {
            j += 1;
        }
        return Some((j, RawMatch::W(s[i..j].to_ascii_lowercase())));
    }
    None
}

/// Raw match with the span (bytes) it came from.
struct Excerpt {
    b: RawMatch,
    start: usize,
    end: usize,
}

fn raws(line: &str) -> Result<Vec<Excerpt>, String> {
    let b = line.as_bytes();
    let mut raws = Vec::new();
    let (mut pos, mut i) = (0, 0);
    while i < b.len() {
        match matches_words(b, line, i) {
            Some((end, t)) => {
                let gap = trim_blank(&line[pos..i]);
                if !gap.is_empty() {
                    return Err(format!("unexpected character: {}", repr_text(gap)));
                }
                raws.push(Excerpt { b: t, start: i, end });
                pos = end;
                i = end;
            }
            None => {
                i += 1;
                while i < b.len() && !line.is_char_boundary(i) {
                    i += 1;
                }
            }
        }
    }
    let rest = trim_blank(&line[pos..]);
    if !rest.is_empty() {
        return Err(format!("unexpected character: {}", repr_text(rest)));
    }
    Ok(raws)
}

/// Tokens with the span (bytes) of the line each one came from.
pub struct Parsed {
    pub toks: Vec<Tok>,
    pub spans: Vec<(usize, usize)>,
}

/// Tokenization failure: the message and, when it is a word outside the lexicon, where it starts.
pub struct ErrorTok {
    pub msg: String,
    pub word: Option<(usize, String)>,
}

#[cfg(test)]
pub fn tokenize(line: &str, lex: &Lexicon) -> Result<Vec<Tok>, String> {
    tokenize_spans(line, lex).map(|l| l.toks).map_err(|e| e.msg)
}

pub fn tokenize_spans(line: &str, lex: &Lexicon) -> Result<Parsed, ErrorTok> {
    let raws = raws(line).map_err(|msg| ErrorTok { msg, word: None })?;
    // longest-sequence matching of words against the lexicon
    let mut output = Vec::new();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < raws.len() {
        let w = match &raws[i].b {
            RawMatch::Token(t) => {
                output.push(t.clone());
                spans.push((raws[i].start, raws[i].end));
                i += 1;
                continue;
            }
            RawMatch::W(w) => w,
        };
        let mut found = false;
        for n in (1..=lex.larger).rev() {
            if i + n > raws.len() {
                continue;
            }
            let word_set: Option<Vec<&str>> = raws[i..i + n]
                .iter()
                .map(|p| match &p.b {
                    RawMatch::W(w) => Some(w.as_str()),
                    _ => None,
                })
                .collect();
            let Some(word_set) = word_set else { continue };
            let sentence = word_set.join(" ");
            // base form of a verb that is also a type (`does not record`): a verb right after the negation
            let base = (output.last() == Some(&Tok::Neg)).then(|| lex.bases_ambiguous.get(&sentence)).flatten();
            if let Some(t) = base.map(|id| Tok::Verb(id.clone())).as_ref().or_else(|| lex.sentences.get(&sentence)) {
                output.push(t.clone());
                spans.push((raws[i].start, raws[i + n - 1].end));
                i += n;
                found = true;
                break;
            }
        }
        if !found {
            return Err(ErrorTok { msg: format!("word outside the lexicon: '{w}'"), word: Some((raws[i].start, w.clone())) });
        }
    }
    Ok(Parsed { toks: output, spans })
}

/// All the tokenizations of the line against the lexicon (not only the longest-match one):
/// basis of the uniqueness test, which requires that only one of them has an analysis.
#[cfg(test)]
pub fn tokenizations(line: &str, lex: &Lexicon) -> Vec<Vec<Tok>> {
    fn rec(b: &[Excerpt], i: usize, lex: &Lexicon, current: &mut Vec<Tok>, out: &mut Vec<Vec<Tok>>) {
        if out.len() > 10_000 {
            return;
        }
        if i == b.len() {
            out.push(current.clone());
            return;
        }
        if let RawMatch::Token(t) = &b[i].b {
            current.push(t.clone());
            rec(b, i + 1, lex, current, out);
            current.pop();
            return;
        }
        let mut word_set: Vec<&str> = Vec::new();
        for j in i..b.len().min(i + lex.larger) {
            let RawMatch::W(w) = &b[j].b else { break };
            word_set.push(w);
            if let Some(t) = lex.sentences.get(&word_set.join(" ")) {
                current.push(t.clone());
                rec(b, j + 1, lex, current, out);
                current.pop();
            }
        }
    }
    let Ok(b) = raws(line) else { return Vec::new() };
    let mut out = Vec::new();
    rec(&b, 0, lex, &mut Vec::new(), &mut out);
    out
}

/// Syntax failure, with the position (index of the token where it stopped) and what was expected there.
#[derive(Clone, Debug)]
pub struct ErrorF {
    pub pos: usize,
    /// token classes accepted at the position (the first is the one the parser asked for)
    pub expected: Vec<&'static str>,
    /// own message, when the failure is not "expected X"
    pub text: Option<String>,
}

impl ErrorF {
    fn msg(pos: usize, text: impl Into<String>) -> ErrorF {
        ErrorF { pos, expected: Vec::new(), text: Some(text.into()) }
    }

    /// The usual short message ("expected NOUN, found NAME"), used by the tests.
    #[cfg(test)]
    fn short_message(&self, toks: &[Tok]) -> String {
        if let Some(t) = &self.text {
            return t.clone();
        }
        let finding = toks.get(self.pos).map(Tok::class).unwrap_or("end");
        format!("expected {}, found {finding}", self.expected.join(" or "))
    }
}

struct Cursor<'a> {
    t: &'a [Tok],
    i: usize,
}

impl Cursor<'_> {
    fn ve(&self, classes: &[&str]) -> bool {
        self.i < self.t.len() && classes.contains(&self.t[self.i].class())
    }

    fn take(&mut self, class: &'static str) -> Result<Tok, ErrorF> {
        if !self.ve(&[class]) {
            return Err(ErrorF { pos: self.i, expected: vec![class], text: None });
        }
        self.i += 1;
        Ok(self.t[self.i - 1].clone())
    }

    fn text(&mut self, class: &'static str) -> Result<String, ErrorF> {
        match self.take(class)? {
            Tok::Cond(s) | Tok::Prep(s) | Tok::Noun(s) | Tok::Verb(s) | Tok::VerbNeg(s) | Tok::Adj(s) | Tok::Cmp(s) | Tok::Abs(s) | Tok::Name(s) => Ok(s),
            _ => unreachable!("class without text"),
        }
    }
}

fn np(c: &mut Cursor) -> Result<Np, ErrorF> {
    c.take("ART")?;
    let kind = c.text("NOUN")?;
    let name = c.text("NAME")?;
    Ok((kind, name))
}

fn name(c: &mut Cursor) -> Result<String, ErrorF> {
    c.text("NAME")
}

/// Coordination: 'X', 'X and Y' or 'X, Y and Z'. A comma list only counts if it ends
/// in 'and'; otherwise it falls back to a single item (the comma may be the conditional clause's).
fn list<T>(c: &mut Cursor, item: fn(&mut Cursor) -> Result<T, ErrorF>) -> Result<Vec<T>, ErrorF> {
    let mut items = vec![item(c)?];
    let after_of_first = c.i;
    while c.ve(&["COMMA"]) && c.i + 1 < c.t.len() && matches!(c.t[c.i + 1], Tok::Art | Tok::Name(_)) {
        c.take("COMMA")?;
        match item(c) {
            Ok(x) => items.push(x),
            Err(_) => break,
        }
        if !c.ve(&["COMMA", "AND"]) {
            break;
        }
    }
    if c.ve(&["AND"]) {
        c.take("AND")?;
        items.push(item(c)?);
        return Ok(items);
    }
    c.i = after_of_first;
    items.truncate(1);
    Ok(items)
}

/// Number or noun phrase after a comparison.
fn value_q(c: &mut Cursor) -> Result<ValueQ, ErrorF> {
    if c.ve(&["ART"]) {
        return Ok(ValueQ::Np(np(c)?));
    }
    match c.take("NUM")? {
        Tok::Num(x) => Ok(ValueQ::Num(x)),
        _ => unreachable!(),
    }
}

fn clause(c: &mut Cursor) -> Result<Clause, ErrorF> {
    let subject = np(c)?;
    clause_of(c, subject)
}

/// The rest of the clause after the subject (noun phrase or quantified subject).
fn clause_of(c: &mut Cursor, subject: Np) -> Result<Clause, ErrorF> {
    if c.ve(&["CMP"]) {
        let op = c.text("CMP")?;
        return Ok(Clause::Cmp { subject, op, value: value_q(c)? });
    }
    if c.ve(&["IS"]) {
        c.take("IS")?;
        // equality: 'is the value "x"' or 'is 3'
        if c.ve(&["ART", "NUM"]) {
            return Ok(Clause::Cmp { subject, op: "==".into(), value: value_q(c)? });
        }
        return Ok(Clause::Adj { subject, adj: c.text("ADJ")? });
    }
    if c.ve(&["ISNOT"]) {
        c.take("ISNOT")?;
        if !c.ve(&["ART", "NUM"]) {
            return Err(ErrorF { pos: c.i, expected: vec!["ART", "NUM"], text: None });
        }
        return Ok(Clause::Cmp { subject, op: "!=".into(), value: value_q(c)? });
    }
    if c.ve(&["AUS"]) {
        return Ok(Clause::Abs { subject, absence: c.text("AUS")? });
    }
    let mut mode = None;
    let verb = if c.ve(&["VNEG"]) {
        // 'is not in': the negated verb already carries the negation
        mode = Some("not".to_string());
        c.text("VNEG")?
    } else {
        if c.ve(&["NEG"]) {
            c.take("NEG")?;
            mode = Some("not".to_string());
        } else if c.ve(&["ALW"]) {
            c.take("ALW")?;
            mode = Some("always".to_string());
        } else if c.ve(&["EXC"]) {
            c.take("EXC")?;
            mode = Some("exception".to_string());
        }
        c.text("VERB")?
    };
    let (mut obj, mut literals) = (Vec::new(), None);
    if c.ve(&["NAME"]) {
        literals = Some(list(c, name)?);
    } else if c.ve(&["ART"]) {
        obj = list(c, np)?;
    }
    let mut roles: Vec<(String, Np)> = Vec::new();
    while c.ve(&["PREP"]) {
        let prep = c.text("PREP")?;
        let target = np(c)?;
        match roles.iter_mut().find(|(p, _)| *p == prep) {
            Some(x) => x.1 = target,
            None => roles.push((prep, target)),
        }
    }
    Ok(Clause::Rel { subject, verb, mode, obj, literals, roles })
}

/// The sentence, or the failure with its position.
pub fn sentence_f(toks: &[Tok]) -> Result<Sentence, ErrorF> {
    let mut c = Cursor { t: toks, i: 0 };
    let res = if c.ve(&["EVERY"]) {
        c.take("EVERY")?;
        let kind = c.text("NOUN")?;
        if c.text("PREP")? != "in" {
            return Err(ErrorF::msg(c.i - 1, "expected 'in' after 'every <type>'"));
        }
        let scope = np(&mut c)?;
        let o = clause_of(&mut c, (kind.clone(), String::new()))?;
        c.take("END")?;
        Sentence::All { kind, scope, o }
    } else if c.ve(&["COND"]) {
        let cond = c.text("COND")?;
        if c.ve(&["EVERY"]) {
            return Err(ErrorF::msg(c.i, "the quantifier 'every' only applies in a simple sentence, without a condition"));
        }
        let when = clause(&mut c)?;
        c.take("COMMA")?;
        let then = clause(&mut c)?;
        c.take("END")?;
        Sentence::Cond { cond, when, then }
    } else {
        let o = clause(&mut c)?;
        c.take("END")?;
        Sentence::Simple(o)
    };
    if c.i != toks.len() {
        return Err(ErrorF::msg(c.i, "leftover text after the period"));
    }
    Ok(res)
}

#[cfg(test)]
pub fn sentence(toks: &[Tok]) -> Result<Sentence, String> {
    sentence_f(toks).map_err(|e| e.short_message(toks))
}
