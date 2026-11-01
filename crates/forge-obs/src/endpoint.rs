use crate::error::ObsError;

pub(crate) const DEFAULT_OBS_PORT: u16 = 4455;

pub fn parse_endpoint(endpoint: &str) -> Result<(String, u16), ObsError> {
    let invalid = || ObsError::Connect(format!("invalid endpoint '{endpoint}'"));
    let trimmed = endpoint.trim();
    if trimmed.starts_with("wss://") {
        return Err(ObsError::Connect(
            "TLS endpoints (wss://) are not supported".to_owned(),
        ));
    }
    let without_scheme = trimmed.strip_prefix("ws://").unwrap_or(trimmed);
    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();

    let (host, port_text) = if authority.starts_with('[') {
        let close = authority.find(']').ok_or_else(invalid)?;
        let (host, rest) = authority.split_at(close + 1);
        let port_text = match rest {
            "" => None,
            rest => Some(rest.strip_prefix(':').ok_or_else(invalid)?),
        };
        (host, port_text)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port_text)) => (host, Some(port_text)),
            None => (authority, None),
        }
    };

    if host.is_empty() || (!host.starts_with('[') && host.contains(':')) {
        return Err(invalid());
    }
    let port = match port_text {
        Some(text) => text
            .parse::<u16>()
            .map_err(|_| ObsError::Connect(format!("invalid port in endpoint '{endpoint}'")))?,
        None => DEFAULT_OBS_PORT,
    };
    Ok((host.to_owned(), port))
}
