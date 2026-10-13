//! Snapshots tab: list, inspect, compare and delete the snapshots of a
//! repository (a profile's destination or S3 bucket, a repository folder,
//! or the S3 bucket configured in the app).
//!
//! All repository work goes through [`rsb_sdk::snapshot`], the same code as
//! `rsb snapshots ...`; this file only picks the repository and renders.
//! The functions above the components are UI-free and unit-tested.

use crate::ui::s3_settings::{S3Settings, apply_session_credentials};
use crate::ui::{
    app::AppConfig,
    i18n::get_texts,
    loading_state::{LoadingOverlay, LoadingStyle},
};
use dioxus::prelude::*;
use rsb_sdk::config::Config;
use rsb_sdk::core::storage_backend::get_storage;
use rsb_sdk::snapshot::{
    self, SnapshotContents, SnapshotDeletion, SnapshotDetails, SnapshotDiff, SnapshotInfo,
    format_bytes, format_timestamp,
};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use zeroize::Zeroizing;

/// Where the snapshots are read from.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RepoSelection {
    /// A profile: its S3 bucket when it has one, else its destination.
    Profile(PathBuf),
    /// A local repository folder (portable mode).
    Directory(PathBuf),
    /// The S3 bucket configured in the app (Settings / S3 fields).
    S3(S3Settings),
}

impl RepoSelection {
    /// Profile first, then the folder, then the app's S3 bucket.
    pub(crate) fn choose(
        profile: &Path,
        directory: &Path,
        s3: &S3Settings,
    ) -> Result<Self, String> {
        if !profile.as_os_str().is_empty() {
            Ok(Self::Profile(profile.to_path_buf()))
        } else if !directory.as_os_str().is_empty() {
            Ok(Self::Directory(directory.to_path_buf()))
        } else if s3.is_enabled() {
            Ok(Self::S3(s3.clone()))
        } else {
            Err("Choose a profile, a backup folder, or configure an S3 bucket.".to_string())
        }
    }

    /// Short description for the screen.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Profile(p) => format!("profile {}", p.display()),
            Self::Directory(d) => d.display().to_string(),
            Self::S3(s) => format!("s3://{}", s.bucket.as_deref().unwrap_or_default()),
        }
    }

    /// Repository config: like `rsb snapshots` (profile → its storage,
    /// folder → that folder, bucket → that bucket).
    pub(crate) fn config(&self) -> Result<Config, String> {
        match self {
            Self::Profile(p) => rsb_sdk::config::load_config(p)
                .map_err(|e| format!("Cannot load profile {}: {}", p.display(), e)),
            Self::Directory(d) => rsb_sdk::config_from_backup_path(d, None)
                .map_err(|e| format!("Not a backup repository ({}): {}", d.display(), e)),
            Self::S3(s) => {
                let mut cfg = rsb_sdk::portable_restore::config_for_s3_restore(
                    s.bucket.as_deref().unwrap_or_default(),
                    s.region.clone(),
                    s.endpoint.clone(),
                )
                .map_err(|e| e.to_string())?;
                s.apply_to(&mut cfg);
                Ok(cfg)
            }
        }
    }

    /// The key typed on the screen, else (for a profile) the one
    /// `rsb` would use: RSB_ENCRYPTION_KEY, the profile's keyring entry,
    /// a legacy key in the profile.
    fn key(&self, typed: &str, cfg: &Config) -> Option<String> {
        if !typed.is_empty() {
            return Some(typed.to_string());
        }
        let profile = match self {
            Self::Profile(p) => p.file_stem().and_then(|s| s.to_str()),
            _ => None,
        };
        rsb_sdk::config::resolve_encryption_key(None, profile, Some(cfg))
    }
}

