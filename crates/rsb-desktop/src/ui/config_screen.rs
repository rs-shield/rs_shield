use dioxus::prelude::*;

use crate::ui::{
    app::AppConfig,
    i18n::{Language, Theme, get_texts},
    s3_settings,
    s3_tester::S3ConnectionTester,
};

#[component]
pub fn ConfigScreen() -> Element {
    let mut app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut remember_status = use_signal(|| None::<(bool, String)>);
    let mut remember_busy = use_signal(|| false);

    // Opt-in: grava as chaves no ficheiro cifrado em vez de só na sessão.
    let on_remember = move |_| {
        let access_key = app_config.s3_access_key();
        let secret_key = app_config.s3_secret_key();
        if access_key.trim().is_empty() || secret_key.trim().is_empty() {
            remember_status.set(Some((false, texts.s3_remember_missing.to_string())));
            return;
        }
        remember_busy.set(true);
        remember_status.set(Some((true, texts.s3_remember_saving.to_string())));
        spawn(async move {
            // keyring + PBKDF2 são bloqueantes: fora da thread da UI.
            let result = tokio::task::spawn_blocking(move || {
                s3_settings::remember_credentials(&access_key, &secret_key)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
            remember_busy.set(false);
            match result {
                Ok(_) => remember_status.set(Some((true, texts.s3_remember_ok.to_string()))),
                Err(e) => remember_status.set(Some((false, format!("❌ {}", e)))),
            }
        });
    };

    rsx! {
        div { class: "card",
            h2 { class: "page-title", "{texts.config_title}" }

            div { class: "form-group",
                label { class: "label-text", "{texts.language_label}" }
                select {
                    class: "select-field",
                    onchange: move |evt| match evt.value().as_str() {
                        "en" => app_config.language.set(Language::En),
                        _ => app_config.language.set(Language::Pt),
                    },
                    option { value: "pt", selected: app_config.language() == Language::Pt, "Português 🇵🇹" }
                    option { value: "en", selected: app_config.language() == Language::En, "English 🇺🇸" }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.theme_label}" }
                select {
                    class: "select-field",
                    onchange: move |evt| match evt.value().as_str() {
                        "light" => app_config.theme.set(Theme::Light),
                        "dark" => app_config.theme.set(Theme::Dark),
                        _ => app_config.theme.set(Theme::System),
                    },
                    option { value: "system", selected: app_config.theme() == Theme::System, "🔄 {texts.theme_system}" }
                    option { value: "light", selected: app_config.theme() == Theme::Light, "☀️ {texts.theme_light}" }
                    option { value: "dark", selected: app_config.theme() == Theme::Dark, "🌙 {texts.theme_dark}" }
                }
            }

            h3 { class: "section-title", "{texts.advanced_settings_title}" }

            div { class: "form-group",
                label { class: "label-text", "{texts.compression_level_label}" }
                div { class: "flex items-center gap-4",
                    input {
                        class: "input-field",
                        r#type: "range",
                        min: "0",
                        max: "22",
                        value: "{app_config.compression_level}",
                        oninput: move |evt| app_config.compression_level.set(evt.value())
                    }
                    span { class: "text-sm font-semibold text-slate-700 dark:text-slate-300 w-8 text-center", "{app_config.compression_level}" }
                }
                p { class: "hint", "{texts.compression_hint}" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.low_battery_pause_label}" }
                input {
                    class: "input-field",
                    r#type: "number",
                    min: "0",
                    max: "100",
                    value: "{app_config.pause_on_low_battery}",
                    oninput: move |evt| app_config.pause_on_low_battery.set(evt.value())
                }
                p { class: "hint", "{texts.battery_pause_hint}" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.high_cpu_pause_label}" }
                input {
                    class: "input-field",
                    r#type: "number",
                    min: "0",
                    max: "100",
                    value: "{app_config.pause_on_high_cpu}",
                    oninput: move |evt| app_config.pause_on_high_cpu.set(evt.value())
                }
                p { class: "hint", "{texts.cpu_pause_hint}" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.backup_mode_label}" }
                select {
                    class: "select-field",
                    onchange: move |evt| app_config.backup_mode.set(evt.value()),
                    option { value: "incremental", selected: app_config.backup_mode() == "incremental", "{texts.incremental_mode}" }
                    option { value: "full", selected: app_config.backup_mode() == "full", "{texts.full_mode}" }
                }
            }

            h3 { class: "section-title", "{texts.encryption_section_title}" }

            div { class: "form-group",
                label { class: "label-text", "{texts.encrypt_patterns_label}" }
                textarea {
                    class: "textarea-field",
                    value: "{app_config.encrypt_patterns}",
                    oninput: move |evt| app_config.encrypt_patterns.set(evt.value())
                }
                p { class: "hint", "{texts.crypto_hint}" }
            }

            h3 { class: "section-title", "{texts.exclude_section_title}" }

            div { class: "form-group",
                label { class: "label-text", "{texts.ignore_files_label}" }
                textarea {
                    class: "textarea-field",
                    value: "{app_config.exclude_patterns}",
                    oninput: move |evt| app_config.exclude_patterns.set(evt.value())
                }
                p { class: "hint", "{texts.exclude_hint}" }
            }

            h3 { class: "section-title", "{texts.s3_config_title}" }

            div { class: "form-group",
                label { class: "label-text", "{texts.bucket_label}" }
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "my-backup-bucket",
                    value: "{app_config.s3_bucket}",
                    oninput: move |evt| app_config.s3_bucket.set(evt.value())
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.region_label}" }
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "us-east-1",
                    value: "{app_config.s3_region}",
                    oninput: move |evt| app_config.s3_region.set(evt.value())
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.endpoint_label}" }
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "https://s3.amazonaws.com",
                    value: "{app_config.s3_endpoint}",
                    oninput: move |evt| app_config.s3_endpoint.set(evt.value())
                }
            }

            div { class: "form-group",
                label { class: "label-text", "Access Key" }
                input {
                    class: "input-field",
                    r#type: "password",
                    placeholder: "AWS Access Key ID",
                    value: "{app_config.s3_access_key}",
                    oninput: move |evt| app_config.s3_access_key.set(evt.value())
                }
            }

            div { class: "form-group",
                label { class: "label-text", "Secret Key" }
                input {
                    class: "input-field",
                    r#type: "password",
                    placeholder: "AWS Secret Access Key",
                    value: "{app_config.s3_secret_key}",
                    oninput: move |evt| app_config.s3_secret_key.set(evt.value())
                }
            }

            div { class: "form-group",
                button {
                    class: "btn-secondary",
                    r#type: "button",
                    disabled: remember_busy(),
                    onclick: on_remember,
                    "{texts.s3_remember_button}"
                }
                p { class: "hint", "{texts.s3_remember_hint}" }
                if let Some((ok, msg)) = remember_status() {
                    div { class: if ok { "info-box" } else { "error-box" }, "{msg}" }
                }
            }

            S3ConnectionTester {
                bucket: app_config.s3_bucket,
                region: app_config.s3_region,
                endpoint: app_config.s3_endpoint,
                access_key: app_config.s3_access_key,
                secret_key: app_config.s3_secret_key,
            }

            div { class: "info-box",
                "ℹ️ {texts.s3_hint}"
            }

        }
    }
}
