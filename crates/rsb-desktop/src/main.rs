#![allow(non_snake_case)]

mod ui;

use dioxus::prelude::*;
use dioxus_desktop::{Config, WindowBuilder};

fn main() {
    tracing_subscriber::fmt::init();

    let config =
        Config::new().with_window(WindowBuilder::new().with_title("RSB - Rust Shield Backup"));

    LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(ui::app::App);
}
