use gpui::{div, prelude::*};

pub fn hello_label() -> impl IntoElement {
    div().child("hello forge")
}
