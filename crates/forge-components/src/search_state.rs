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
        self.query.is_empty() || hay.to_lowercase().contains(&self.query)
    }

    pub fn clear<V: 'static>(&mut self, cx: &mut Context<V>) {
        self.query.clear();
        self.field.update(cx, |input, cx| input.set_content("", cx));
    }
}
