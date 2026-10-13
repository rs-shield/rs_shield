//! FIDO2 keys of this computer (`rsb auth list / register / delete /
//! generate-codes`), through the same SDK calls as the CLI.

use crate::ui::app::AppConfig;
use crate::ui::i18n::{Translations, get_texts, tr};
use axum::response::Html;
use dioxus::prelude::*;
use rsb_sdk::CancellationToken;
use rsb_sdk::credentials::Fido2Manager;
use rsb_sdk::credentials::fido2_manager::Fido2Error;
use rsb_sdk::fido2::fido2_web::{self, ServerMode};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// One row of the list.
#[derive(Debug, Clone, PartialEq)]
pub struct CredentialRow {
    pub user_id: String,
    pub display_name: String,
    pub created_at: String,
    pub last_used: String,
}

/// Rows of the registered credentials, sorted by user.
pub fn credential_rows(t: &Translations, mgr: &Fido2Manager) -> Vec<CredentialRow> {
    let mut rows: Vec<CredentialRow> = mgr
        .list_credentials()
        .into_iter()
        .map(|c| CredentialRow {
            user_id: c.user_id,
            display_name: c.display_name,
            created_at: c.created_at,
            last_used: c.last_used.unwrap_or_else(|| t.fido_never.to_string()),
        })
        .collect();
    rows.sort_by(|a, b| a.user_id.cmp(&b.user_id));
    rows
}

/// Replaces the recovery codes of `user` and saves the credentials (like
/// `rsb auth generate-codes`): reloaded from disk first so nothing newer
/// is overwritten, and refused for unknown users.
pub fn regenerate_codes(
    t: &Translations,
    mgr: &mut Fido2Manager,
    path: &std::path::Path,
    user: &str,
) -> Result<Vec<String>, String> {
    mgr.load_from_file(path)
        .map_err(|e| tr(t.fido_load_failed, &[&e]))?;
    if !mgr.has_credential(user) {
        return Err(tr(t.fido_unknown_user, &[&user]));
    }
    let codes = mgr
        .generate_backup_codes(user)
        .map_err(|e| tr(t.fido_generate_failed, &[&e]))?;
    mgr.save_to_file(path)
        .map_err(|e| tr(t.fido_save_failed, &[&e]))?;
    Ok(codes)
}

/// Deletes every key of the signed-in user `me` (like `rsb auth delete`,
/// which only lets a session delete its own user), after the user typed
/// the identifier again to confirm.
pub fn delete_own_keys(
    mgr: &mut Fido2Manager,
    path: &std::path::Path,
    me: &str,
    typed: &str,
    t: &Translations,
) -> Result<String, String> {
    if me.is_empty() || typed.trim() != me {
        return Err(t.kl_fido2_delete_mismatch.to_string());
    }
    mgr.revoke_user_persisted(path, me).map_err(|e| match e {
        Fido2Error::CredentialNotFound => tr(t.kl_fido2_delete_unknown, &[&me]),
        e => e.to_string(),
    })?;
    Ok(tr(t.kl_fido2_delete_done, &[&me]))
}

