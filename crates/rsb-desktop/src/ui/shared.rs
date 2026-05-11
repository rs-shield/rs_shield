use dioxus::prelude::*;

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
            class: active_class,
            onclick: onclick,
            span { class: "text-lg", "{icon}" }
            span { "{label}" }
        }
    }
}

#[component]
pub fn ProgressBar(progress: f64) -> Element {
    let width_percent = (progress * 100.0).min(100.0).max(0.0);
    rsx! {
        div { class: "progress-container",
            div {
                class: "progress-bar",
                style: "width: {width_percent}%;"
            }
        }
    }
}
