use gpui::{
    Context, Entity, EventEmitter, Focusable, MouseDownEvent, Subscription, Window, div,
    prelude::*, px,
};

use crate::palette::ForgePalette;
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{BORDER_THIN, FONT_XS, Radius, radius};

pub enum InlineEditEvent {
    Commit(String),
    Cancel,
}

/// A single-line inline rename field: a borderless mono input inside an active-field
/// wrapper. Commits on Enter, blur, or a click outside; cancels on Escape. The caret
/// is focused on the frame after it mounts so a double-click lands the cursor at once.
pub struct InlineEdit {
    input: Entity<TextInput>,
    palette: ForgePalette,
    committed: bool,
    _sub: Subscription,
    _focus_sub: Option<Subscription>,
}

impl EventEmitter<InlineEditEvent> for InlineEdit {}

pub fn inline_edit<V: 'static>(
    seed: impl Into<String>,
    palette: ForgePalette,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Entity<InlineEdit> {
    let editor = cx.new(|cx| InlineEdit::new(seed.into(), palette, cx));
    editor.update(cx, |this, cx| this.arm(window, cx));
    editor
}

impl InlineEdit {
    fn new(seed: String, palette: ForgePalette, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut ti = TextInput::new("", cx)
                .with_palette(palette)
                .plain()
                .mono()
                .with_font_size(FONT_XS);
            ti.set_content(seed, cx);
            ti
        });
        let sub = cx.subscribe(&input, |this, _f, event: &InputEvent, cx| match event {
            InputEvent::Submitted(_) => this.commit(cx),
            InputEvent::Cancelled => cx.emit(InlineEditEvent::Cancel),
            InputEvent::Changed(_) => {}
        });
        Self {
            input,
            palette,
            committed: false,
            _sub: sub,
            _focus_sub: None,
        }
    }

    fn arm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.input.read(cx).focus_handle(cx);
        self._focus_sub = Some(
            cx.on_focus_out(&handle, window, |this, _event, _window, cx| {
                this.commit(cx);
            }),
        );
        let target = self.input.clone();
        cx.defer_in(window, move |_this, window, cx| {
            target.update(cx, |f, cx| f.focus(window, cx));
        });
    }

    fn commit(&mut self, cx: &mut Context<Self>) {
        if self.committed {
            return;
        }
        self.committed = true;
        let text = self.input.read(cx).content().trim().to_owned();
        cx.emit(InlineEditEvent::Commit(text));
    }
}

impl Render for InlineEdit {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .px(px(4.0))
            .rounded(radius(Radius::Sm))
            .bg(self.palette.shell)
            .border(BORDER_THIN)
            .border_color(self.palette.border_active)
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| this.commit(cx)))
            .child(self.input.clone())
    }
}
