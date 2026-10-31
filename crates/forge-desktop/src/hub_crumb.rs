use std::mem::discriminant;

use forge_components::{BreadcrumbCrumb, tr};
use forge_platform_core::IntegrationCategory;
use gpui::{ClickEvent, Context, ElementId, EventEmitter};

use crate::integration_catalog::core_features;
use crate::navigation::NavRequested;
use crate::navigation::Screen;

const WHOLE_HUB_ID: &str = "hub-crumb";

pub(crate) fn core_category(screen: &Screen) -> Option<IntegrationCategory> {
    core_features()
        .into_iter()
        .find(|feature| discriminant(&feature.screen) == discriminant(screen))
        .map(|feature| feature.category)
}

pub(crate) fn hub_crumb_id(category: Option<IntegrationCategory>) -> String {
    match category {
        Some(category) => format!("{WHOLE_HUB_ID}-{}", category.key()),
        None => WHOLE_HUB_ID.to_owned(),
    }
}

pub(crate) fn hub_crumb<V: EventEmitter<NavRequested>>(
    category: Option<IntegrationCategory>,
    cx: &mut Context<V>,
) -> BreadcrumbCrumb {
    let label = match category {
        Some(category) => tr!(category.label_key()),
        None => tr!("integrations_breadcrumb"),
    };
    BreadcrumbCrumb::link(
        label,
        ElementId::Name(hub_crumb_id(category).into()),
        cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.emit(NavRequested(Screen::Integrations(category)));
        }),
    )
}

#[cfg(test)]
mod tests {
    use forge_components::breadcrumb;
    use gpui::{IntoElement, Render, TestAppContext, Window, div, prelude::*};

    use super::*;
    use crate::navigation::TtsSection;
    use crate::test_support::hub_crumb_targets;
    use forge_components::ActivePresentation;

    #[test]
    fn core_screens_map_to_their_hub_category_regardless_of_payload() {
        for (screen, expected) in [
            (Screen::Soundboard, Some(IntegrationCategory::AUDIO)),
            (Screen::Server, Some(IntegrationCategory::TOOLS)),
            (
                Screen::Tts(Some(TtsSection::Filters)),
                Some(IntegrationCategory::AUDIO),
            ),
            (Screen::Home, None),
            (Screen::Integrations(None), None),
        ] {
            assert_eq!(core_category(&screen), expected, "{screen:?}");
        }
    }

    struct CrumbHost {
        category: Option<IntegrationCategory>,
    }

    impl EventEmitter<NavRequested> for CrumbHost {}

    impl Render for CrumbHost {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let palette = cx.palette();
            div()
                .size_full()
                .child(breadcrumb(vec![hub_crumb(self.category, cx)], &palette))
        }
    }

    #[gpui::test]
    fn clicking_a_category_crumb_opens_the_hub_on_that_category(cx: &mut TestAppContext) {
        let category = Some(IntegrationCategory::AUDIO);
        let screens = hub_crumb_targets(cx, category, |_, _| CrumbHost { category });

        assert_eq!(screens, vec![Screen::Integrations(category)]);
    }

    #[gpui::test]
    fn clicking_the_uncategorised_crumb_opens_the_whole_hub(cx: &mut TestAppContext) {
        let screens = hub_crumb_targets(cx, None, |_, _| CrumbHost { category: None });

        assert_eq!(screens, vec![Screen::Integrations(None)]);
    }
}
