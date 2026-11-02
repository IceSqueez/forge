pub trait ChatEmoteBook: Send + Sync {
    fn emote_image(&self, code: &str) -> Option<String>;
}
