#![allow(non_snake_case)]

mod ui;

use dioxus::prelude::*;
use dioxus_desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    tracing_subscriber::fmt::init();

    let config = Config::new().with_window(
        WindowBuilder::new()
            .with_title("RSB - Rust Shield Backup")
            // Define tamanho inicial otimizado para diferentes resoluções
            .with_inner_size(LogicalSize::new(1280.0, 900.0))
            // Tamanho mínimo para garantir usabilidade
            .with_min_inner_size(LogicalSize::new(1024.0, 768.0)), // Inicia maximizado em alguns SOs (comentado para ter controle manual)
                                                                   // .with_maximized(true)
    );

    LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(ui::app::App);
}
