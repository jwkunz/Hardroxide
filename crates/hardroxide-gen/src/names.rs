use std::collections::BTreeSet;

const KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

/// snake_case a Vivado name into a valid Rust identifier.
pub fn snake(s: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if !c.is_ascii_alphanumeric() {
            if !out.ends_with('_') {
                out.push('_');
            }
            prev = Some('_');
            continue;
        }
        if c.is_ascii_uppercase() && prev.is_some_and(|p| p.is_ascii_lowercase()) {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
        prev = Some(c);
    }
    let mut out = out.trim_matches('_').to_string();
    if out.is_empty() {
        out.push('x');
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    if KEYWORDS.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// Assign unique identifiers in input order; collisions get `_2`, `_3`, ...
pub fn unique_idents<'a>(raw: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    raw.into_iter()
        .map(|r| {
            let base = snake(r);
            let mut cand = base.clone();
            let mut n = 2;
            while !seen.insert(cand.clone()) {
                cand = format!("{base}_{n}");
                n += 1;
            }
            cand
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_cases() {
        assert_eq!(snake("axi_gpio_0"), "axi_gpio_0");
        assert_eq!(snake("myIp"), "my_ip");
        assert_eq!(snake("GPIO2_DATA"), "gpio2_data");
        assert_eq!(snake("0abc"), "_0abc");
        assert_eq!(snake("type"), "type_");
        assert_eq!(snake("a-b.c"), "a_b_c");
        assert_eq!(snake("$$"), "x");
    }

    #[test]
    fn collisions_are_resolved_in_order() {
        assert_eq!(unique_idents(["a-b", "a_b", "A_B"]), ["a_b", "a_b_2", "a_b_3"]);
    }
}
