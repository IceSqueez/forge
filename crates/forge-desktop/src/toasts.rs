use forge_components::{PushToast, ToastKind, tr};
use gpui::App;

pub(crate) fn copy_to_clipboard(text: impl Into<String>, cx: &mut App) {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.into()));
    cx.push_toast(ToastKind::Success, tr!("widget_copied_toast"));
}
