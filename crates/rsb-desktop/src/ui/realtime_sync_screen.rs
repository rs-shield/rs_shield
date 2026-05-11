use crate::ui::{app::AppConfig, i18n::get_texts};
use battery::Manager;
use dioxus::prelude::*;
use notify_rust::Notification;
use rsb_sdk::realtime::{create_backup, sync_all_files};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Event structure para webhook
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotificationEvent {
    event_type: String, // "sync_complete", "backup_created", "error", "battery_low"
    title: String,
    message: String,
    timestamp: String,
    status: String, // "success", "error", "warning"
}

/// Enviar notificação do sistema com fallback e webhook
fn send_system_notification(
    title: &str,
    message: &str,
    notification_type: &str,
    webhook_url: Option<&str>,
) {
    let icon = match notification_type {
        "success" => "dialog-information",
        "error" => "dialog-error",
        "warning" => "dialog-warning",
        "info" => "dialog-information",
        _ => "dialog-information",
    };

    // Criar evento para webhook
    let event = NotificationEvent {
        event_type: notification_type.to_string(),
        title: title.to_string(),
        message: message.to_string(),
        timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        status: match notification_type {
            "success" => "success".to_string(),
            "error" => "error".to_string(),
            "warning" => "warning".to_string(),
            _ => "info".to_string(),
        },
    };

    // Tenta enviar via notify-rust (sistema)
    let title = title.to_string();
    let message = message.to_string();
    let icon = icon.to_string();
    let webhook_url = webhook_url.map(|s| s.to_string());

    std::thread::spawn(move || {
        // 1. Tenta notificação do sistema
        let notif_result = Notification::new()
            .summary(&title)
            .body(&message)
            .icon(&icon)
            .timeout(5000)
            .show();

        match notif_result {
            Ok(_) => {
                println!(
                    "✅ [notify-rust] Notificação enviada: {} - {}",
                    title, message
                );
            }
            Err(e) => {
                eprintln!("⚠️ [notify-rust] Falhou: {:?}", e);
                #[cfg(target_os = "macos")]
                {
                    eprintln!("   macOS tip: System Preferences > Notifications > Terminal");
                    eprintln!("   Certifique-se de que 'Allow Notifications' está ativado");
                }
            }
        }

        // 2. Enviar para webhook se configurado
        if let Some(url) = webhook_url {
            send_webhook_notification(&event, &url);
        }
    });
}

/// Enviar notificação para webhook
fn send_webhook_notification(event: &NotificationEvent, webhook_url: &str) {
    let event = event.clone();
    let webhook_url = webhook_url.to_string();

    std::thread::spawn(async move || {
        let client = reqwest::Client::new();

        match client.post(&webhook_url).json(&event).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    println!("✅ [Webhook] Enviado com sucesso para {}", webhook_url);
                } else {
                    eprintln!("⚠️ [Webhook] Status {}: {}", response.status(), webhook_url);
                }
            }
            Err(e) => {
                eprintln!("⚠️ [Webhook] Erro ao enviar: {:?}", e);
            }
        }
    });
}

