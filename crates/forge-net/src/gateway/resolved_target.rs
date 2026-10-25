use reqwest::Url;

use super::failure::InvalidRequest;

const HOST_PORT_SEPARATOR: char = ':';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    scheme: String,
    host: String,
    host_header: String,
    path: String,
}

impl ResolvedTarget {
    pub(crate) fn from_url(url: &Url) -> Self {
        let host = url.host_str().unwrap_or_default();
        let host_header = match url.port() {
            Some(port) => format!("{host}{HOST_PORT_SEPARATOR}{port}"),
            None => host.to_owned(),
        };
        Self {
            scheme: url.scheme().to_owned(),
            host: host.to_owned(),
            host_header,
            path: url.path().to_owned(),
        }
    }

    pub fn absolute(raw: &str) -> Result<Self, InvalidRequest> {
        let url = Url::parse(raw).map_err(|_| InvalidRequest::MalformedUrl)?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(InvalidRequest::EmbeddedCredentials);
        }
        Ok(Self::from_url(&url))
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn host_header(&self) -> &str {
        &self.host_header
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}
