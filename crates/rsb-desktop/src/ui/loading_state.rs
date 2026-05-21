use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadingStyle {
    Spinner,      // Animated spinner
    Dots,         // Animated dots
    ProgressBar,  // Progress bar with percentage
    Pulse,        // Pulsing animation
}

#[component]
pub fn LoadingState(
    message: String,
    #[props(default = LoadingStyle::Spinner)] style: LoadingStyle,
    #[props(default = 0.0)] progress: f64,
    #[props(default = Option::None)] elapsed_time: Option<String>,
    #[props(default = Option::None)] estimated_time: Option<String>,
) -> Element {
    rsx! {
        div { class: "loading-state-container",
            // Spinner/Animation based on style
            match style {
                LoadingStyle::Spinner => {
                    rsx! {
                        div { class: "spinner-container",
                            div { class: "spinner" }
                        }
                    }
                }
                LoadingStyle::Dots => {
                    rsx! {
                        div { class: "dots-container",
                            span { class: "dot" }
                            span { class: "dot" }
                            span { class: "dot" }
                        }
                    }
                }
                LoadingStyle::ProgressBar => {
                    rsx! {
                        div { class: "progress-container",
                            div {
                                class: "progress-bar",
                                style: "width: {progress * 100.0}%;"
                            }
                        }
                    }
                }
                LoadingStyle::Pulse => {
                    rsx! {
                        div { class: "pulse-container",
                            div { class: "pulse" }
                        }
                    }
                }
            }

            // Message
            p { class: "loading-message",
                "{message}"
            }

            // Time info
            if let Some(elapsed) = elapsed_time {
                div { class: "time-info",
                    p { "⏱️  {elapsed}" }
                }
            }

            if let Some(estimated) = estimated_time {
                div { class: "time-info",
                    p { "⏱️  Estimated: {estimated}" }
                }
            }

            // Progress percentage
            if style == LoadingStyle::ProgressBar && progress > 0.0 {
                p { class: "progress-text",
                    "{((progress * 100.0) as u32)}%"
                }
            }
        }
    }
}

#[component]
pub fn LoadingOverlay(
    is_visible: bool,
    #[props(default = LoadingStyle::Spinner)] style: LoadingStyle,
    message: String,
    #[props(default = 0.0)] progress: f64,
    #[props(default = Option::None)] elapsed_time: Option<String>,
) -> Element {
    if !is_visible {
        return rsx! {
            div {}
        };
    }

    rsx! {
        div { class: "loading-overlay",
            div { class: "loading-content",
                LoadingState {
                    message,
                    style,
                    progress,
                    elapsed_time,
                }
            }
        }
    }
}
