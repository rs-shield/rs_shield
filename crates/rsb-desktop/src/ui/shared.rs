use crate::ui::key_lifecycle::progress_fraction;
use dioxus::prelude::*;
use rsb_sdk::core::types::ProgressCallback;
use std::sync::Arc;

/// SDK progress callback that can be called from any thread: updates
/// `progress` and `status` on the UI side while `active` is true (so a late
/// message never overwrites the final result).
pub fn progress_reporter(
    mut progress: Signal<f64>,
    mut status: Signal<String>,
    active: Signal<bool>,
) -> ProgressCallback {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(f64, String)>();
    spawn(async move {
        while let Some((p, msg)) = rx.recv().await {
            if *active.peek() {
                progress.set(p);
                status.set(msg);
            }
        }
    });
    Arc::new(move |i: usize, total: usize, msg: String| {
        let _ = tx.send((
            progress_fraction(i, total),
            format!("[{}/{}] {}", (i + 1).min(total.max(1)), total, msg),
        ));
    })
}

#[component]
pub fn TabButton(
    label: String,
    icon: String,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let active_class = if active {
        "nav-item active"
    } else {
        "nav-item"
    };
    rsx! {
        button {
            class: "{active_class} text-xs",
            onclick: onclick,
            span { class: "text-xs", "{icon}" }
            span { class: "text-xs font-medium", "{label}" }
        }
    }
}

#[component]
pub fn ProgressBar(progress: f64) -> Element {
    let width_percent = if progress.is_nan() {
        0.0
    } else {
        (progress * 100.0).clamp(0.0, 100.0)
    };
    rsx! {
        div { class: "progress-container",
            div {
                class: "progress-bar",
                style: "width: {width_percent}%;"
            }
        }
    }
}
