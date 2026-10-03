use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientInfoWire {
    #[serde(default)]
    pub(crate) jars: Vec<JarWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JarWire {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) send_id: Option<String>,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) currency_code: Option<i64>,
    #[serde(default)]
    pub(crate) goal: Option<i64>,
}

pub(crate) const TRANSACTION_ID_FIELD: &str = "id";
pub(crate) const TRANSACTION_TIME_FIELD: &str = "time";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatementItemWire {
    pub(crate) id: String,
    pub(crate) time: i64,
    pub(crate) amount: i64,
    pub(crate) currency_code: i64,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) comment: Option<String>,
    #[serde(default)]
    pub(crate) counter_name: Option<String>,
}
