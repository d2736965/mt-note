use dioxus::prelude::*;

stylance::import_style!(styles, "toast.module.scss");

static TOAST_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn next_toast_id() -> usize {
    TOAST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub fn push_toast(mut toasts: Signal<Vec<ToastItem>>, message: String) {
    let id = next_toast_id();
    toasts.write().push(ToastItem { id, message });
}

#[derive(Clone, PartialEq)]
pub struct ToastItem {
    pub id: usize,
    pub message: String,
}

pub fn provide_toast_context() -> Signal<Vec<ToastItem>> {
    let toasts: Signal<Vec<ToastItem>> = Signal::new(Vec::new());
    use_context_provider(|| toasts);
    toasts
}

#[component]
pub fn ToastHost() -> Element {
    let toasts: Signal<Vec<ToastItem>> = use_context();
    let mut scheduled: Signal<std::collections::HashSet<usize>> =
        use_signal(std::collections::HashSet::new);

    use_effect(move || {
        let items = toasts.read();
        let new_ids: Vec<usize> = items
            .iter()
            .map(|t| t.id)
            .filter(|id| !scheduled.read().contains(id))
            .collect();
        for id in new_ids {
            scheduled.write().insert(id);
            let mut t = toasts;
            let mut s = scheduled;
            spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
                t.write().retain(|item| item.id != id);
                s.write().remove(&id);
            });
        }
    });

    rsx! {
        div { class: styles::host,
            for toast in toasts.read().iter() {
                div { key: "{toast.id}", class: styles::item, "{toast.message}" }
            }
        }
    }
}
