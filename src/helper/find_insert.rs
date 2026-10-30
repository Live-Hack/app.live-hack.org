use regex::Regex;

pub fn find_insert(body: &str) -> Vec<String> {
    let rule = Regex::new(r"\{\{inject_([a-z0-9_/]+)\}\}").unwrap();
    rule.captures_iter(body)
        .map(|cap| cap[1].to_string())
        .collect()
}
