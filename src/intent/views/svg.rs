//! SVG primitives of the views: text, box, quadratic arrow and the document with the arrow
//! markers. Everything with one decimal place and in the order requested: same model, same SVG.

pub const TEXT: &str = "#2c2c2a";
pub const MUTED: &str = "#6b6a64";
pub const BORDER: &str = "#b4b2a9";
pub const BOX: &str = "#f8f7f3";
pub const STATE: &str = "#ba7517";
pub const INV: &str = "#534ab7";
pub const GAP: &str = "#a32d2d";
pub const WARNING: &str = "#a32d2d";
/// declared absence: "none, on purpose" (not a risk, a decision)
pub const ABSENT: &str = "#0f6e56";
pub const EXT: &str = "#eef6f2";
pub const EXTB: &str = "#5dcaa5";
pub const A1: &str = "#185fa5";
pub const A2: &str = "#993c1d";
const SOURCE: &str = "font-family:system-ui,sans-serif";

/// Estimated text width by character class (the system's sans-serif font); uppercase text is
/// wider than lowercase.
pub fn width(t: &str, px: f64) -> f64 {
    let scope: f64 = t
        .chars()
        .map(|c| match c {
            'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' | 'ı' => 0.27,
            ' ' | 'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '/' | '-' | '·' => 0.34,
            'm' | 'w' | 'M' | 'W' | '—' => 0.86,
            c if c.is_ascii_uppercase() => 0.68,
            c if c.is_ascii_digit() => 0.57,
            'a'..='z' | 'à'..='ÿ' => 0.56,
            c if (c as u32) < 0x2000 => 0.58,
            // symbols (●, ⚠, →, ⏱ ...) and the rest
            _ => 0.9,
        })
        .sum();
    scope * px
}

/// Width of bold text (weight 600): a little wider.
pub fn width_b(t: &str, px: f64) -> f64 {
    width(t, px) * 1.1
}

/// Simple wrap by width, word by word.
pub fn wrap(txt: &str, w: f64, px: f64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for p in txt.split_whitespace() {
        let attempt = if line.is_empty() { p.to_string() } else { format!("{line} {p}") };
        if width(&format!("{line} {p}"), px) > w && !line.is_empty() {
            lines.push(std::mem::replace(&mut line, p.to_string()));
        } else {
            line = attempt;
        }
    }
    lines.push(line);
    lines
}

fn escapes(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#x27;"),
            c => o.push(c),
        }
    }
    o
}

/// Document under construction. Keeps how far the drawing went (text by estimated width), so
/// the document grows instead of cutting what went past the requested width.
#[derive(Default)]
pub struct Svg {
    parts: Vec<String>,
    xmax: f64,
    ymax: f64,
}

/// Style of a text: size, color, weight and anchor.
#[derive(Clone, Copy)]
pub struct Style {
    pub px: f64,
    pub color: &'static str,
    pub weight: u32,
    pub anchor: &'static str,
    pub italic: bool,
}

impl Style {
    pub const fn new(px: f64, color: &'static str) -> Style {
        Style { px, color, weight: 400, anchor: "start", italic: false }
    }

    pub const fn bold(mut self) -> Style {
        self.weight = 600;
        self
    }

    pub const fn end(mut self) -> Style {
        self.anchor = "end";
        self
    }

    pub const fn middle(mut self) -> Style {
        self.anchor = "middle";
        self
    }

    pub const fn italic(mut self) -> Style {
        self.italic = true;
        self
    }
}

impl Svg {
    fn reach(&mut self, x: f64, y: f64) {
        self.xmax = self.xmax.max(x);
        self.ymax = self.ymax.max(y);
    }

    pub fn t(&mut self, x: f64, y: f64, s: &str, e: Style) {
        let w = width(s, e.px);
        let end = match e.anchor {
            "end" => x,
            "middle" => x + w / 2.0,
            _ => x + w,
        };
        self.reach(end, y + 4.0);
        let it = if e.italic { " font-style=\"italic\"" } else { "" };
        self.parts.push(format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"{}\" fill=\"{}\" font-weight=\"{}\" text-anchor=\"{}\"{it} style=\"{SOURCE}\">{}</text>",
            e.px,
            e.color,
            e.weight,
            e.anchor,
            escapes(s)
        ));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn r(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str, rx: f64, dashed: bool) {
        self.reach(x + w, y + h);
        let d = if dashed { " stroke-dasharray=\"4 3\"" } else { "" };
        self.parts.push(format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"{rx}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"0.8\"{d}/>"
        ));
    }

    /// Quadratic arrow with the control point in the middle, offset by `curve` in x.
    #[allow(clippy::too_many_arguments)]
    pub fn arrow(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: &str, curve: f64, dashed: bool) {
        let (mx, my) = ((x1 + x2) / 2.0 + curve, (y1 + y2) / 2.0);
        let d = if dashed { " stroke-dasharray=\"5 4\"" } else { "" };
        self.parts.push(format!(
            "<path d=\"M{x1:.1} {y1:.1} Q{mx:.1} {my:.1} {x2:.1} {y2:.1}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1\" marker-end=\"url(#m{})\"{d}/>",
            &color[1..]
        ));
    }

    /// Position in the drawing, to tag later what was drawn after it (see `tag`).
    pub fn mark(&self) -> usize {
        self.parts.len()
    }

    /// Adds `data-entity="name"` (and `data-state="state"`) to every rect and text drawn since
    /// `from`, so a page can find the shapes of an entity. Attributes only: nothing is drawn
    /// differently.
    pub fn tag(&mut self, from: usize, entity: &str, state: Option<&str>) {
        let attrs = match state {
            Some(st) => format!(" data-entity=\"{}\" data-state=\"{}\"", escapes(entity), escapes(st)),
            None => format!(" data-entity=\"{}\"", escapes(entity)),
        };
        for part in &mut self.parts[from..] {
            for tag in ["<rect", "<text"] {
                if part.starts_with(tag) {
                    part.insert_str(tag.len(), &attrs);
                }
            }
        }
    }

    /// Path already written (curves that are not the standard arrow).
    pub fn raw(&mut self, p: String) {
        self.parts.push(p);
    }

    /// The document, with at least `w` x `h` and large enough for what was drawn.
    pub fn svg(self, w: f64, h: f64) -> String {
        let (w, h) = (w.max(self.xmax + 16.0), h.max(self.ymax + 16.0));
        let mut cores = vec![MUTED, A1, A2, EXTB];
        cores.sort();
        let marks: String = cores
            .iter()
            .map(|c| {
                format!(
                    "<marker id=\"m{}\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"7\" markerHeight=\"7\" orient=\"auto\"><path d=\"M1 1L9 5L1 9\" fill=\"none\" stroke=\"{c}\" stroke-width=\"1.5\"/></marker>",
                    &c[1..]
                )
            })
            .collect();
        let (w, h) = (w.ceil() as i64, h.ceil() as i64);
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"><defs>{marks}</defs><rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>{}</svg>\n",
            self.parts.concat()
        )
    }
}
