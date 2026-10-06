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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::collections::HashSet;

    use forge_components::Platform;
    use gpui::SharedString;

    use super::{AuthorHandle, AuthorKey};

    #[test]
    fn resolve_prefers_a_non_empty_viewer_id_and_needs_a_name() {
        for (viewer_id, name, expected) in [
            (
                Some("u1"),
                "alice",
                Some(AuthorHandle::ViewerId("u1".into())),
            ),
            (None, "alice", Some(AuthorHandle::Name("alice".into()))),
            (Some(""), "alice", Some(AuthorHandle::Name("alice".into()))),
            (Some("u1"), "", None),
            (None, "", None),
        ] {
            let viewer_id = viewer_id.map(SharedString::from);
            let resolved = AuthorKey::resolve(
                Platform::Kick,
                viewer_id.as_ref(),
                &SharedString::from(name),
            );
            assert_eq!(
                resolved,
                expected.map(|handle| AuthorKey {
                    platform: Platform::Kick,
                    handle,
                }),
                "viewer_id={viewer_id:?} name={name:?}"
            );
        }
    }

    #[test]
    fn element_ids_differ_across_platform_and_handle_kind_for_the_same_text() {
        let keys = [
            AuthorKey::by_viewer_id(Platform::Twitch, "alice"),
            AuthorKey::by_name(Platform::Twitch, "alice"),
            AuthorKey::by_viewer_id(Platform::Kick, "alice"),
            AuthorKey::by_name(Platform::Kick, "alice"),
            AuthorKey::by_name(Platform::YouTube, "alice"),
        ];
        let ids: HashSet<SharedString> = keys.iter().map(|key| key.element_id("row")).collect();
        assert_eq!(ids.len(), keys.len());
    }
}