/// Verificar estado da bateria
fn check_battery_status() -> (f32, bool, String) {
    match Manager::new() {
        Ok(manager) => match manager.batteries() {
            Ok(mut batteries) => {
                if let Some(battery) = batteries.next() {
                    match battery {
                        Ok(batt) => {
                            let percent = batt.state_of_charge().value * 100.0;
                            let is_charging = batt.state() == battery::State::Charging;
                            let energy_full = batt.energy_full().value;
                            let energy = batt.energy().value;
                            let health = format!("{:.1}%", (energy_full / energy) * 100.0);

                            return (percent, is_charging, health);
                        }
                        Err(e) => {
                            eprintln!("⚠️ Erro ao obter info da bateria: {:?}", e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("⚠️ Erro ao listar baterias: {:?}", e);
            }
        },
        Err(e) => {
            eprintln!("⚠️ Manager de bateria indisponível: {:?}", e);
        }
    }

    // Fallback: simular com valores padrão
    (100.0, true, "N/A".to_string())
}

/// Evento de sincronização para histórico
#[derive(Clone, Debug)]
struct SyncEvent {
    timestamp: String,
    event_type: String, // "file_synced", "backup_created", "error"
    details: String,
}

#[component]
pub fn RealtimeSyncScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    let mut source_path = use_signal(PathBuf::new);
    let mut dest_path = use_signal(PathBuf::new);
    let mut backup_path = use_signal(PathBuf::new);
    let mut backup_password = use_signal(String::new);
    let mut is_monitoring = use_signal(|| false);
    let mut status_msg = use_signal(|| "Pronto para monitorar e fazer backup".to_string());

    let mut files_synced = use_signal(|| 0usize);
    let mut files_changed = use_signal(|| 0usize);
    let mut backups_created = use_signal(|| 0usize);
    let mut sync_errors = use_signal(|| 0usize);

    // Histórico de eventos para dashboard
    let mut sync_history = use_signal(Vec::<SyncEvent>::new);
    let mut last_sync_time = use_signal(|| String::from("Nunca"));
    let mut success_rate = use_signal(|| 100.0);

    // Notificações do sistema
    let mut notifications_enabled = use_signal(|| true);

    // AlertDialog (fallback visual para notificações)
    let mut show_alert = use_signal(|| false);
    let mut alert_title = use_signal(String::new);
    let mut alert_message = use_signal(String::new);
    let mut alert_type = use_signal(|| String::from("info")); // "success", "error", "warning", "info"

    // Monitoramento de bateria
    let mut battery_percent = use_signal(|| 100.0);
    let mut battery_status = use_signal(|| "AC".to_string());
    let mut show_low_battery_alert = use_signal(|| false);
    let mut battery_health = use_signal(|| String::from("N/A"));

    // Webhook notifications
    let mut webhook_enabled = use_signal(|| false);
    let mut webhook_url = use_signal(String::new);
    let _show_webhook_config = use_signal(|| false);

    // Configuração de ignores (padrões)
    let mut ignore_patterns = use_signal(|| {
        vec![
            ".*\\.tmp$".to_string(),
            ".*\\.lock$".to_string(),
            ".*\\.swp$".to_string(),
            ".git".to_string(),
            ".DS_Store".to_string(),
            "node_modules".to_string(),
            "target".to_string(),
        ]
    });
    let mut new_pattern = use_signal(String::new);
    let mut show_ignore_editor = use_signal(|| false);
    let mut pattern_test_input = use_signal(String::new);
    let mut pattern_test_result = use_signal(String::new);

    // Função para mostrar AlertDialog visual
    let mut show_dialog = move |title: &str, message: &str, alert_type_str: &str| {
        alert_title.set(title.to_string());
        alert_message.set(message.to_string());
        alert_type.set(alert_type_str.to_string());
        show_alert.set(true);

        // Auto-fechar após 5 segundos (para "success" e "info")
        if alert_type_str == "success" || alert_type_str == "info" {
            spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                show_alert.set(false);
            });
        }
    };

    let handle_select_source = move |_| {
        spawn(async move {
            if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                source_path.set(folder.path().to_path_buf());
                status_msg.set(texts.source_folder_selected.to_string());
            }
        });
    };

    let handle_select_dest = move |_| {
        spawn(async move {
            if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                dest_path.set(folder.path().to_path_buf());
                status_msg.set(texts.dest_folder_selected.to_string());
            }
        });
    };

    let handle_select_backup = move |_| {
        spawn(async move {
            if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                backup_path.set(folder.path().to_path_buf());
                status_msg.set(texts.backup_folder_selected.to_string());
            }
        });
    };

    let handle_start_monitoring = move |_| {
        if source_path().as_os_str().is_empty() {
            status_msg.set(texts.select_source_folder.to_string());
            return;
        }
        if dest_path().as_os_str().is_empty() {
            status_msg.set(texts.select_dest_folder.to_string());
            return;
        }
        if backup_path().as_os_str().is_empty() {
            status_msg.set(texts.select_backup_folder.to_string());
            return;
        }
        if backup_password().is_empty() {
            status_msg.set(texts.enter_backup_password.to_string());
            return;
        }

        is_monitoring.set(true);
        status_msg.set("🟢 Monitorando e fazendo backup...".to_string());
        files_synced.set(0);
        files_changed.set(0);
        backups_created.set(0);
        sync_errors.set(0);

        // Notificar início de monitoramento
        if notifications_enabled() {
            send_system_notification(
                "RS Shield",
                "Monitoramento iniciado com sucesso",
                "info",
                None,
            );
        }

        let src = source_path();
        let dst = dest_path();
        let bkp = backup_path();
        let pwd = backup_password();
        let notify_enabled = notifications_enabled();
        let webhook_enabled_val = webhook_enabled();
        let webhook_url_val = webhook_url();

        spawn(async move {
            // Sync inicial com timeout
            println!("[DEBUG] Iniciando sync inicial...");
            status_msg.set(
                "⏳ Processando sincronização inicial (pode levar alguns segundos)...".to_string(),
            );

            let sync_result = tokio::time::timeout(
                tokio::time::Duration::from_secs(30),
                sync_all_files(&src, &dst),
            )
            .await;

            match sync_result {
                Ok(Ok(count)) => {
                    println!("[DEBUG] ✅ Sync inicial completo: {} ficheiros", count);
                    files_synced.set(count);
                    status_msg.set("🟢 Sync inicial ok. Monitorando mudanças...".to_string());

                    // Monitorar alterações
                    let mut last_count = count;
                    let mut last_battery_check = 0usize;
                    loop {
                        if !is_monitoring() {
                            println!("[DEBUG] ⏹️ Monitoramento parado");
                            status_msg.set("⏹️ Monitoramento parado".to_string());
                            break;
                        }

                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                        // Verificar bateria a cada 10 segundos
                        last_battery_check += 1;
                        if last_battery_check >= 5 {
                            last_battery_check = 0;
                            let (percent, is_charging, health) = check_battery_status();

                            battery_percent.set(percent);
                            battery_status.set(if is_charging {
                                "⚡ Carregando".to_string()
                            } else {
                                "🔋 Bateria".to_string()
                            });
                            battery_health.set(health);

                            // Alerta de bateria baixa (< 15%)
                            if percent < 15.0 && !is_charging && !show_low_battery_alert() {
                                show_low_battery_alert.set(true);
                                show_dialog(
                                    "⚠️ Bateria Baixa",
                                    &format!(
                                        "Bateria em {:.0}% - Considere conectar o carregador",
                                        percent
                                    ),
                                    "warning",
                                );

                                if notify_enabled {
                                    let webhook = if webhook_enabled_val {
                                        Some(webhook_url_val.as_str())
                                    } else {
                                        None
                                    };
                                    send_system_notification(
                                        "⚠️ Bateria Baixa",
                                        &format!(
                                            "Bateria em {:.0}% - Sincronização pode ser pausada",
                                            percent
                                        ),
                                        "warning",
                                        webhook,
                                    );
                                }
                            } else if percent > 20.0 && is_charging {
                                show_low_battery_alert.set(false);
                            }
                        }

                        // Verificar novas mudanças
                        match sync_all_files(&src, &dst).await {
                            Ok(new_count) => {
                                if new_count > last_count {
                                    let changes = new_count - last_count;
                                    println!("[DEBUG] 📄 Detectadas {} mudanças", changes);
                                    files_changed.set(files_changed() + changes);
                                    last_count = new_count;

                                    // Adicionar evento ao histórico
                                    let timestamp =
                                        chrono::Local::now().format("%H:%M:%S").to_string();
                                    let mut history = sync_history();
                                    history.push(SyncEvent {
                                        timestamp,
                                        event_type: "file_synced".to_string(),
                                        details: format!("{} arquivo(s) sincronizado(s)", changes),
                                    });
                                    // Manter apenas os últimos 20 eventos
                                    if history.len() > 20 {
                                        history.remove(0);
                                    }
                                    sync_history.set(history);
                                    last_sync_time
                                        .set(chrono::Local::now().format("%H:%M:%S").to_string());

                                    // Notificar sincronização
                                    if notify_enabled {
                                        let webhook = if webhook_enabled_val {
                                            Some(webhook_url_val.as_str())
                                        } else {
                                            None
                                        };
                                        send_system_notification(
                                            "📄 Arquivos Sincronizados",
                                            &format!(
                                                "{} arquivo(s) sincronizado(s) com sucesso",
                                                changes
                                            ),
                                            "success",
                                            webhook,
                                        );
                                    }

                                    // Fazer backup automático das mudanças (com criptografia)
                                    if let Ok(backup_name) =
                                        create_backup(&src, &bkp, Some(&pwd)).await
                                    {
                                        backups_created.set(backups_created() + 1);
                                        status_msg.set(format!("✅ {}", backup_name));

                                        // Adicionar evento de backup ao histórico
                                        let timestamp_bkp =
                                            chrono::Local::now().format("%H:%M:%S").to_string();
                                        let mut history = sync_history();
                                        history.push(SyncEvent {
                                            timestamp: timestamp_bkp,
                                            event_type: "backup_created".to_string(),
                                            details: "Backup automático criado com sucesso"
                                                .to_string(),
                                        });
                                        if history.len() > 20 {
                                            history.remove(0);
                                        }
                                        sync_history.set(history);

                                        // Notificar criação de backup
                                        if notify_enabled {
                                            let webhook = if webhook_enabled_val {
                                                Some(webhook_url_val.as_str())
                                            } else {
                                                None
                                            };
                                            send_system_notification(
                                            "💾 Backup Criado",
                                            &format!("Backup '{}' criado com sucesso e criptografado", backup_name),
                                            "success",
                                            webhook
                                        );
                                        }
                                    } else {
                                        sync_errors.set(sync_errors() + 1);

                                        // Adicionar evento de erro
                                        let timestamp_err =
                                            chrono::Local::now().format("%H:%M:%S").to_string();
                                        let mut history = sync_history();
                                        history.push(SyncEvent {
                                            timestamp: timestamp_err,
                                            event_type: "error".to_string(),
                                            details: "Falha ao criar backup".to_string(),
                                        });
                                        if history.len() > 20 {
                                            history.remove(0);
                                        }
                                        sync_history.set(history);

                                        // Notificar erro
                                        if notify_enabled {
                                            let webhook = if webhook_enabled_val {
                                                Some(webhook_url_val.as_str())
                                            } else {
                                                None
                                            };
                                            send_system_notification(
                                            "❌ Erro de Backup",
                                            "Falha ao criar backup automático - verifique as permissões",
                                            "error",
                                            webhook
                                        );
                                        }
                                    }

                                    // Atualizar taxa de sucesso
                                    let total = backups_created() + sync_errors();
                                    if total > 0 {
                                        let rate =
                                            (backups_created() as f64 / total as f64) * 100.0;
                                        success_rate.set(rate);
                                    }
                                }
                            }
                            Err(e) => {
                                println!("[DEBUG] ❌ Erro no sync: {}", e);
                                sync_errors.set(sync_errors() + 1);

                                // Adicionar evento de erro
                                let timestamp_err =
                                    chrono::Local::now().format("%H:%M:%S").to_string();
                                let mut history = sync_history();
                                history.push(SyncEvent {
                                    timestamp: timestamp_err,
                                    event_type: "error".to_string(),
                                    details: format!("Erro de sincronização: {}", e),
                                });
                                if history.len() > 20 {
                                    history.remove(0);
                                }
                                sync_history.set(history);

                                // Notificar erro
                                if notify_enabled {
                                    let webhook = if webhook_enabled_val {
                                        Some(webhook_url_val.as_str())
                                    } else {
                                        None
                                    };
                                    send_system_notification(
                                        "❌ Erro de Sincronização",
                                        &format!("Falha na sincronização: {}", e),
                                        "error",
                                        webhook,
                                    );
                                }
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    println!("[DEBUG] Erro no sync inicial: {}", e);
                    status_msg.set(format!("❌ Erro no sync inicial: {}", e));
                    is_monitoring.set(false);
                }
                Err(_) => {
                    println!("[DEBUG] Timeout: Sincronização inicial excedeu 30 segundos");
                    status_msg.set(
                        "❌ Tempo limite excedido (30s) - pasta com muitos ficheiros?".to_string(),
                    );
                    is_monitoring.set(false);
                }
            }
        });
    };

    let handle_stop_monitoring = move |_| {
        is_monitoring.set(false);
        status_msg.set("⏹️ Monitoramento parado".to_string());

        // Notificar parada de monitoramento
        if notifications_enabled() {
            send_system_notification(
                "⏹️ Monitoramento Parado",
                "O monitoramento foi parado com sucesso",
                "info",
                None,
            );
        }
    };

    // Handlers para gerenciar padrões de ignore
    let handle_add_pattern = move |_| {
        let pattern = new_pattern().trim().to_string();
        if !pattern.is_empty() {
            let mut patterns = ignore_patterns();
            if !patterns.contains(&pattern) {
                patterns.push(pattern);
                ignore_patterns.set(patterns);
                new_pattern.set(String::new());
                status_msg.set("✅ Padrão adicionado com sucesso".to_string());
            } else {
                status_msg.set("❌ Padrão já existe".to_string());
            }
        } else {
            status_msg.set("❌ Digite um padrão válido".to_string());
        }
    };

    let handle_test_pattern = move |_| {
        let test_input = pattern_test_input().trim().to_string();
        if test_input.is_empty() {
            pattern_test_result.set("❌ Digite um caminho de arquivo para testar".to_string());
            return;
        }

        let patterns = ignore_patterns();
        let mut matched_patterns = Vec::new();

        for pattern in &patterns {
            if let Ok(regex) = regex::Regex::new(pattern) {
                if regex.is_match(&test_input) {
                    matched_patterns.push(pattern.clone());
                }
            }
        }

        if matched_patterns.is_empty() {
            pattern_test_result.set(format!("✅ '{}' NÃO seria ignorado", test_input));
        } else {
            pattern_test_result.set(format!(
                "🚫 '{}' seria ignorado por: {}",
                test_input,
                matched_patterns.join(", ")
            ));
        }
    };

    let handle_reset_patterns = move |_| {
        ignore_patterns.set(vec![
            ".*\\.tmp$".to_string(),
            ".*\\.lock$".to_string(),
            ".*\\.swp$".to_string(),
            ".git".to_string(),
            ".DS_Store".to_string(),
            "node_modules".to_string(),
            "target".to_string(),
        ]);
        status_msg.set("✅ Padrões restaurados para padrão".to_string());
    };

    rsx! {
        div { class: "card",
            h2 { class: "page-title", "⚡ Real-Time Sync com Backup" }

            p { class: "hint mb-4",
                "Monitore uma pasta, sincronize em tempo real E crie backups automáticos das mudanças."
            }

            div { class: "form-group",
                label { class: "label-text", "Pasta a Monitorar (Origem)" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "Clique para selecionar",
                        value: "{source_path.read().to_string_lossy()}",
                        readonly: true,
                        disabled: is_monitoring()
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_select_source,
                        disabled: is_monitoring(),
                        "📂"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "Pasta de Sincronização (Destino)" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "Clique para selecionar",
                        value: "{dest_path.read().to_string_lossy()}",
                        readonly: true,
                        disabled: is_monitoring()
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_select_dest,
                        disabled: is_monitoring(),
                        "📂"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "Pasta de Backup (Automático)" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "Clique para selecionar",
                        value: "{backup_path.read().to_string_lossy()}",
                        readonly: true,
                        disabled: is_monitoring()
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_select_backup,
                        disabled: is_monitoring(),
                        "💾"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "🔐 Senha para Criptografia" }
                input {
                    class: "input-field",
                    r#type: "password",
                    placeholder: "Digite uma senha forte",
                    value: "{backup_password()}",
                    oninput: move |evt| backup_password.set(evt.value()),
                    disabled: is_monitoring()
                }
            }

            if !status_msg().is_empty() {
                div {
                    class: "alert",
                    class: if status_msg().starts_with("✅") { "alert-success" } else if status_msg().starts_with("❌") { "alert-error" } else { "alert-info" },
                    "{status_msg}"
                }
            }

            // Configurações de notificações
            div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-4 mb-4 border border-slate-200 dark:border-slate-700",
                div { class: "flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        span { class: "text-lg", "🔔" }
                        div {
                            p { class: "text-sm font-medium text-slate-900 dark:text-slate-100", "Notificações do Sistema" }
                            p { class: "text-xs text-slate-600 dark:text-slate-400",
                                if notifications_enabled() { "Ativadas" } else { "Desativadas" }
                            }
                        }
                    }
                    button {
                        class: "px-3 py-2 rounded-lg font-medium text-sm transition-colors",
                        class: if notifications_enabled() {
                            "bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300 hover:bg-green-200 dark:hover:bg-green-900/50"
                        } else {
                            "bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600"
                        },
                        onclick: move |_| notifications_enabled.set(!notifications_enabled()),
                        if notifications_enabled() { "✅ Ativo" } else { "❌ Inativo" }
                    }
                }
                div { class: "mt-3 text-xs text-slate-600 dark:text-slate-400 space-y-1",
                    p { "📨 Você receberá notificações para:" }
                    ul { class: "list-disc list-inside ml-1",
                        li { "Início e parada do monitoramento" }
                        li { "Arquivos sincronizados com sucesso" }
                        li { "Backups criados e criptografados" }
                        li { "Erros durante o processo" }
                    }
                }
            }

            // Configuração de Webhook Notifications
            div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-4 mb-4 border border-slate-200 dark:border-slate-700",
                div { class: "flex items-center justify-between mb-4",
                    div { class: "flex items-center gap-3",
                        span { class: "text-lg", "🔗" }
                        div {
                            p { class: "text-sm font-medium text-slate-900 dark:text-slate-100", "Webhook Notifications" }
                            p { class: "text-xs text-slate-600 dark:text-slate-400",
                                if webhook_enabled() { "Ativado" } else { "Desativado" }
                            }
                        }
                    }
                    button {
                        class: "px-3 py-2 rounded-lg font-medium text-sm transition-colors",
                        class: if webhook_enabled() {
                            "bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 hover:bg-blue-200 dark:hover:bg-blue-900/50"
                        } else {
                            "bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600"
                        },
                        onclick: move |_| webhook_enabled.set(!webhook_enabled()),
                        if webhook_enabled() { "✅ Ativo" } else { "❌ Inativo" }
                    }
                }

                if webhook_enabled() {
                    div { class: "space-y-3 border-t border-slate-200 dark:border-slate-600 pt-4",
                        div { class: "form-group",
                            label { class: "label-text text-xs", "URL do Webhook (ex: https://webhook.site/abc123)" }
                            input {
                                class: "input-field",
                                r#type: "text",
                                placeholder: "https://seu-webhook.com/notify",
                                value: "{webhook_url()}",
                                oninput: move |evt| webhook_url.set(evt.value()),
                                disabled: is_monitoring()
                            }
                        }

                        div { class: "bg-blue-50 dark:bg-blue-900/20 rounded p-3 text-xs text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800 space-y-2",
                            p { class: "font-semibold", "📤 Estrutura do JSON enviado:" }
                            pre { class: "block font-mono text-xs overflow-auto bg-white dark:bg-slate-800 p-2 rounded border border-blue-200 dark:border-blue-700 text-left",
    r#"{{"#
    r#"  "event_type": "sync_complete","#
    r#"  "title": "Arquivos Sincronizados","#
    r#"  "message": "5 arquivo(s) sincronizado(s) com sucesso","#
    r#"  "timestamp": "2026-02-07 14:30:45","#
    r#"  "status": "success""#
    r#"}}"#
                            }
                        }
                    }
                }
            }

            // Configuração de padrões de ignore
            div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-4 mb-4 border border-slate-200 dark:border-slate-700",
                div { class: "flex items-center justify-between mb-4",
                    div { class: "flex items-center gap-3",
                        span { class: "text-lg", "🚫" }
                        div {
                            p { class: "text-sm font-medium text-slate-900 dark:text-slate-100", "Padrões de Ignore" }
                            p { class: "text-xs text-slate-600 dark:text-slate-400",
                                "{ignore_patterns().len()} padrões configurados"
                            }
                        }
                    }
                    button {
                        class: "px-3 py-2 rounded-lg font-medium text-sm transition-colors bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600",
                        onclick: move |_| show_ignore_editor.set(!show_ignore_editor()),
                        if show_ignore_editor() { "🔼 Fechar" } else { "⚙️ Editar" }
                    }
                }

                // Editor de padrões (colapsável)
                if show_ignore_editor() {
                    div { class: "space-y-3 border-t border-slate-200 dark:border-slate-600 pt-4",
                        // Adicionar novo padrão
                        div { class: "flex gap-2",
                            input {
                                class: "input-field flex-1",
                                placeholder: "Ex: .*\\.temp$ ou node_modules",
                                value: "{new_pattern()}",
                                oninput: move |evt| new_pattern.set(evt.value()),
                                disabled: is_monitoring()
                            }
                            button {
                                class: "btn-icon",
                                onclick: handle_add_pattern,
                                disabled: is_monitoring(),
                                "➕"
                            }
                        }

                        // Testar padrão
                        div { class: "bg-white dark:bg-slate-800 rounded p-3 border border-slate-200 dark:border-slate-600",
                            p { class: "text-xs font-medium text-slate-700 dark:text-slate-300 mb-2", "🧪 Testar Padrão" }
                            div { class: "flex gap-2 mb-2",
                                input {
                                    class: "input-field flex-1 text-sm",
                                    placeholder: "Digite um caminho (ex: /home/user/.tmp/file.txt)",
                                    value: "{pattern_test_input()}",
                                    oninput: move |evt| pattern_test_input.set(evt.value()),
                                }
                                button {
                                    class: "px-3 py-1 rounded text-sm font-medium bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 hover:bg-blue-200 dark:hover:bg-blue-900/50",
                                    onclick: handle_test_pattern,
                                    "Testar"
                                }
                            }
                            if !pattern_test_result().is_empty() {
                                p { class: "text-xs text-slate-700 dark:text-slate-300", "{pattern_test_result}" }
                            }
                        }

                        // Lista de padrões
                        div { class: "space-y-1",
                            for pattern in ignore_patterns() {
                                div { class: "flex items-center justify-between bg-white dark:bg-slate-800 p-2 rounded border border-slate-200 dark:border-slate-600",
                                    code { class: "text-xs font-mono text-slate-700 dark:text-slate-300 flex-1", "{pattern}" }
                                    button {
                                        class: "px-2 py-1 rounded text-xs font-medium bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300 hover:bg-red-200 dark:hover:bg-red-900/50",
                                        onclick: move |_| {
                                            let mut patterns = ignore_patterns();
                                            patterns.retain(|p| p != &pattern);
                                            ignore_patterns.set(patterns);
                                            status_msg.set(format!("✅ Padrão '{}' removido", pattern));
                                        },
                                        disabled: is_monitoring(),
                                        "✕"
                                    }
                                }
                            }
                        }

                        // Botão restaurar padrões
                        button {
                            class: "w-full px-3 py-2 rounded text-sm font-medium bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600",
                            onclick: handle_reset_patterns,
                            disabled: is_monitoring(),
                            "🔄 Restaurar Padrões"
                        }
                    }
                } else {
                    // Pré-visualização dos padrões (quando colapsado)
                    div { class: "flex flex-wrap gap-2",
                        for pattern in ignore_patterns().iter().take(5) {
                            div { class: "px-2 py-1 rounded text-xs bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-300",
                                code { "{pattern}" }
                            }
                        }
                        if ignore_patterns().len() > 5 {
                            div { class: "px-2 py-1 rounded text-xs bg-slate-200 dark:bg-slate-700 text-slate-600 dark:text-slate-400",
                                "+{ignore_patterns().len() - 5}"
                            }
                        }
                    }
                }
            }

            div { class: "flex gap-3 mb-4",
                button {
                    class: "btn-primary flex-1",
                    onclick: handle_start_monitoring,
                    disabled: is_monitoring() || source_path().as_os_str().is_empty(),
                    if is_monitoring() { "🔴 Monitorando..." } else { "▶️ Iniciar" }
                }
                button {
                    class: "btn-secondary flex-1",
                    onclick: handle_stop_monitoring,
                    disabled: !is_monitoring(),
                    "⏹️ Parar"
                }
            }

            // Painel de estatísticas
            div { class: "grid grid-cols-4 gap-3 mb-4",
                div { class: "bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg p-4",
                    p { class: "text-xs text-blue-600 dark:text-blue-400 font-medium", "Ficheiros Sincronizados" }
                    p { class: "text-2xl font-bold text-blue-900 dark:text-blue-300", "{files_synced}" }
                }
                div { class: "bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg p-4",
                    p { class: "text-xs text-green-600 dark:text-green-400 font-medium", "Mudanças Detectadas" }
                    p { class: "text-2xl font-bold text-green-900 dark:text-green-300", "{files_changed}" }
                }
                div { class: "bg-purple-50 dark:bg-purple-900/20 border border-purple-200 dark:border-purple-800 rounded-lg p-4",
                    p { class: "text-xs text-purple-600 dark:text-purple-400 font-medium", "Backups Criados" }
                    p { class: "text-2xl font-bold text-purple-900 dark:text-purple-300", "{backups_created}" }
                }
                div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-4",
                    p { class: "text-xs text-red-600 dark:text-red-400 font-medium", "Erros" }
                    p { class: "text-2xl font-bold text-red-900 dark:text-red-300", "{sync_errors}" }
                }
            }

            // Dashboard visual com gráficos
            if is_monitoring() {
                div { class: "card-section mb-4",
                    h3 { class: "section-title", "📊 Dashboard em Tempo Real" }

                    // Taxa de sucesso visual
                    div { class: "mb-6",
                        div { class: "flex justify-between items-center mb-2",
                            span { class: "text-sm font-medium text-slate-700 dark:text-slate-300", "Taxa de Sucesso" }
                            span { class: "text-sm font-bold text-slate-900 dark:text-slate-100", "{(success_rate() as i32)}%" }
                        }
                        div { class: "w-full bg-slate-200 dark:bg-slate-700 rounded-full h-3 overflow-hidden",
                            div {
                                class: "h-full transition-all duration-500",
                                class: if success_rate() >= 90.0 { "bg-green-500" } else if success_rate() >= 70.0 { "bg-yellow-500" } else { "bg-red-500" },
                                style: "width: {success_rate()}%"
                            }
                        }
                    }

                    // Último sync
                    div { class: "grid grid-cols-2 gap-4 mb-6",
                        div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-4 border border-slate-200 dark:border-slate-700",
                            p { class: "text-xs text-slate-600 dark:text-slate-400 font-medium mb-1", "⏱️ Último Sync" }
                            p { class: "text-lg font-bold text-slate-900 dark:text-slate-100", "{last_sync_time}" }
                        }
                        div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-4 border border-slate-200 dark:border-slate-700",
                            p { class: "text-xs text-slate-600 dark:text-slate-400 font-medium mb-1", "📈 Taxa" }
                            {
                                let total = backups_created() + sync_errors();
                                if total > 0 {
                                    rsx! {
                                        p { class: "text-lg font-bold text-slate-900 dark:text-slate-100",
                                            "{backups_created()}/{total} backups OK"
                                        }
                                    }
                                } else {
                                    rsx! {
                                        p { class: "text-lg font-bold text-slate-900 dark:text-slate-100",
                                            "Aguardando eventos"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Histórico de eventos
                    if !sync_history().is_empty() {
                        div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-4 border border-slate-200 dark:border-slate-700",
                            p { class: "text-sm font-semibold text-slate-900 dark:text-slate-100 mb-3", "📜 Histórico (Últimos 20 eventos)" }
                            div { class: "max-h-48 overflow-y-auto space-y-2",
                                    {
                                        let events: Vec<_> = sync_history().iter().rev().cloned().collect();
                                        events.into_iter().map(|event| {
                                            let (bg_color, icon, text_color) = match event.event_type.as_str() {
                                                "file_synced" => ("bg-blue-50 dark:bg-blue-900/30", "📄", "text-blue-900 dark:text-blue-200"),
                                                "backup_created" => ("bg-green-50 dark:bg-green-900/30", "✅", "text-green-900 dark:text-green-200"),
                                                "error" => ("bg-red-50 dark:bg-red-900/30", "❌", "text-red-900 dark:text-red-200"),
                                                _ => ("bg-slate-100 dark:bg-slate-800", "🔔", "text-slate-900 dark:text-slate-200"),
                                            };

                                            rsx! {
                                                div { key: "{event.timestamp}-{event.event_type}", class: "flex items-start gap-3 p-2 {bg_color} rounded border border-slate-200 dark:border-slate-700",
                                                    span { class: "text-lg flex-shrink-0", "{icon}" }
                                                    div { class: "flex-1 min-w-0",
                                                        div { class: "flex justify-between items-start gap-2",
                                                            p { class: "text-xs font-medium text-slate-600 dark:text-slate-400", "{event.timestamp}" }
                                                            span { class: "text-xs px-2 py-0.5 bg-slate-200 dark:bg-slate-700 rounded whitespace-nowrap {text_color}",
                                                                "{event.event_type}"
                                                            }
                                                        }
                                                        p { class: "text-sm text-slate-700 dark:text-slate-300 mt-1", "{event.details}" }
                                                    }
                                                }
                                            }
                                        })
                                    }
                            }
                        }
                    }
                }
            }

            // Informações
            div { class: "card-section",
                h3 { class: "section-title", "ℹ️ Como Funciona" }
                div { class: "space-y-2 text-sm text-slate-700 dark:text-slate-300",
                    p { "🔍 Monitora pasta em tempo real (a cada 2 segundos)" }
                    p { "✅ Sincroniza ficheiros novos/modificados para destino" }
                    p { "💾 Cria backup automático (.tar.gz) a cada mudança detectada" }
                    p { "📊 Mostra estatísticas: ficheiros, mudanças, backups" }
                    p { "🔴 Deixe aberto para funcionar continuamente" }
                }
            }


            // AlertDialog Modal (Notificação Visual)
            if show_alert() {
                div { class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                    div { class: "bg-white dark:bg-slate-900 rounded-lg shadow-xl max-w-md w-full mx-4 border border-slate-200 dark:border-slate-700",
                        // Header
                        div { class: "flex items-center gap-3 p-4 border-b border-slate-200 dark:border-slate-700",
                            {
                                let icon = match alert_type().as_str() {
                                    "success" => "✅",
                                    "error" => "❌",
                                    "warning" => "⚠️",
                                    _ => "ℹ️",
                                };
                                rsx! { span { class: "text-2xl", "{icon}" } }
                            }
                            h2 { class: "text-lg font-bold text-slate-900 dark:text-slate-100", "{alert_title}" }
                        }

                        // Body
                        div { class: "p-4",
                            p { class: "text-slate-700 dark:text-slate-300 text-sm leading-relaxed", "{alert_message}" }
                        }

                        // Footer
                        div { class: "flex gap-2 p-4 border-t border-slate-200 dark:border-slate-700",
                            button {
                                class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600",
                                onclick: move |_| show_alert.set(false),
                                "Fechar"
                            }
                        }
                    }
                }
            }
        }
    }
}
