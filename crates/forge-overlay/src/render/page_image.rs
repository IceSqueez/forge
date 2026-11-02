const SECURE_SCHEME: &str = "https://";
const URL_LENGTH_CAP: usize = 2048;

fn is_unsafe_url_char(c: char) -> bool {
    c.is_whitespace() || c.is_control() || matches!(c, '"' | '\'' | '<' | '>' | '\\' | '`')
}

fn on_domain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|subdomain| subdomain.ends_with('.'))
}

pub(crate) fn vetted_image_url(raw: &str, hosts: &[&str]) -> Option<String> {
    let raw = raw.trim();
    if raw.len() > URL_LENGTH_CAP || raw.chars().any(is_unsafe_url_char) {
        return None;
    }
    let scheme = raw.get(..SECURE_SCHEME.len())?;
    if !scheme.eq_ignore_ascii_case(SECURE_SCHEME) {
        return None;
    }
    let authority = raw[SECURE_SCHEME.len()..]
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if authority.is_empty() || authority.contains(['@', ':']) {
        return None;
    }
    let host = authority.to_ascii_lowercase();
    hosts
        .iter()
        .any(|domain| on_domain(&host, domain))
        .then(|| raw.to_owned())
}
