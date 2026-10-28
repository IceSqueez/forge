use forge_net::{Gateway, GatewayRequest};
use forge_platform_core::PlatformError;
use serde::Deserialize;

use crate::gateway::{RefusalReasons, authorized, malformed, platform_error};

const CATEGORIES_PATH: &str = "/categories";
const MAX_MATCHES: usize = 10;

const CATEGORIES_REFUSALS: RefusalReasons = RefusalReasons {
    unauthorized: "categories token rejected (401)",
    forbidden: None,
};

pub struct KickCategories {
    api: Gateway,
}

pub struct CategoryMatch {
    pub id: u64,
    pub name: String,
}

#[derive(Deserialize)]
struct CategoriesEnvelope {
    #[serde(default)]
    data: Vec<CategoryData>,
}

#[derive(Deserialize, Default)]
struct CategoryData {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    name: String,
}

impl KickCategories {
    pub fn new(api: Gateway) -> Self {
        Self { api }
    }

    pub async fn search(
        &self,
        token: &str,
        query: &str,
    ) -> Result<Vec<CategoryMatch>, PlatformError> {
        let request = authorized(GatewayRequest::get(CATEGORIES_PATH), token, CATEGORIES_PATH)
            .query("q", query);
        let response = self
            .api
            .send(request)
            .await
            .map_err(|failure| platform_error(failure, CATEGORIES_REFUSALS))?;
        let envelope: CategoriesEnvelope = response.json().map_err(malformed)?;

        Ok(envelope
            .data
            .into_iter()
            .take(MAX_MATCHES)
            .map(|c| CategoryMatch {
                id: c.id,
                name: c.name,
            })
            .collect())
    }
}
