pub(crate) const CLIP_PATH_PREFIX: &str = "/audio/v1/clip/";
pub(crate) const REPORT_PATH_PREFIX: &str = "/audio/v1/report/";

pub(crate) fn clip_path(capability: &str) -> String {
    format!("{CLIP_PATH_PREFIX}{capability}")
}

pub(crate) fn report_path(capability: &str) -> String {
    format!("{REPORT_PATH_PREFIX}{capability}")
}