/// Opens the repository and the key, then runs `op`.
async fn with_repository<T, F, Fut>(
    sel: RepoSelection,
    typed_key: String,
    op: F,
) -> Result<T, String>
where
    F: FnOnce(std::sync::Arc<dyn rsb_sdk::storage::Storage>, Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<T, rsb_sdk::RsbError>>,
{
    let cfg = sel.config()?;
    let key = sel.key(&typed_key, &cfg);
    let storage = get_storage(&cfg).await.map_err(|e| e.to_string())?;
    op(storage, key).await.map_err(|e| e.to_string())
}

/// Snapshots of the repository, newest first.
pub(crate) async fn load_snapshots(
    sel: RepoSelection,
    typed_key: String,
) -> Result<Vec<SnapshotInfo>, String> {
    with_repository(sel, typed_key, |storage, key| async move {
        let cache = snapshot::open_key_cache(&*storage, key.as_deref()).await?;
        snapshot::list_snapshots(&*storage, cache.as_ref()).await
    })
    .await
}

/// Details (file list) of snapshot `id`.
pub(crate) async fn load_details(
    sel: RepoSelection,
    typed_key: String,
    id: String,
) -> Result<SnapshotDetails, String> {
    with_repository(sel, typed_key, |storage, key| async move {
        let cache = snapshot::open_key_cache(&*storage, key.as_deref()).await?;
        snapshot::snapshot_details(&*storage, &id, cache.as_ref()).await
    })
    .await
}

/// Differences from snapshot `from` to snapshot `to`.
pub(crate) async fn compare(
    sel: RepoSelection,
    typed_key: String,
    from: String,
    to: String,
) -> Result<SnapshotDiff, String> {
    with_repository(sel, typed_key, |storage, key| async move {
        let cache = snapshot::open_key_cache(&*storage, key.as_deref()).await?;
        snapshot::diff_snapshots(&*storage, &from, &to, cache.as_ref()).await
    })
    .await
}

/// Deletes snapshot `id` (and the data only it references).
pub(crate) async fn delete(
    sel: RepoSelection,
    typed_key: String,
    id: String,
) -> Result<SnapshotDeletion, String> {
    with_repository(sel, typed_key, |storage, key| async move {
        snapshot::delete_snapshot(storage, &id, key.as_deref()).await
    })
    .await
}

/// (files, size, note) shown for one snapshot of the list.
pub(crate) fn summary_cells(info: &SnapshotInfo) -> (String, String, String) {
    let lock = if info.encrypted { "🔒 encrypted" } else { "" };
    match &info.contents {
        SnapshotContents::Readable(s) => (
            s.file_count.to_string(),
            format_bytes(s.display_size()),
            lock.to_string(),
        ),
        SnapshotContents::Locked => (
            "?".into(),
            "?".into(),
            "🔒 encrypted: enter the key to see its contents".into(),
        ),
        SnapshotContents::Unreadable { error } => (
            "?".into(),
            "?".into(),
            format!(
                "⚠️ unreadable: {}",
                error
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .trim()
            ),
        ),
    }
}

/// Plain-text rendering of a diff for the Compare view.
pub(crate) fn diff_text(d: &SnapshotDiff) -> String {
    if d.is_empty() {
        return format!("No changes between {} and {}.", d.from, d.to);
    }
    let mut out = format!("{} → {}\n", d.from, d.to);
    if !d.added.is_empty() {
        out.push_str(&format!("\n✅ Added ({}):\n", d.added.len()));
        for f in &d.added {
            out.push_str(&format!(
                "  + {} ({})\n",
                f.path,
                format_bytes(f.display_size())
            ));
        }
    }
    if !d.removed.is_empty() {
        out.push_str(&format!("\n❌ Removed ({}):\n", d.removed.len()));
        for f in &d.removed {
            out.push_str(&format!(
                "  - {} ({})\n",
                f.path,
                format_bytes(f.display_size())
            ));
        }
    }
    if !d.modified.is_empty() {
        out.push_str(&format!("\n🔄 Modified ({}):\n", d.modified.len()));
        for (old, new) in &d.modified {
            out.push_str(&format!(
                "  ~ {} ({} → {})\n",
                new.path,
                format_bytes(old.display_size()),
                format_bytes(new.display_size())
            ));
        }
    }
    out
}

/// Message shown after a delete.
pub(crate) fn deletion_message(d: &SnapshotDeletion) -> String {
    if d.gc_ran {
        format!(
            "✅ Snapshot {} deleted ({} data object(s) no other snapshot used were removed).",
            d.id, d.objects_deleted
        )
    } else {
        format!(
            "✅ Snapshot {} deleted. It was the last snapshot: its data was kept.",
            d.id
        )
    }
}

/// Repository chosen on this screen, kept while the app runs so that
/// coming back to the tab (e.g. after a backup or a prune) reloads it.
/// Memory only; the key is zeroized when replaced.
struct Remembered {
    profile: PathBuf,
    directory: PathBuf,
    key: Zeroizing<String>,
}

static REMEMBERED: Mutex<Option<Remembered>> = Mutex::new(None);

fn remember(profile: PathBuf, directory: PathBuf, key: String) {
    if let Ok(mut slot) = REMEMBERED.lock() {
        *slot = Some(Remembered {
            profile,
            directory,
            key: Zeroizing::new(key),
        });
    }
}

fn remembered() -> (PathBuf, PathBuf, String) {
    REMEMBERED
        .lock()
        .ok()
        .and_then(|slot| {
            slot.as_ref()
                .map(|r| (r.profile.clone(), r.directory.clone(), r.key.to_string()))
        })
        .unwrap_or_default()
}

#[derive(Clone, Copy, PartialEq)]
enum SnapshotMode {
    List,
    Diff,
    Details,
}

#[component]
pub fn SnapshotsScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    let (init_profile, init_dir, init_key) = remembered();
    let mut profile_path = use_signal(|| init_profile);
    let mut repo_path = use_signal(|| init_dir);
    let mut key = use_signal(|| init_key);

    let mut snapshots = use_signal(Vec::<SnapshotInfo>::new);
    let mut source_label = use_signal(|| Option::<String>::None);
    let mut is_loading = use_signal(|| false);
    let mut mode = use_signal(|| SnapshotMode::List);
    let mut selected_from = use_signal(|| Option::<String>::None);
    let mut selected_to = use_signal(|| Option::<String>::None);
    let mut diff_result = use_signal(|| Option::<String>::None);
    let mut details = use_signal(|| Option::<Result<SnapshotDetails, String>>::None);
    let mut pending_delete = use_signal(|| Option::<String>::None);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut info_msg = use_signal(|| Option::<String>::None);

    // Current selection (also applies the S3 credentials typed in the app
    // as session credentials, like the other screens).
    let selection = move || -> Result<RepoSelection, String> {
        let s3 = S3Settings::from_app(&app_config);
        apply_session_credentials(&app_config);
        RepoSelection::choose(&profile_path.read(), &repo_path.read(), &s3)
    };

    let mut reload = move |announce: Option<String>| {
        if is_loading() {
            return;
        }
        error_msg.set(None);
        info_msg.set(announce);
        let sel = match selection() {
            Ok(s) => s,
            Err(e) => {
                snapshots.set(Vec::new());
                source_label.set(None);
                error_msg.set(Some(e));
                return;
            }
        };
        remember(profile_path(), repo_path(), key());
        is_loading.set(true);
        let label = sel.label();
        let typed = key();
        spawn(async move {
            // Off the UI thread: S3 round trips and key derivation.
            let result = tokio::spawn(load_snapshots(sel, typed))
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            match result {
                Ok(list) => {
                    // Drop selections that no longer exist.
                    for mut sig in [selected_from, selected_to] {
                        let gone = sig
                            .read()
                            .as_ref()
                            .is_some_and(|id| !list.iter().any(|s| &s.id == id));
                        if gone {
                            sig.set(None);
                        }
                    }
                    snapshots.set(list);
                    source_label.set(Some(label));
                }
                Err(e) => {
                    snapshots.set(Vec::new());
                    source_label.set(None);
                    error_msg.set(Some(e));
                }
            }
            is_loading.set(false);
        });
    };

    // Load on open: a repository chosen earlier (this run) is reloaded, so
    // the list is current after a backup or prune in another tab.
    use_hook(move || {
        spawn(async move {
            let has_target = !profile_path.peek().as_os_str().is_empty()
                || !repo_path.peek().as_os_str().is_empty()
                || !app_config.s3_bucket.peek().trim().is_empty();
            if has_target {
                reload(None);
            }
        });
    });

    let pick_profile = move |_| {
        spawn(async move {
            if let Some(h) = rfd::AsyncFileDialog::new()
                .add_filter("TOML", &["toml"])
                .pick_file()
                .await
            {
                profile_path.set(h.path().to_path_buf());
                repo_path.set(PathBuf::new());
                reload(None);
            }
        });
    };

    let pick_folder = move |_| {
        spawn(async move {
            if let Some(h) = rfd::AsyncFileDialog::new().pick_folder().await {
                repo_path.set(h.path().to_path_buf());
                profile_path.set(PathBuf::new());
                reload(None);
            }
        });
    };

    let on_show = move |id: String| {
        let sel = match selection() {
            Ok(s) => s,
            Err(e) => {
                error_msg.set(Some(e));
                return;
            }
        };
        details.set(None);
        mode.set(SnapshotMode::Details);
        let typed = key();
        spawn(async move {
            let r = tokio::spawn(load_details(sel, typed, id))
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            details.set(Some(r));
        });
    };

    let on_compare = move |_| {
        let (Some(from), Some(to)) = (selected_from(), selected_to()) else {
            error_msg.set(Some(texts.snapshots_select_two.to_string()));
            return;
        };
        let sel = match selection() {
            Ok(s) => s,
            Err(e) => {
                error_msg.set(Some(e));
                return;
            }
        };
        error_msg.set(None);
        diff_result.set(Some(texts.snapshots_loading.to_string()));
        let typed = key();
        spawn(async move {
            let r = tokio::spawn(compare(sel, typed, from, to))
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            match r {
                Ok(d) => diff_result.set(Some(diff_text(&d))),
                Err(e) => {
                    diff_result.set(None);
                    error_msg.set(Some(e));
                }
            }
        });
    };

    let mut on_confirm_delete = move |id: String| {
        pending_delete.set(None);
        let sel = match selection() {
            Ok(s) => s,
            Err(e) => {
                error_msg.set(Some(e));
                return;
            }
        };
        error_msg.set(None);
        info_msg.set(Some(format!("Deleting snapshot {}…", id)));
        is_loading.set(true);
        let typed = key();
        spawn(async move {
            let r = tokio::spawn(delete(sel, typed, id))
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            is_loading.set(false);
            match r {
                Ok(done) => reload(Some(deletion_message(&done))),
                Err(e) => {
                    info_msg.set(None);
                    error_msg.set(Some(e));
                }
            }
        });
    };

    rsx! {
        div { class: "space-y-3",
            LoadingOverlay {
                is_visible: is_loading(),
                style: LoadingStyle::Spinner,
                message: texts.snapshots_loading.to_string(),
            }

            // Header
            div { class: "flex flex-col sm:flex-row sm:justify-between sm:items-center gap-3",
                h2 { class: "text-lg font-bold text-slate-900 dark:text-white", "📸 {texts.nav_snapshots}" }
                button {
                    class: "btn-secondary py-1 px-2 flex items-center gap-1 text-sm",
                    onclick: move |_| reload(None),
                    disabled: is_loading(),
                    if is_loading() { "{texts.snapshots_loading}" } else { "{texts.snapshots_refresh}" }
                }
            }

            // Repository
            div { class: "card space-y-2",
                div { class: "form-group",
                    label { class: "label-text", "📋 Profile" }
                    div { class: "flex gap-3",
                        input {
                            class: "input-field",
                            r#type: "text",
                            placeholder: "Select a profile (.toml)",
                            value: "{profile_path.read().to_string_lossy()}",
                            readonly: true
                        }
                        button { class: "btn-icon", onclick: pick_profile, disabled: is_loading(), "📂" }
                    }
                }
                div { class: "form-group",
                    label { class: "label-text", "📁 Or a backup folder" }
                    div { class: "flex gap-3",
                        input {
                            class: "input-field",
                            r#type: "text",
                            placeholder: "Backup folder (contains snapshots/ and data/)",
                            value: "{repo_path.read().to_string_lossy()}",
                            oninput: move |evt| {
                                repo_path.set(PathBuf::from(evt.value()));
                                profile_path.set(PathBuf::new());
                            }
                        }
                        button { class: "btn-icon", onclick: pick_folder, disabled: is_loading(), "📂" }
                    }
                    p { class: "hint", "With neither, the S3 bucket configured in the app is used." }
                }
                div { class: "form-group",
                    label { class: "label-text", "🔑 Encryption key (encrypted backups)" }
                    input {
                        class: "input-field",
                        r#type: "password",
                        placeholder: "Leave empty to use RSB_ENCRYPTION_KEY or the profile's saved key",
                        value: "{key}",
                        oninput: move |evt| key.set(evt.value()),
                    }
                }
                if let Some(label) = source_label.read().as_ref() {
                    p { class: "hint", "Repository: {label}" }
                }
            }

            if let Some(error) = error_msg.read().as_ref() {
                div { class: "p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/50 rounded-lg text-red-700 dark:text-red-300 text-sm whitespace-pre-wrap",
                    "{error}"
                }
            }
            if let Some(info) = info_msg.read().as_ref() {
                div { class: "p-3 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800/50 rounded-lg text-green-800 dark:text-green-200 text-sm",
                    "{info}"
                }
            }

            if let Some(id) = pending_delete.read().clone() {
                div { class: "p-3 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800/50 rounded-lg text-sm space-y-2",
                    p { class: "text-amber-900 dark:text-amber-100",
                        "Delete snapshot {id}? The data that only this snapshot uses is deleted too. This cannot be undone."
                    }
                    div { class: "flex gap-2",
                        button {
                            class: "btn-primary text-xs py-1 px-2",
                            onclick: move |_| on_confirm_delete(id.clone()),
                            "{texts.snapshots_delete_button}"
                        }
                        button {
                            class: "btn-secondary text-xs py-1 px-2",
                            onclick: move |_| pending_delete.set(None),
                            "Cancel"
                        }
                    }
                }
            }

            // Mode selector
            div { class: "flex flex-wrap gap-2 mb-3",
                button {
                    class: if *mode.read() == SnapshotMode::List { "btn-primary" } else { "btn-secondary" },
                    onclick: move |_| mode.set(SnapshotMode::List),
                    "{texts.snapshots_list_label}"
                }
                button {
                    class: if *mode.read() == SnapshotMode::Diff { "btn-primary" } else { "btn-secondary" },
                    onclick: move |_| mode.set(SnapshotMode::Diff),
                    "{texts.snapshots_compare_label}"
                }
            }

            match *mode.read() {
                SnapshotMode::List => rsx! {
                    SnapshotsListView {
                        snapshots: snapshots.read().clone(),
                        loaded: source_label.read().is_some(),
                        on_delete: move |id: String| pending_delete.set(Some(id)),
                        on_show: on_show,
                    }
                },
                SnapshotMode::Diff => rsx! {
                    SnapshotsDiffView {
                        snapshots: snapshots.read().clone(),
                        selected_from: selected_from.read().clone(),
                        selected_to: selected_to.read().clone(),
                        on_select_from: move |id: String| {
                            selected_from.set(Some(id).filter(|s| !s.is_empty()));
                            diff_result.set(None);
                        },
                        on_select_to: move |id: String| {
                            selected_to.set(Some(id).filter(|s| !s.is_empty()));
                            diff_result.set(None);
                        },
                        on_compare: on_compare,
                        diff_result: diff_result.read().clone(),
                    }
                },
                SnapshotMode::Details => rsx! {
                    SnapshotsDetailsView {
                        details: details.read().clone(),
                        on_back: move |_| mode.set(SnapshotMode::List),
                    }
                },
            }
        }
    }
}

