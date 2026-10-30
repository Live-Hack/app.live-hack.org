//! Markdown for challenge and event descriptions.
//! comrak in safe mode: raw HTML is dropped and javascript:/data: links are neutralised,
//! so author content cannot inject scripts.
use comrak::{Options, markdown_to_html};
use once_cell::sync::Lazy;

static OPTIONS: Lazy<Options> = Lazy::new(|| {
    let mut o = Options::default();
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.autolink = true;
    o.extension.tasklist = true;
    o.extension.header_ids = Some(String::new());
    o.render.unsafe_ = false;
    o
});

pub fn render(src: &str) -> String {
    markdown_to_html(src, &OPTIONS)
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn markdown_is_safe() {
        let html =
            render("# Title\n\n<script>alert(1)</script>\n\n[x](javascript:alert(1)) `code`");
        assert!(html.contains("<h1>"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("javascript:"));
        assert!(html.contains("<code>code</code>"));
    }
}