/// Resolves once `token` is cancelled.
async fn cancelled(token: CancellationToken) {
    while !token.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Registers a key in the browser (like `rsb auth register`): the same
/// local FIDO2 server, in registration mode. `Ok(false)` when cancelled
/// (the server future is dropped, which frees its port).
pub async fn register_key(
    manager: Arc<Mutex<Fido2Manager>>,
    token: CancellationToken,
) -> Result<bool, String> {
    let html = include_str!("../../../rsb-cli/src/assets/fido2_auth.html");
    tokio::select! {
        // Cancellation first: a cancelled token never starts the server.
        biased;
        _ = cancelled(token) => Ok(false),
        res = fido2_web::run_server_with_mode(manager, Html(html), ServerMode::Register) => {
            res.map(|_| true).map_err(|e| e.to_string())
        }
    }
}

#[component]
pub fn Fido2ManagerView() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let manager = use_context::<Arc<Mutex<Fido2Manager>>>();
    let app_config = use_context::<AppConfig>();
    let t = get_texts(app_config.language());
    let mut user = use_context::<Signal<Option<String>>>();
    let mut rows = use_signal(Vec::<CredentialRow>::new);
    let mut codes = use_signal(Vec::<String>::new);
    let mut status = use_signal(String::new);
    let mut registering = use_signal(|| false);
    let mut register_token = use_signal(CancellationToken::new);
    let mut confirm_delete = use_signal(|| false);
    let mut typed_id = use_signal(String::new);

    let refresh_mgr = manager.clone();
    let refresh = use_callback(move |_: ()| {
        let mgr = refresh_mgr.clone();
        spawn(async move {
            let mut m = mgr.lock().await;
            if let Ok(path) = Fido2Manager::default_storage_path()
                && path.exists()
                && let Err(e) = m.load_from_file(&path)
            {
                status.set(tr(texts.fido_could_not_load, &[&e]));
            }
            rows.set(credential_rows(texts, &m));
        });
    });
    use_hook(move || refresh.call(()));

    let codes_mgr = manager.clone();
    let handle_codes = move |_| {
        let Some(me) = user() else {
            return;
        };
        let mgr = codes_mgr.clone();
        spawn(async move {
            let mut m = mgr.lock().await;
            let res = Fido2Manager::default_storage_path()
                .and_then(|path| regenerate_codes(texts, &mut m, &path, &me));
            match res {
                Ok(c) => {
                    codes.set(c);
                    status.set(texts.fido_codes_generated.to_string());
                }
                Err(e) => status.set(format!("❌ {}", e)),
            }
        });
    };

    let register_mgr = manager.clone();
    let handle_register = move |_| {
        let mgr = register_mgr.clone();
        let token = CancellationToken::new();
        register_token.set(token.clone());
        registering.set(true);
        status.set(t.kl_fido2_register_waiting.to_string());
        spawn(async move {
            let res = register_key(mgr, token).await;
            registering.set(false);
            status.set(match res {
                Ok(true) => t.kl_fido2_register_done.to_string(),
                Ok(false) => t.kl_fido2_register_cancelled.to_string(),
                Err(e) => format!("❌ {}", e),
            });
            refresh.call(());
        });
    };

    let delete_mgr = manager.clone();
    let handle_delete = move |_| {
        let Some(me) = user() else {
            return;
        };
        let typed = typed_id();
        let mgr = delete_mgr.clone();
        spawn(async move {
            let res = {
                let mut m = mgr.lock().await;
                Fido2Manager::default_storage_path()
                    .and_then(|path| delete_own_keys(&mut m, &path, &me, &typed, t))
            };
            match res {
                Ok(msg) => {
                    confirm_delete.set(false);
                    typed_id.set(String::new());
                    status.set(msg);
                    codes.set(Vec::new());
                    refresh.call(());
                    // The session's user has no key any more: sign out.
                    user.set(None);
                }
                Err(e) => status.set(format!("❌ {}", e)),
            }
        });
    };

    let me = user().unwrap_or_default();

    rsx! {
        div { class: "card",
            h2 { class: "page-title", "{texts.fido_title}" }
            p { class: "hint mb-4",
                "{texts.fido_hint}"
            }

            if rows().is_empty() {
                p { class: "text-sm", "{texts.fido_none}" }
            } else {
                table { class: "w-full text-sm",
                    thead {
                        tr {
                            th { class: "text-left", "{texts.fido_col_user}" }
                            th { class: "text-left", "{texts.fido_col_name}" }
                            th { class: "text-left", "{texts.fido_col_registered}" }
                            th { class: "text-left", "{texts.fido_col_last_used}" }
                        }
                    }
                    tbody {
                        for row in rows() {
                            tr { key: "{row.user_id}-{row.created_at}",
                                td { "{row.user_id}" }
                                td { "{row.display_name}" }
                                td { "{row.created_at}" }
                                td { "{row.last_used}" }
                            }
                        }
                    }
                }
            }

            div { class: "flex gap-3 mt-4",
                button { class: "btn-secondary flex-1", onclick: move |_| refresh.call(()), "{texts.snapshots_refresh}" }
                button {
                    class: "btn-warning flex-1",
                    disabled: me.is_empty(),
                    onclick: handle_codes,
                    {tr(texts.fido_new_codes_for, &[&me])}
                }
            }

            div { class: "flex gap-3 mt-3",
                if registering() {
                    button {
                        class: "btn-secondary flex-1",
                        onclick: move |_| register_token().cancel(),
                        "{t.kl_cancel_button}"
                    }
                } else {
                    button {
                        class: "btn-primary flex-1",
                        onclick: handle_register,
                        "{t.kl_fido2_register_button}"
                    }
                }
                button {
                    class: "flex-1 px-4 py-2 bg-red-500 dark:bg-red-600 hover:bg-red-600 dark:hover:bg-red-700 text-white font-semibold rounded-lg transition-colors",
                    disabled: me.is_empty() || registering(),
                    onclick: move |_| {
                        typed_id.set(String::new());
                        confirm_delete.toggle();
                    },
                    "{t.kl_fido2_delete_button}"
                }
            }
            p { class: "hint mt-2", "{t.kl_fido2_register_hint}" }

            if confirm_delete() && !me.is_empty() {
                div { class: "status-box mt-4 border-l-4 border-red-500",
                    p { class: "font-semibold mb-2", "{t.kl_confirm_title}" }
                    p { class: "whitespace-pre-wrap mb-2", {tr(t.kl_fido2_delete_confirm, &[&me])} }
                    input {
                        class: "input-field mb-2",
                        r#type: "text",
                        placeholder: "{me}",
                        value: "{typed_id}",
                        oninput: move |evt| typed_id.set(evt.value())
                    }
                    div { class: "flex gap-3",
                        button {
                            class: "flex-1 px-4 py-2 bg-red-500 dark:bg-red-600 hover:bg-red-600 dark:hover:bg-red-700 text-white font-semibold rounded-lg transition-colors",
                            disabled: typed_id().trim() != me,
                            onclick: handle_delete,
                            "{t.kl_fido2_delete_action}"
                        }
                        button {
                            class: "flex-1 btn-secondary",
                            onclick: move |_| confirm_delete.set(false),
                            "{t.kl_cancel_button}"
                        }
                    }
                }
            }

            if !status().is_empty() {
                div { class: "status-box mt-4", p { class: "whitespace-pre-wrap", "{status}" } }
            }
            if !codes().is_empty() {
                div { class: "status-box mt-2 font-mono",
                    for c in codes() {
                        p { "{c}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_needs_the_typed_identifier_and_a_known_user() {
        use crate::ui::i18n::EN_STRINGS;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fido2.json");
        let mut mgr = Fido2Manager::new("http://localhost:3000", "localhost", "test").unwrap();
        // Typed identifier does not match: refused before anything is read.
        assert_eq!(
            delete_own_keys(&mut mgr, &path, "alice", "alic", &EN_STRINGS).unwrap_err(),
            EN_STRINGS.kl_fido2_delete_mismatch
        );
        assert!(delete_own_keys(&mut mgr, &path, "", "", &EN_STRINGS).is_err());
        // No credentials file and no such user: refused, nothing written
        // (and the OS keyring is never touched).
        let e = delete_own_keys(&mut mgr, &path, "alice", "alice", &EN_STRINGS).unwrap_err();
        assert!(e.contains("Unknown user: alice"), "{e}");
        assert!(!path.exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancelled_registration_never_starts_the_server() {
        // The cancellation branch is polled first, so the server (which
        // would read the real credentials file) is never polled.
        let mgr = Arc::new(Mutex::new(
            Fido2Manager::new("http://localhost:3000", "localhost", "test").unwrap(),
        ));
        let token = CancellationToken::new();
        token.cancel();
        assert_eq!(register_key(mgr, token).await, Ok(false));
    }

    #[test]
    fn unknown_user_gets_no_codes_and_nothing_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fido2.json");
        let mut mgr = Fido2Manager::new("http://localhost:3000", "localhost", "test").unwrap();
        let t = get_texts(crate::ui::i18n::Language::En);
        assert!(credential_rows(t, &mgr).is_empty());
        // No credentials registered: refused, and nothing is written.
        assert!(regenerate_codes(t, &mut mgr, &path, "alice").is_err());
        assert!(!path.exists());
    }
}
