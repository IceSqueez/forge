use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::message::arg::MessageArg;
use crate::message::code::MessageCode;
use crate::message::common::UPSTREAM_FAILURE;
use crate::message::detail::UpstreamDetail;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageRef {
    code: MessageCode,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    args: BTreeMap<Cow<'static, str>, MessageArg>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    detail: Option<UpstreamDetail>,
}

impl MessageRef {
    pub fn new(code: MessageCode) -> Self {
        Self {
            code,
            args: BTreeMap::new(),
            detail: None,
        }
    }

    pub fn upstream_failure(detail: UpstreamDetail) -> Self {
        Self::new(UPSTREAM_FAILURE).with_detail(detail)
    }

    pub fn with_arg(mut self, name: &'static str, value: impl Into<MessageArg>) -> Self {
        self.args.insert(Cow::Borrowed(name), value.into());
        self
    }

    pub fn with_detail(mut self, detail: UpstreamDetail) -> Self {
        self.detail = Some(detail);
        self
    }

    pub fn code(&self) -> &MessageCode {
        &self.code
    }

    pub fn arg(&self, name: &str) -> Option<&MessageArg> {
        self.args.get(name)
    }

    pub fn args(&self) -> impl Iterator<Item = (&str, &MessageArg)> {
        self.args.iter().map(|(name, value)| (name.as_ref(), value))
    }

    pub fn detail(&self) -> Option<&UpstreamDetail> {
        self.detail.as_ref()
    }
}
