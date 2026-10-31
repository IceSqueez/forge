use serde_json::Value;

use super::outcome::{FailureCause, Verdict};
use crate::fakes::kick::KickRequest;
use crate::fakes::youtube::YouTubeRequest;
use crate::harness::scenario::{HttpRequestSeen, RequestCount};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LedgerMark {
    pub(crate) sessions: usize,
    pub(crate) requests: usize,
}

pub(crate) trait RecordedHttp {
    fn method(&self) -> &str;
    fn path(&self) -> &str;
    fn status(&self) -> u16;
    fn body(&self) -> Option<&Value>;
}

impl RecordedHttp for KickRequest {
    fn method(&self) -> &str {
        &self.method
    }

    fn path(&self) -> &str {
        &self.path
    }

    fn status(&self) -> u16 {
        self.status
    }

    fn body(&self) -> Option<&Value> {
        self.body.as_ref()
    }
}

impl RecordedHttp for YouTubeRequest {
    fn method(&self) -> &str {
        &self.method
    }

    fn path(&self) -> &str {
        &self.path
    }

    fn status(&self) -> u16 {
        self.status
    }

    fn body(&self) -> Option<&Value> {
        self.body.as_ref()
    }
}

pub(crate) fn request_matches(request: &impl RecordedHttp, expected: &HttpRequestSeen) -> bool {
    request.path() == expected.path
        && expected
            .method
            .as_deref()
            .is_none_or(|method| request.method() == method)
        && expected
            .status
            .is_none_or(|status| request.status() == status)
        && expected.body.0.iter().all(|(pointer, matcher)| {
            matcher.matches(request.body().and_then(|body| body.pointer(pointer)))
        })
}

pub(crate) fn counted(request: &impl RecordedHttp, expected: &RequestCount) -> bool {
    request.path() == expected.path
        && expected
            .method
            .as_deref()
            .is_none_or(|method| request.method() == method)
}

pub(crate) fn count_verdict(observed: usize, expected: &RequestCount) -> Verdict {
    let below = expected.min.is_some_and(|min| observed < min as usize);
    let above = expected.max.is_some_and(|max| observed > max as usize);
    if below || above {
        Verdict::Failed(FailureCause::RequestCountOutOfRange {
            observed,
            min: expected.min,
            max: expected.max,
        })
    } else {
        Verdict::Passed
    }
}
