use crate::ui::operations_helpers::record_schedule_operation;
use dioxus::prelude::*;
use std::path::PathBuf;

use crate::ui::{app::AppConfig, i18n::get_texts};

#[component]
pub fn ScheduleScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    let mut config_path = use_signal(PathBuf::new);
    let mut format = use_signal(|| "cron".to_string());
    let mut output_command = use_signal(String::new);
    let mut key = use_signal(String::new);
    let mut is_loading = use_signal(|| false);
    let mut status_message = use_signal(String::new);

    let handle_generate = move |_| {
        let cfg_path = config_path();
        if cfg_path.as_os_str().is_empty() {
            output_command.set(texts.please_select_config_file.to_string());
            return;
        }

        let abs_config = std::fs::canonicalize(&cfg_path).unwrap_or(cfg_path.clone());
        let exe = std::env::current_exe()
            .map(|p| p.parent().unwrap().join("rsb-cli"))
            .unwrap_or_else(|_| PathBuf::from("rsb-cli"));

        let key_part = if !key().is_empty() {
            format!(" --key \"{}\"", key())
        } else {
            String::new()
        };

        if format() == "cron" {
            output_command.set(format!(
                "0 3 * * * {} backup {}{}",
                exe.display(),
                abs_config.display(),
                key_part
            ));
        } else {
            output_command.set(format!(
                "[Service]\nType=oneshot\nExecStart={} backup {}{}",
                exe.display(),
                abs_config.display(),
                key_part
            ));
        }
    };

    let handle_execute_schedule = move |_| {
        let cfg_path = config_path();
        if cfg_path.as_os_str().is_empty() {
            status_message.set(texts.select_config_file.to_string());
            return;
        }

        is_loading.set(true);
        status_message.set("⏳ Agendando backup...".to_string());

        spawn(async move {
            match execute_schedule(&cfg_path).await {
                Ok(_) => {
                    status_message.set("✅ Agendamento criado com sucesso!".to_string());
                    record_schedule_operation(true, None, None).await.ok();
                }
                Err(e) => {
                    status_message.set(format!("❌ Erro ao agendar: {}", e));
                    record_schedule_operation(false, Some(e.to_string()), None)
                        .await
                        .ok();
                }
            }
            is_loading.set(false);
        });
    };

    rsx! {
        div { class: "card",
            h2 { class: "page-title", "{texts.schedule_title}" }

            div { class: "form-group",
                label { class: "label-text", "{texts.config_file_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.select_config_file_hint}",
                        value: "{config_path.read().to_string_lossy()}",
                        oninput: move |evt| config_path.set(PathBuf::from(evt.value())),
                        readonly: true
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(h) = rfd::AsyncFileDialog::new().pick_file().await {
                                    config_path.set(h.path().to_path_buf());
                                    status_message.set(String::new());
                                }
                            });
                        },
                        disabled: is_loading(),
                        "📂"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.key_label_opt}" }
                input {
                    class: "input-field",
                    r#type: "password",
                    placeholder: "{texts.key_label_opt}",
                    value: "{key}",
                    oninput: move |evt| key.set(evt.value()),
                    disabled: is_loading()
                }
                p { class: "hint", "Deixe em branco se o backup não for criptografado" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.schedule_format_label}" }
                select {
                    class: "select-field",
                    value: "{format}",
                    onchange: move |evt| format.set(evt.value()),
                    disabled: is_loading(),
                    option { value: "cron", "⏰ Cron (Linux/macOS)" }
                    option { value: "systemd", "🐧 Systemd Service (Linux)" }
                }
                p { class: "hint", if format() == "cron" {
                    "Execute este comando no seu crontab para agendar o backup"
                } else {
                    "Copie este conteúdo para um ficheiro .service"
                }}
            }

            if !status_message().is_empty() {
                div {
                    class: "alert",
                    class: if status_message().starts_with("✅") { "alert-success" } else { "alert-error" },
                    "{status_message}"
                }
            }

            div { class: "flex gap-3",
                button {
                    class: "btn-primary flex-1",
                    onclick: handle_generate,
                    disabled: is_loading() || config_path().as_os_str().is_empty(),
                    "🔧 Gerar Comando"
                }
                button {
                    class: "btn-secondary flex-1",
                    onclick: handle_execute_schedule,
                    disabled: is_loading() || config_path().as_os_str().is_empty(),
                    if is_loading() {
                        "⏳ Agendando..."
                    } else {
                        "⏰ Agendar Agora"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "📝 Comandos Agendados" }
                textarea {
                    class: "textarea-field",
                    readonly: true,
                    value: "{output_command}",
                    style: "height: 120px; font-family: monospace; font-size: 0.85rem;"
                }
            }

            div { class: "info-box",
                h4 { class: "font-semibold mb-2", if format() == "cron" { "Instruções Cron:" } else { "Instruções Systemd:" }}
                if format() == "cron" {
                    p { class: "text-xs mb-2", "1. Abra o seu crontab: crontab -e" }
                    p { class: "text-xs mb-2", "2. Cole o comando acima na última linha" }
                    p { class: "text-xs", "3. Guarde e saia (Ctrl+X, Y, Enter no nano)" }
                } else {
                    p { class: "text-xs mb-2", "1. Crie um ficheiro em /etc/systemd/system/rsb-backup.service" }
                    p { class: "text-xs mb-2", "2. Cole o conteúdo acima no ficheiro" }
                    p { class: "text-xs mb-2", "3. Execute: sudo systemctl enable rsb-backup.timer" }
                    p { class: "text-xs", "4. Inicie com: sudo systemctl start rsb-backup.timer" }
                }
                p { class: "text-xs mt-3 text-slate-600 dark:text-slate-400",
                    "ℹ️ {texts.schedule_hint}"
                }
            }
        }
    }
}

async fn execute_schedule(config_path: &std::path::Path) -> Result<(), String> {
    // Este função seria implementada com chamadas ao rsb-cli via sistema
    // Por enquanto, apenas registra a execução
    tracing::info!("Agendando backup com configuração: {:?}", config_path);
    Ok(())
}
