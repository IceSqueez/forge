use forge_components::Platform;
use forge_storage::ViewerPlatform;
use gpui::SharedString;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AuthorHandle {
    ViewerId(SharedString),
    Name(SharedString),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AuthorKey {
    pub platform: Platform,
    pub handle: AuthorHandle,
}

impl AuthorKey {
    pub fn resolve(
        platform: Platform,
        viewer_id: Option<&SharedString>,
        name: &SharedString,
    ) -> Option<Self> {
        if name.is_empty() {
            return None;
        }
        let handle = match viewer_id.filter(|id| !id.is_empty()) {
            Some(id) => AuthorHandle::ViewerId(id.clone()),
            None => AuthorHandle::Name(name.clone()),
        };
        Some(Self { platform, handle })
    }

    pub fn by_viewer_id(platform: Platform, viewer_id: &str) -> Self {
        Self {
            platform,
            handle: AuthorHandle::ViewerId(SharedString::from(viewer_id.to_owned())),
        }
    }

    pub fn by_name(platform: Platform, name: &str) -> Self {
        Self {
            platform,
            handle: AuthorHandle::Name(SharedString::from(name.to_owned())),
        }
    }

    pub fn fallback_name(&self) -> Option<&SharedString> {
        match &self.handle {
            AuthorHandle::Name(name) => Some(name),
            AuthorHandle::ViewerId(_) => None,
        }
    }

    pub fn element_id(&self, prefix: &str) -> SharedString {
        let (kind, value) = match &self.handle {
            AuthorHandle::ViewerId(id) => ("id", id),
            AuthorHandle::Name(name) => ("name", name),
        };
        SharedString::from(format!("{prefix}-{:?}-{kind}-{value}", self.platform))
    }
}

pub fn viewer_platform(platform: &ViewerPlatform) -> Platform {
    match platform {
        ViewerPlatform::Twitch => Platform::Twitch,
        ViewerPlatform::YouTube => Platform::YouTube,
        ViewerPlatform::Kick => Platform::Kick,
    }
}
