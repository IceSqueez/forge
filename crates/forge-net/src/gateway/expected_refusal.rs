use super::failure::GatewayFailure;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedRefusal {
    status: u16,
    excerpt_marker: Option<String>,
}

impl ExpectedRefusal {
    pub fn status(status: u16) -> Self {
        Self {
            status,
            excerpt_marker: None,
        }
    }

    pub fn status_with_excerpt(status: u16, marker: impl Into<String>) -> Self {
        Self {
            status,
            excerpt_marker: Some(marker.into().to_lowercase()),
        }
    }

    pub fn matches(&self, failure: &GatewayFailure) -> bool {
        if failure.status() != Some(self.status) {
            return false;
        }
        match &self.excerpt_marker {
            None => true,
            Some(marker) => failure
                .excerpt()
                .is_some_and(|excerpt| excerpt.as_str().to_lowercase().contains(marker.as_str())),
        }
    }
}
