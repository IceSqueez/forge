use gpui::{AppContext, Context, Entity, SharedString};

use crate::palette::ForgePalette;
use crate::text_input::{InputEvent, TextInput, search_input, search_input_on_surface};

pub struct SearchState {
    field: Entity<TextInput>,
    query: String,
}

impl SearchState {
    pub fn new<V: 'static>(
        cx: &mut Context<V>,
        palette: ForgePalette,
        placeholder: impl Into<SharedString>,
    ) -> Self {
        Self::from_field(cx.new(|cx| search_input(placeholder, palette, cx)))
    }

    pub fn on_surface<V: 'static>(
        cx: &mut Context<V>,
        palette: ForgePalette,
        placeholder: impl Into<SharedString>,
    ) -> Self {
        Self::from_field(cx.new(|cx| search_input_on_surface(placeholder, palette, cx)))
    }

    pub fn from_field(field: Entity<TextInput>) -> Self {
        Self {
            field,
            query: String::new(),
        }
    }

    pub fn field(&self) -> &Entity<TextInput> {
        &self.field
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn is_empty(&self) -> bool {
        self.query.is_empty()
    }

    pub fn on_changed(&mut self, event: &InputEvent) -> bool {
        let InputEvent::Changed(text) = event else {
            return false;
        };
        self.query = text.trim().to_lowercase();
        true
    }

    pub fn matches(&self, hay: &str) -> bool {
        contains_folded(hay, &self.query)
    }

    pub fn clear<V: 'static>(&mut self, cx: &mut Context<V>) {
        self.query.clear();
        self.field.update(cx, |input, cx| input.set_content("", cx));
    }
}

pub fn contains_folded(hay: &str, folded_query: &str) -> bool {
    folded_query.is_empty() || hay.to_lowercase().contains(folded_query)
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;
    use crate::palette::FORGE_DEFAULT;

    struct Host {
        search: SearchState,
    }

    impl gpui::Render for Host {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut Context<Self>,
        ) -> impl gpui::IntoElement {
            gpui::div()
        }
    }

    fn mount(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
        cx.add_window_view(|_window, cx| Host {
            search: SearchState::new(cx, FORGE_DEFAULT, "search"),
        })
    }

    fn typed(text: &str) -> InputEvent {
        InputEvent::Changed(SharedString::from(text.to_owned()))
    }

    #[gpui::test]
    fn a_typed_query_is_trimmed_and_folded_before_it_matches(cx: &mut TestAppContext) {
        let (host, vcx) = mount(cx);

        host.update(vcx, |host, _cx| {
            assert!(host.search.on_changed(&typed("  ЗіРКа ")));
            assert_eq!(host.search.query(), "зірка");
            assert!(host.search.matches("Нова ЗІРКА сходить"));
            assert!(!host.search.matches("місяць"));
        });
    }

    #[gpui::test]
    fn a_query_of_only_spaces_is_empty_and_matches_everything(cx: &mut TestAppContext) {
        let (host, vcx) = mount(cx);

        host.update(vcx, |host, _cx| {
            host.search.on_changed(&typed("   "));
            assert!(host.search.is_empty());
            assert!(host.search.matches("anything"));
        });
    }

    #[gpui::test]
    fn an_event_that_is_not_a_change_leaves_the_query_alone(cx: &mut TestAppContext) {
        let (host, vcx) = mount(cx);

        host.update(vcx, |host, _cx| {
            host.search.on_changed(&typed("kept"));
            for event in [
                InputEvent::Submitted("other".into()),
                InputEvent::Blurred("other".into()),
                InputEvent::Cancelled,
            ] {
                assert!(!host.search.on_changed(&event));
            }
            assert_eq!(host.search.query(), "kept");
        });
    }
}
