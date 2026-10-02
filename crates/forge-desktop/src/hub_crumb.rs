use std::mem::discriminant;

use forge_components::{BreadcrumbCrumb, tr};
use forge_platform_core::IntegrationCategory;
use gpui::{ClickEvent, Context, ElementId, EventEmitter};

use crate::integration_catalog::core_features;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

const WHOLE_HUB_ID: &str = "hub-crumb";

pub fn core_category(screen: &Screen) -> Option<IntegrationCategory> {
    core_features()
        .into_iter()
        .find(|feature| discriminant(&feature.screen) == discriminant(screen))
        .map(|feature| feature.category)
}

pub fn hub_crumb<V: EventEmitter<NavRequested>>(
    category: Option<IntegrationCategory>,
    cx: &mut Context<V>,
) -> BreadcrumbCrumb {
    let (label, id) = match category {
        Some(category) => (
            tr!(category.label_key()),
            ElementId::Name(format!("{WHOLE_HUB_ID}-{}", category.key()).into()),
        ),
        None => (
            tr!("integrations_breadcrumb"),
            ElementId::Name(WHOLE_HUB_ID.into()),
        ),
    };
    BreadcrumbCrumb::link(
        label,
        id,
        cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.emit(NavRequested(Screen::Integrations(category)));
        }),
    )
}

#[cfg(test)]
mod tests {
    use forge_components::{Density, ThemeId, breadcrumb};
    use gpui::{
        AppContext as _, IntoElement, Modifiers, Render, Subscription, TestAppContext,
        VisualTestContext, Window, div, point, prelude::*, px, size,
    };

    use super::*;
    use crate::presentation::{ActivePresentation, Presentation};
    use crate::tts::TtsSection;

    const WINDOW_W: f32 = 600.0;
    const WINDOW_H: f32 = 80.0;
    const SCAN_STEP: f32 = 4.0;

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

    struct Heard {
        screens: Vec<Screen>,
        _sub: Subscription,
    }

    fn clicked_screens(
        category: Option<IntegrationCategory>,
        cx: &mut TestAppContext,
    ) -> Vec<Screen> {
        cx.update(|cx| cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy)));
        let (host, vcx) = cx.add_window_view(|_window, _cx| CrumbHost { category });
        let heard = vcx.update(|_window, cx| {
            cx.new(|cx| Heard {
                screens: Vec::new(),
                _sub: cx.subscribe(&host, |heard: &mut Heard, _, event: &NavRequested, _| {
                    heard.screens.push(event.0.clone());
                }),
            })
        });
        vcx.simulate_resize(size(px(WINDOW_W), px(WINDOW_H)));
        vcx.run_until_parked();
        scan_rows(vcx);
        heard.read_with(vcx, |heard, _| heard.screens.clone())
    }

    fn scan_rows(vcx: &mut VisualTestContext) {
        let mut y = SCAN_STEP / 2.0;
        while y < WINDOW_H {
            let mut x = SCAN_STEP / 2.0;
            while x < WINDOW_W {
                vcx.simulate_click(point(px(x), px(y)), Modifiers::none());
                x += SCAN_STEP;
            }
            y += SCAN_STEP;
        }
    }

    #[gpui::test]
    fn clicking_a_category_crumb_opens_the_hub_on_that_category(cx: &mut TestAppContext) {
        let screens = clicked_screens(Some(IntegrationCategory::AUDIO), cx);

        assert!(!screens.is_empty(), "no click reached the crumb");
        assert!(
            screens
                .iter()
                .all(|screen| *screen == Screen::Integrations(Some(IntegrationCategory::AUDIO))),
            "{screens:?}"
        );
    }

    #[gpui::test]
    fn clicking_the_uncategorised_crumb_opens_the_whole_hub(cx: &mut TestAppContext) {
        let screens = clicked_screens(None, cx);

        assert!(!screens.is_empty(), "no click reached the crumb");
        assert!(
            screens
                .iter()
                .all(|screen| *screen == Screen::Integrations(None)),
            "{screens:?}"
        );
    }
}
