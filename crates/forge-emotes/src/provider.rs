#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEmote {
    pub code: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmoteSource {
    SevenTvChannel,
    BetterTtvChannel,
    FrankerFaceZChannel,
    SevenTvGlobal,
    BetterTtvGlobal,
    FrankerFaceZGlobal,
}

pub const PRECEDENCE: [EmoteSource; 6] = [
    EmoteSource::SevenTvChannel,
    EmoteSource::BetterTtvChannel,
    EmoteSource::FrankerFaceZChannel,
    EmoteSource::SevenTvGlobal,
    EmoteSource::BetterTtvGlobal,
    EmoteSource::FrankerFaceZGlobal,
];

const CODE_LENGTH_CAP: usize = 100;
const ID_LENGTH_CAP: usize = 64;
const PROTOCOL_RELATIVE: &str = "//";
const SECURE_SCHEME: &str = "https:";

pub(crate) fn emote(code: &str, url: String) -> Option<ProviderEmote> {
    let code = code.trim();
    let usable =
        !code.is_empty() && code.len() <= CODE_LENGTH_CAP && !code.chars().any(char::is_whitespace);
    usable.then(|| ProviderEmote {
        code: code.to_owned(),
        url,
    })
}

pub(crate) fn is_path_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= ID_LENGTH_CAP
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub(crate) fn absolute_https(raw: &str) -> String {
    match raw.strip_prefix(PROTOCOL_RELATIVE) {
        Some(rest) => format!("{SECURE_SCHEME}{PROTOCOL_RELATIVE}{rest}"),
        None => raw.to_owned(),
    }
}
