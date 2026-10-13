//! FIDO2 keys of this computer (`rsb auth list` / `generate-codes`).
//! This tab used to be a placeholder.

use crate::ui::app::AppConfig;
use crate::ui::i18n::{Translations, get_texts, tr};
use dioxus::prelude::*;
use rsb_sdk::credentials::Fido2Manager;
use std::sync::Arc;
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

#[component]
pub fn Fido2ManagerView() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let manager = use_context::<Arc<Mutex<Fido2Manager>>>();
    let user = use_context::<Signal<Option<String>>>();
    let mut rows = use_signal(Vec::<CredentialRow>::new);
    let mut codes = use_signal(Vec::<String>::new);
    let mut status = use_signal(String::new);

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
