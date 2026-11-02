use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use forge_platform_core::ChatEmoteBook;
use forge_types::{PlatformId, RecoverRwLock};

#[derive(Clone, Default)]
pub struct ChatEmotes {
    books: Arc<RwLock<HashMap<PlatformId, Arc<dyn ChatEmoteBook>>>>,
}

impl ChatEmotes {
    pub fn register(&self, platform: PlatformId, book: Arc<dyn ChatEmoteBook>) {
        self.books.write_recover().insert(platform, book);
    }

    pub fn image(&self, platform: PlatformId, code: &str) -> Option<String> {
        let book = self.books.read_recover().get(&platform).cloned()?;
        book.emote_image(code)
    }
}
