/// Replaces `{{key}}` with a trusted HTML fragment (templates, rendered components).
pub fn replace_html(body: &str, key: &str, html: &str) -> String {
    body.replace(&format!("{{{{{}}}}}", key), html)
}

/// Replaces `{{key}}` with user-controlled text, HTML-escaped.
/// Any value that comes from a user (name, description, flag...) must go through this one.
pub fn replace_text(body: &str, key: &str, text: &str) -> String {
    replace_html(body, key, &escape_html(text))
}

pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_escaped_html_is_not() {
        let tpl = "<p>{{name}}</p>{{block}}";
        let out = replace_text(tpl, "name", "<script>alert('x')</script>");
        assert_eq!(
            out,
            "<p>&lt;script&gt;alert(&#x27;x&#x27;)&lt;/script&gt;</p>{{block}}"
        );
        assert!(replace_html(&out, "block", "<b>ok</b>").ends_with("<b>ok</b>"));
    }
}
