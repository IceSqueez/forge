pub mod webhook {
    pub const WEBHOOK_NAME: &str = "webhook_name";
}

pub mod posted {
    pub const MESSAGE_ID: &str = "message_id";
    pub const EMBED_COUNT: &str = "embed_count";
}

pub mod failed {
    pub const REASON: &str = "reason";
    pub const DETAIL: &str = "detail";
    pub const STATUS_CODE: &str = "status_code";
}

pub mod rate_limited {
    pub const RETRY_AFTER_SECS: &str = "retry_after_secs";
}
