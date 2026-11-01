use forge_components::tr;
use gpui::{App, Context, Task};

use crate::app::async_bridge::{self, Generation};
use crate::shared::option_choices::{ChoiceOptions, resolve_options, watch_option_changes};
use forge_host::integrations::BuiltinRegistry;

pub(super) struct ChoiceFeed {
    builtins: BuiltinRegistry,
    rt_handle: tokio::runtime::Handle,
    options: ChoiceOptions,
    generation: Generation,
    watch: Option<Task<()>>,
}

impl ChoiceFeed {
    pub(super) fn new(rt_handle: tokio::runtime::Handle) -> Self {
        Self {
            builtins: BuiltinRegistry::default(),
            rt_handle,
            options: ChoiceOptions::new(),
            generation: Generation::default(),
            watch: None,
        }
    }

    pub(super) fn set_builtins(&mut self, builtins: BuiltinRegistry) {
        self.builtins = builtins;
    }

    pub(super) fn options(&self) -> &ChoiceOptions {
        &self.options
    }
}

pub(super) trait ChoiceHost: Sized + 'static {
    fn feed(&mut self) -> &mut ChoiceFeed;
    fn option_keys(&self, cx: &App) -> Vec<String>;
    fn refresh_choices(&mut self, blank: &str, cx: &mut Context<Self>);
}

pub(super) fn start_option_choices<V: ChoiceHost>(view: &mut V, cx: &mut Context<V>) {
    view.refresh_choices(&tr!("config_form_choice_any"), cx);
    let keys = view.option_keys(cx);
    let builtins = view.feed().builtins.clone();
    view.feed().watch = watch_option_changes(&builtins, keys, reload_option_choices::<V>, cx);
    reload_option_choices(view, cx);
}

fn reload_option_choices<V: ChoiceHost>(view: &mut V, cx: &mut Context<V>) {
    let keys = view.option_keys(cx);
    if keys.is_empty() {
        return;
    }
    let feed = view.feed();
    let builtins = feed.builtins.clone();
    let ticket = feed.generation.next();
    async_bridge::run_async(
        &feed.rt_handle,
        resolve_options(builtins, keys),
        move |view: &mut V, options, cx| {
            if view.feed().generation.is_current(ticket) {
                view.feed().options.extend(options);
                view.refresh_choices(&tr!("config_form_choice_any"), cx);
                cx.notify();
            }
        },
        cx,
    );
}
