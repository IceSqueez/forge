pub const PREVIEW_PARAM: &str = "preview";
pub const PREVIEW_VALUE: &str = "1";

const QUERY_START: char = '?';
const QUERY_JOIN: char = '&';
const FRAGMENT_START: char = '#';

pub fn preview_page_url(page_url: &str) -> String {
    let (base, fragment) = match page_url.find(FRAGMENT_START) {
        Some(at) => page_url.split_at(at),
        None => (page_url, ""),
    };
    let flag = format!("{PREVIEW_PARAM}={PREVIEW_VALUE}");
    let Some((_, query)) = base.split_once(QUERY_START) else {
        return format!("{base}{QUERY_START}{flag}{fragment}");
    };
    if query.split(QUERY_JOIN).any(|pair| pair == flag) {
        return page_url.to_owned();
    }
    format!("{base}{QUERY_JOIN}{flag}{fragment}")
}
