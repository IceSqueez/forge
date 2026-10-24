use reqwest::Url;

const HOST_PORT_SEPARATOR: char = ':';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
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
            host_header,
            path: url.path().to_owned(),
        }
    }

    pub fn host_header(&self) -> &str {
        &self.host_header
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}