#[component]
fn SnapshotsListView(
    snapshots: Vec<SnapshotInfo>,
    loaded: bool,
    on_delete: EventHandler<String>,
    on_show: EventHandler<String>,
) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    rsx! {
        div { class: "space-y-4",
            if snapshots.is_empty() {
                div { class: "text-center py-6 bg-slate-50 dark:bg-slate-800/50 rounded-lg border-2 border-dashed border-slate-200 dark:border-slate-700",
                    span { class: "text-xl mb-2 block", "📂" }
                    p { class: "text-slate-500 dark:text-slate-400",
                        if loaded { "{texts.snapshots_no_found}" } else { "Choose a repository to list its snapshots." }
                    }
                }
            } else {
                p { class: "text-xs text-slate-500 dark:text-slate-400", "{snapshots.len()} snapshot(s), newest first" }
                div { class: "grid gap-2",
                    for snapshot in snapshots {
                        {
                            let id_for_show = snapshot.id.clone();
                            let id_for_delete = snapshot.id.clone();
                            let (files, size, note) = summary_cells(&snapshot);
                            let readable = matches!(snapshot.contents, SnapshotContents::Readable(_));
                            let date = snapshot.date_label();
                            rsx! {
                                div { key: "{snapshot.id}", class: "p-2 bg-gradient-to-r from-slate-50 to-slate-100 dark:from-slate-800/50 dark:to-slate-800 border border-slate-200 dark:border-slate-700 rounded-lg hover:shadow-md transition-shadow",
                                    div { class: "flex items-start justify-between mb-2",
                                        div { class: "flex-1",
                                            h3 { class: "font-semibold text-slate-900 dark:text-white text-sm font-mono", "📸 {snapshot.id}" }
                                            p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1", "{texts.snapshots_created}: {date}" }
                                        }
                                        div { class: "flex gap-2",
                                            button {
                                                class: "btn-secondary text-xs py-1 px-2",
                                                disabled: !readable,
                                                onclick: move |_| on_show.call(id_for_show.clone()),
                                                "{texts.snapshots_view_button}"
                                            }
                                            button {
                                                class: "btn-secondary text-xs py-1 px-2",
                                                onclick: move |_| on_delete.call(id_for_delete.clone()),
                                                "{texts.snapshots_delete_button}"
                                            }
                                        }
                                    }
                                    div { class: "flex flex-wrap gap-3 text-xs",
                                        div { class: "flex items-center gap-1 text-slate-600 dark:text-slate-400",
                                            span { "📄" }
                                            span { "{files} {texts.snapshots_files}" }
                                        }
                                        div { class: "flex items-center gap-1 text-slate-600 dark:text-slate-400",
                                            span { "💾" }
                                            span { "{size}" }
                                        }
                                        if !note.is_empty() {
                                            div { class: "flex items-center gap-1 text-slate-600 dark:text-slate-400",
                                                span { "{note}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SnapshotsDiffView(
    snapshots: Vec<SnapshotInfo>,
    selected_from: Option<String>,
    selected_to: Option<String>,
    on_select_from: EventHandler<String>,
    on_select_to: EventHandler<String>,
    on_compare: EventHandler<()>,
    diff_result: Option<String>,
) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    rsx! {
        div { class: "space-y-4",
            div { class: "grid grid-cols-2 gap-3",
                div { class: "space-y-2",
                    label { class: "block text-sm font-semibold text-slate-700 dark:text-slate-300", "{texts.snapshots_from}" }
                    select {
                        class: "w-full px-3 py-2 border border-slate-300 dark:border-slate-600 rounded-lg bg-white dark:bg-slate-800 text-slate-900 dark:text-white",
                        onchange: move |e| on_select_from.call(e.value()),
                        option { value: "", "Select a snapshot..." }
                        for snapshot in &snapshots {
                            option {
                                value: snapshot.id.clone(),
                                selected: selected_from.as_ref() == Some(&snapshot.id),
                                "{snapshot.id}"
                            }
                        }
                    }
                }

                div { class: "space-y-2",
                    label { class: "block text-sm font-semibold text-slate-700 dark:text-slate-300", "{texts.snapshots_to}" }
                    select {
                        class: "w-full px-3 py-2 border border-slate-300 dark:border-slate-600 rounded-lg bg-white dark:bg-slate-800 text-slate-900 dark:text-white",
                        onchange: move |e| on_select_to.call(e.value()),
                        option { value: "", "Select a snapshot..." }
                        for snapshot in &snapshots {
                            option {
                                value: snapshot.id.clone(),
                                selected: selected_to.as_ref() == Some(&snapshot.id),
                                "{snapshot.id}"
                            }
                        }
                    }
                }
            }

            button {
                class: "btn-primary w-full text-sm py-1.5",
                onclick: move |_| on_compare.call(()),
                "{texts.snapshots_compare_button}"
            }

            if let Some(result) = diff_result {
                div { class: "p-3 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800/50 rounded-lg text-blue-900 dark:text-blue-100 text-sm font-mono whitespace-pre-wrap overflow-auto max-h-96",
                    "{result}"
                }
            }
        }
    }
}

#[component]
fn SnapshotsDetailsView(
    details: Option<Result<SnapshotDetails, String>>,
    on_back: EventHandler<()>,
) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    rsx! {
        div { class: "space-y-4",
            button {
                class: "btn-secondary py-1 px-2 flex items-center gap-2 text-sm",
                onclick: move |_| on_back.call(()),
                "{texts.snapshots_back_button}"
            }

            match details {
                None => rsx! {
                    p { class: "text-sm text-slate-600 dark:text-slate-400", "{texts.snapshots_loading}" }
                },
                Some(Err(e)) => rsx! {
                    div { class: "p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/50 rounded-lg text-red-700 dark:text-red-300 text-sm whitespace-pre-wrap",
                        "{e}"
                    }
                },
                Some(Ok(d)) => {
                    let date = format_timestamp(d.timestamp);
                    let encrypted = if d.encrypted { "yes" } else { "no" };
                    let size = d.summary.original_size.map(format_bytes).unwrap_or_else(|| "-".into());
                    let stored = format_bytes(d.summary.stored_size);
                    rsx! {
                        div { class: "p-4 bg-slate-50 dark:bg-slate-800/50 border border-slate-200 dark:border-slate-700 rounded-lg",
                            h3 { class: "font-semibold text-slate-900 dark:text-white mb-4 font-mono", "📸 {d.id}" }
                            div { class: "grid grid-cols-2 gap-2 text-sm text-slate-600 dark:text-slate-400 mb-4",
                                span { "Date" } span { "{date}" }
                                span { "Encrypted" } span { "{encrypted}" }
                                span { "Files" } span { "{d.summary.file_count} ({d.summary.encrypted_files} encrypted)" }
                                span { "Size" } span { "{size}" }
                                span { "Stored size" } span { "{stored}" }
                            }
                            div { class: "text-xs font-mono max-h-96 overflow-auto space-y-0.5",
                                for f in d.files.iter() {
                                    div { key: "{f.path}", class: "flex justify-between gap-3",
                                        span { class: "truncate", "{f.path}" }
                                        span { class: "whitespace-nowrap text-slate-500", "{format_bytes(f.display_size())}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsb_sdk::snapshot::{SnapshotFile, SnapshotSummary};
    use std::fs;

    fn s3(bucket: &str) -> S3Settings {
        S3Settings::from_values(bucket.into(), String::new(), String::new())
    }

    #[test]
    fn selection_prefers_profile_then_folder_then_bucket() {
        let p = Path::new("p.toml");
        let d = Path::new("repo");
        let none = Path::new("");
        assert_eq!(
            RepoSelection::choose(p, d, &s3("b")).unwrap(),
            RepoSelection::Profile(p.into())
        );
        assert_eq!(
            RepoSelection::choose(none, d, &s3("b")).unwrap(),
            RepoSelection::Directory(d.into())
        );
        assert_eq!(
            RepoSelection::choose(none, none, &s3("b")).unwrap(),
            RepoSelection::S3(s3("b"))
        );
        assert!(RepoSelection::choose(none, none, &s3("")).is_err());
        assert_eq!(RepoSelection::S3(s3("b")).label(), "s3://b");
    }

    #[test]
    fn s3_selection_builds_an_s3_config() {
        let cfg = RepoSelection::S3(S3Settings::from_values(
            "bucket".into(),
            "auto".into(),
            "https://s3.example.com".into(),
        ))
        .config()
        .unwrap();
        assert_eq!(cfg.s3_bucket.as_deref(), Some("bucket"));
        assert_eq!(cfg.s3_endpoint.as_deref(), Some("https://s3.example.com"));
        assert!(cfg.destination_path.is_empty());
    }

    fn write_manifest(repo: &Path, id: &str, files: &[(&str, u64)]) {
        let mut m = std::collections::HashMap::new();
        for (i, (p, size)) in files.iter().enumerate() {
            let mut meta =
                rsb_sdk::core::types::FileMetadata::new(format!("{:064x}", i + 1), false, true);
            meta.size = Some(*size);
            meta.stored_size = Some(*size);
            m.insert(PathBuf::from(p), meta);
        }
        // Uncompressed TOML: older manifests, still read.
        fs::write(
            repo.join("snapshots").join(format!("{id}.toml")),
            toml::to_string(&m).unwrap(),
        )
        .unwrap();
    }

    fn repo_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("snapshots")).unwrap();
        fs::create_dir_all(dir.path().join("data/clear")).unwrap();
        dir
    }

    #[tokio::test]
    async fn folder_repository_is_listed_newest_first() {
        let dir = repo_dir();
        write_manifest(dir.path(), "2026-05-24T100000.000Z", &[("a", 1)]);
        write_manifest(
            dir.path(),
            "2026-05-25T100000.000Z",
            &[("a", 1), ("b", 2048)],
        );
        write_manifest(dir.path(), "2026-05-25T100000.000Z-1", &[("a", 1)]);
        let sel = RepoSelection::Directory(dir.path().into());

        let list = load_snapshots(sel.clone(), String::new()).await.unwrap();
        let ids: Vec<_> = list.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "2026-05-25T100000.000Z-1",
                "2026-05-25T100000.000Z",
                "2026-05-24T100000.000Z"
            ]
        );
        assert_eq!(summary_cells(&list[1]).0, "2");
        assert_eq!(summary_cells(&list[1]).1, "2.00 KB");

        let d = load_details(sel.clone(), String::new(), "2026-05-25T100000.000Z".into())
            .await
            .unwrap();
        assert_eq!(d.files.len(), 2);

        let diff = compare(
            sel.clone(),
            String::new(),
            "2026-05-24T100000.000Z".into(),
            "2026-05-25T100000.000Z".into(),
        )
        .await
        .unwrap();
        assert!(diff_text(&diff).contains("+ b (2.00 KB)"));

        let done = delete(sel.clone(), String::new(), "2026-05-24T100000.000Z".into())
            .await
            .unwrap();
        assert!(deletion_message(&done).contains("2026-05-24T100000.000Z"));
        assert_eq!(load_snapshots(sel, String::new()).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn errors_are_reported_not_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let e = load_snapshots(RepoSelection::Directory(dir.path().into()), String::new())
            .await
            .unwrap_err();
        assert!(e.contains("Not a backup repository"), "{e}");

        let dir = repo_dir();
        fs::write(
            dir.path().join("snapshots/2026-05-25T100000.000Z.toml"),
            b"\xff\xfe",
        )
        .unwrap();
        let list = load_snapshots(RepoSelection::Directory(dir.path().into()), String::new())
            .await
            .unwrap();
        // Undecodable without a key: shown as encrypted/locked, not hidden.
        assert_eq!(list.len(), 1);
        assert!(summary_cells(&list[0]).2.contains("encrypted"));
    }

    #[test]
    fn cells_and_messages() {
        let info = SnapshotInfo {
            id: "x".into(),
            timestamp: None,
            encrypted: false,
            contents: SnapshotContents::Unreadable {
                error: "\n bad thing \nmore".into(),
            },
        };
        assert_eq!(summary_cells(&info).2, "⚠️ unreadable: bad thing");
        let ok = SnapshotInfo {
            contents: SnapshotContents::Readable(SnapshotSummary {
                file_count: 1,
                ..Default::default()
            }),
            encrypted: true,
            ..info
        };
        assert_eq!(summary_cells(&ok).2, "🔒 encrypted");
        let f = SnapshotFile {
            path: "p".into(),
            size: None,
            stored_size: Some(3),
            encrypted: false,
            chunks: 0,
        };
        let d = SnapshotDiff {
            from: "a".into(),
            to: "b".into(),
            removed: vec![f],
            ..Default::default()
        };
        assert!(diff_text(&d).contains("- p (3 B)"));
        assert!(
            diff_text(&SnapshotDiff::default()).starts_with("No changes"),
            "{}",
            diff_text(&SnapshotDiff::default())
        );
        let last = SnapshotDeletion {
            id: "x".into(),
            objects_deleted: 0,
            gc_ran: false,
        };
        assert!(deletion_message(&last).contains("last snapshot"));
    }
}
