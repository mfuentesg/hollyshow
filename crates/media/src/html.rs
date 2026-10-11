//! Tiny HTML helpers for Commons `extmetadata` values (`Artist` is an HTML snippet).

/// Removes tags, decodes the common entities and collapses whitespace.
pub fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    let decoded = decode_entities(&out);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match tail.find(';').filter(|&j| j <= 10) {
            Some(j) => {
                let ent = &tail[1..j];
                let rep = match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" | "#39" => Some('\''),
                    "nbsp" => Some(' '),
                    e if e.starts_with("#x") || e.starts_with("#X") => {
                        u32::from_str_radix(&e[2..], 16)
                            .ok()
                            .and_then(char::from_u32)
                    }
                    e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
                    _ => None,
                };
                match rep {
                    Some(ch) => {
                        out.push(ch);
                        rest = &tail[j + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = &tail[1..];
                    }
                }
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// First `href="..."` value in the snippet, made absolute for protocol-relative and
/// root-relative Commons links. Only http(s) links are returned.
pub fn first_href(html: &str) -> Option<String> {
    let i = html.find("href=")?;
    let rest = &html[i + 5..];
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let end = rest[1..].find(quote)?;
    let raw = decode_entities(&rest[1..1 + end]);
    let url = if let Some(r) = raw.strip_prefix("//") {
        format!("https://{r}")
    } else if raw.starts_with('/') {
        format!("https://commons.wikimedia.org{raw}")
    } else {
        raw
    };
    (url.starts_with("https://") || url.starts_with("http://")).then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_entities() {
        assert_eq!(
            strip_tags("<a href=\"//x\">Jos&eacute; &amp; Ana</a> <b>Lopez</b>"),
            "Jos&eacute; & Ana Lopez"
        );
        assert_eq!(strip_tags("  plain   text "), "plain text");
        assert_eq!(
            strip_tags("Mu&#241;oz &#x41; &unknown; a&b"),
            "Muñoz A &unknown; a&b"
        );
    }

    #[test]
    fn extracts_links() {
        assert_eq!(
            first_href("<a href=\"//commons.wikimedia.org/wiki/User:X\">X</a>").unwrap(),
            "https://commons.wikimedia.org/wiki/User:X"
        );
        assert_eq!(
            first_href("<a href='/wiki/User:Y'>Y</a>").unwrap(),
            "https://commons.wikimedia.org/wiki/User:Y"
        );
        assert_eq!(first_href("<a href=\"javascript:alert(1)\">x</a>"), None);
        assert_eq!(first_href("no link"), None);
    }
}
