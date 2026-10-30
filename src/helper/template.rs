//! Templates from `html/`, parsed once at startup.
//! `{{name}}` is HTML-escaped, `{{{name}}}` is inserted raw (trusted HTML only),
//! `{{inject_x}}` includes `html/inject/x.html` at load time.
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

enum Seg {
    Lit(String),
    Esc(String),
    Raw(String),
}

pub struct Template(Vec<Seg>);

const ROOT: &str = "html";

static TEMPLATES: Lazy<HashMap<String, Template>> = Lazy::new(|| {
    let mut files = Vec::new();
    collect(Path::new(ROOT), &mut files);
    let raw: HashMap<String, String> = files
        .into_iter()
        .filter_map(|p| {
            let name = p
                .strip_prefix(ROOT)
                .ok()?
                .to_string_lossy()
                .trim_start_matches('/')
                .to_string();
            Some((name, fs::read_to_string(&p).ok()?))
        })
        .collect();
    raw.iter()
        .map(|(name, src)| (name.clone(), parse(&expand(src, &raw, 0))))
        .collect()
});

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "html") {
            out.push(p);
        }
    }
}

/// Resolves `{{inject_x}}` recursively (depth-limited).
fn expand(src: &str, raw: &HashMap<String, String>, depth: u8) -> String {
    if depth > 4 || !src.contains("{{inject_") {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(i) = rest.find("{{inject_") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 9..];
        let Some(end) = after.find("}}") else { break };
        let name = format!("inject/{}.html", &after[..end]);
        if let Some(inc) = raw.get(&name) {
            out.push_str(&expand(inc, raw, depth + 1));
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn parse(src: &str) -> Template {
    let mut segs = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find("{{") {
        if i > 0 {
            segs.push(Seg::Lit(rest[..i].to_string()));
        }
        let (raw, open, close) = if rest[i..].starts_with("{{{") {
            (true, 3, "}}}")
        } else {
            (false, 2, "}}")
        };
        let after = &rest[i + open..];
        match after.find(close) {
            Some(end) => {
                let name = after[..end].trim().to_string();
                segs.push(if raw { Seg::Raw(name) } else { Seg::Esc(name) });
                rest = &after[end + close.len()..];
            }
            None => {
                segs.push(Seg::Lit(rest[i..].to_string()));
                rest = "";
            }
        }
    }
    if !rest.is_empty() {
        segs.push(Seg::Lit(rest.to_string()));
    }
    Template(segs)
}

pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    escape_into(&mut out, text);
    out
}

fn escape_into(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
}

/// Template variables. Small linear list: templates use a handful of keys.
#[derive(Default)]
pub struct Vars(Vec<(&'static str, String)>);

impl Vars {
    pub fn new() -> Self {
        Self(Vec::with_capacity(16))
    }
    pub fn set(&mut self, key: &'static str, value: impl Into<String>) -> &mut Self {
        self.0.push((key, value.into()));
        self
    }
    pub fn extend(&mut self, other: Vars) {
        self.0.extend(other.0);
    }
    fn get(&self, key: &str) -> Option<&str> {
        // last write wins so callers can override defaults
        self.0
            .iter()
            .rev()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
    }
}

impl Template {
    pub fn render(&self, vars: &Vars) -> String {
        let mut out = String::with_capacity(8 * 1024);
        for seg in &self.0 {
            match seg {
                Seg::Lit(s) => out.push_str(s),
                Seg::Esc(k) => escape_into(&mut out, vars.get(k).unwrap_or("")),
                Seg::Raw(k) => out.push_str(vars.get(k).unwrap_or("")),
            }
        }
        out
    }
}

/// Renders `html/<name>`; panics on unknown template names (a programming error).
pub fn render(name: &str, vars: &Vars) -> String {
    TEMPLATES
        .get(name)
        .unwrap_or_else(|| panic!("unknown template {name}"))
        .render(vars)
}

pub fn exists(name: &str) -> bool {
    TEMPLATES.contains_key(name)
}

/// Forces loading at startup so a broken template fails fast.
pub fn preload() -> usize {
    TEMPLATES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_raw_and_inject() {
        let mut raw = HashMap::new();
        raw.insert("inject/x.html".to_string(), "<i>{{who}}</i>".to_string());
        let t = parse(&expand("A{{inject_x}}B{{{html}}}C{{missing}}", &raw, 0));
        let mut v = Vars::new();
        v.set("who", "<script>").set("html", "<b>ok</b>");
        assert_eq!(t.render(&v), "A<i>&lt;script&gt;</i>B<b>ok</b>C");
    }
}
