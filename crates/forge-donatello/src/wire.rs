use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum NumberOrText {
    Number(serde_json::Number),
    Text(String),
}

impl NumberOrText {
    pub(crate) fn as_text(&self) -> String {
        match self {
            Self::Number(number) => number.to_string(),
            Self::Text(text) => text.clone(),
        }
    }

    fn as_count(&self) -> Option<u64> {
        match self {
            Self::Number(number) => number.as_u64(),
            Self::Text(text) => text.trim().parse().ok(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MeWire {
    #[serde(default)]
    pub(crate) nickname: Option<String>,
    #[serde(default)]
    pub(crate) page: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DonatesPageWire {
    #[serde(default)]
    pub(crate) content: Vec<serde_json::Value>,
    #[serde(default)]
    pages: Option<NumberOrText>,
    #[serde(default)]
    total: Option<NumberOrText>,
    #[serde(default)]
    pub(crate) last: Option<bool>,
}

impl DonatesPageWire {
    pub(crate) fn page_count(&self) -> Option<u64> {
        self.pages.as_ref().and_then(NumberOrText::as_count)
    }

    pub(crate) fn total_count(&self) -> Option<u64> {
        self.total.as_ref().and_then(NumberOrText::as_count)
    }
}

pub(crate) const DONATION_ID_FIELD: &str = "pubId";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DonateWire {
    pub(crate) pub_id: String,
    #[serde(default)]
    pub(crate) client_name: Option<String>,
    #[serde(default)]
    pub(crate) message: Option<String>,
    pub(crate) amount: NumberOrText,
    pub(crate) currency: String,
    #[serde(default)]
    pub(crate) is_published: Option<bool>,
    pub(crate) created_at: String,
}
