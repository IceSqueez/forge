use forge_components::{
    Density, ForgePalette, Spacing, empty_state, error_row_with_retry, failure_with_retry, spacing,
    tr,
};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, IntoElement, ParentElement, SharedString, Styled,
    Window, div,
};

use crate::async_bridge::Loadable;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoticeWidth {
    Pane,
    NarrowColumn,
}

pub(crate) struct LoadNotice {
    pub(crate) id: &'static str,
    pub(crate) loading: String,
    pub(crate) width: NoticeWidth,
}

const SPINNER_SLOT: usize = 0;

pub(crate) fn load_notice<T>(
    load: &Loadable<T>,
    notice: LoadNotice,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    palette: &ForgePalette,
    density: Density,
) -> Option<AnyElement> {
    let LoadNotice { id, loading, width } = notice;
    match load {
        Loadable::Loaded(_) => None,
        Loadable::Loading => Some(
            empty_state(SharedString::from(loading), palette)
                .loading(ElementId::from((id, SPINNER_SLOT)))
                .density(density)
                .into_any_element(),
        ),
        Loadable::Failed(failure) => {
            let reason = failure.text();
            let message = tr!("load_notice_failed", reason = reason.as_str());
            Some(match width {
                NoticeWidth::Pane => div()
                    .w_full()
                    .p(spacing(Spacing::Md, density))
                    .child(error_row_with_retry(
                        id, message, palette, on_retry, density,
                    ))
                    .into_any_element(),
                NoticeWidth::NarrowColumn => div()
                    .w_full()
                    .p(spacing(Spacing::Xs, density))
                    .child(failure_with_retry(id, &message, palette, on_retry, density))
                    .into_any_element(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{Density, FORGE_DEFAULT};
    use gpui::{Context, Entity, Modifiers, Render, TestAppContext, VisualTestContext, Window};

    use super::*;
    use crate::repo_failure::RepoFailure;

    struct Host {
        load: Loadable<()>,
        retries: usize,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
            let notice = load_notice(
                &self.load,
                LoadNotice {
                    id: "host-load",
                    loading: "Reading".to_owned(),
                    width: NoticeWidth::Pane,
                },
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.retries += 1;
                    cx.notify();
                }),
                &FORGE_DEFAULT,
                Density::Cozy,
            );
            div().size_full().children(notice)
        }
    }

    fn mount(
        cx: &mut TestAppContext,
        load: Loadable<()>,
    ) -> (Entity<Host>, &mut VisualTestContext) {
        crate::test_support::install_presentation(cx);
        let (host, vcx) = cx.add_window_view(|_, _| Host { load, retries: 0 });
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();
        (host, vcx)
    }

    #[gpui::test]
    fn a_failed_load_offers_a_retry_that_real_clicks_reach(cx: &mut TestAppContext) {
        let (host, vcx) = mount(
            cx,
            Loadable::Failed(RepoFailure::localized("table is gone")),
        );
        let retry = vcx
            .debug_bounds("host-load")
            .expect("the retry button is painted");

        vcx.simulate_click(retry.center(), Modifiers::none());
        vcx.run_until_parked();

        assert_eq!(host.read_with(vcx, |host, _| host.retries), 1);
    }

    #[gpui::test]
    fn nothing_is_shown_once_loaded(cx: &mut TestAppContext) {
        let (_, vcx) = mount(cx, Loadable::Loaded(()));

        assert!(vcx.debug_bounds("host-load").is_none());
    }
}
