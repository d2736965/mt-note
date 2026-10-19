use dioxus::prelude::*;

use crate::frontend::js_bridge::{DioxusBridge, js_api};

static BACK_INTERCEPTOR_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn next_id() -> usize {
    BACK_INTERCEPTOR_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub fn provide_back_intercept_context() -> Signal<Option<usize>> {
    let requested: Signal<Option<usize>> = Signal::new(None);
    use_context_provider(|| requested);
    requested
}

pub fn use_back_interceptor(active: bool, mut on_intercept: impl FnMut() + 'static) {
    let id = use_hook(next_id);
    let mut registered = use_signal(|| false);
    let mut active_signal = use_signal(|| active);
    if active_signal() != active {
        active_signal.set(active);
    }
    let mut requested: Signal<Option<usize>> = use_context();

    use_effect(move || {
        let active = active_signal();
        if active && !registered() {
            js_api::back_interceptor::push::<DioxusBridge>(id as f64);
            registered.set(true);
        } else if !active && registered() {
            js_api::back_interceptor::remove::<DioxusBridge>(id as f64);
            registered.set(false);
        }
    });

    use_effect(move || {
        if let Some(rid) = requested()
            && active_signal()
            && rid == id
        {
            requested.set(None);
            on_intercept();
        }
    });

    use_drop(move || {
        if registered() {
            js_api::back_interceptor::remove::<DioxusBridge>(id as f64);
        }
    });
}
