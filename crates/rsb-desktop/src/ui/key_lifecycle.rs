//! Repository key lifecycle for the desktop app: init, migrate and rotate,
//! through the same SDK calls as `rsb key init/migrate/rotate`
//! ([`rsb_sdk::crypto::keyfile`], [`rsb_sdk::crypto::lifecycle`]).
//!
//! Everything here is UI-independent so it can be tested headless; the
//! screen ([`crate::ui::key_screen`]) only wires it to signals. Passwords
//! are only borrowed: never logged, never stored.

use crate::ui::i18n::{Translations, tr};
use rsb_sdk::core::types::ProgressCallback;
use rsb_sdk::crypto::keyfile::{self, KdfParams};
use rsb_sdk::crypto::lifecycle::{self, KeyStatus, MigrationReport, RotationReport};
use rsb_sdk::snapshot::format_bytes;
use rsb_sdk::storage::Storage;
use rsb_sdk::{CancellationToken, RsbError};
use std::future::Future;
use std::sync::Arc;

/// Runs `make()` on a blocking-pool thread of the current runtime, so long
/// key operations (KDF, re-encryption of the whole repository) never stall
/// the UI thread. Same pattern as the diagnostics screen.
pub async fn off_ui_thread<T, F, Fut>(make: F) -> Result<T, String>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T>,
    T: Send + 'static,
{
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || handle.block_on(make()))
        .await
        .map_err(|e| e.to_string())
}

/// Fraction done after item `i` (0-based) of `total`.
pub fn progress_fraction(i: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        ((i + 1) as f64 / total as f64).clamp(0.0, 1.0)
    }
}

/// Status text of a repository's key (what the screen shows after
/// "Check repository").
pub fn status_text(s: &KeyStatus, t: &Translations) -> String {
    let mut lines = Vec::new();
    if s.key_file {
        lines.push(tr(t.kl_status_key, &[&s.key_id.as_deref().unwrap_or("?")]));
    } else {
        lines.push(t.kl_status_no_key.to_string());
    }
    if s.rotating {
        lines.push(t.kl_status_rotating.to_string());
    }
    lines.push(tr(
        t.kl_status_encrypted,
        &[
            &s.encrypted_objects.to_string(),
            &format_bytes(s.encrypted_bytes),
            &s.encrypted_manifests.to_string(),
        ],
    ));
    if !s.legacy.is_empty() {
        lines.push(legacy_counts(t.kl_status_legacy, s));
    }
    if s.legacy_secret {
        lines.push(t.kl_status_legacy_secret.to_string());
    }
    lines.join("\n")
}

fn legacy_counts(template: &str, s: &KeyStatus) -> String {
    tr(
        template,
        &[
            &s.legacy.objects.to_string(),
            &format_bytes(s.legacy.bytes),
            &s.legacy.manifests.to_string(),
        ],
    )
}

/// What a migration will do, shown before the user confirms it.
pub fn migrate_preview(s: &KeyStatus, t: &Translations) -> String {
    if s.legacy.is_empty() {
        t.kl_migrate_nothing.to_string()
    } else {
        legacy_counts(t.kl_migrate_confirm, s)
    }
}

/// What a rotation will do, shown before the user confirms it.
pub fn rotate_preview(s: &KeyStatus, t: &Translations) -> String {
    let mut msg = tr(
        t.kl_rotate_confirm,
        &[
            &s.encrypted_objects.to_string(),
            &format_bytes(s.encrypted_bytes),
            &s.encrypted_manifests.to_string(),
        ],
    );
    if s.rotating {
        msg.push('\n');
        msg.push_str(t.kl_rotate_resume);
    }
    if !s.legacy.is_empty() {
        msg.push('\n');
        msg.push_str(&legacy_counts(t.kl_status_legacy, s));
    }
    msg
}

/// Checks the new password and its confirmation.
pub fn check_new_password(new: &str, confirm: &str, t: &Translations) -> Result<(), String> {
    if new.is_empty() {
        return Err(t.rk_new_password_empty.to_string());
    }
    if new != confirm {
        return Err(t.rk_new_passwords_mismatch.to_string());
    }
    Ok(())
}

/// Error text of a key operation; a cancellation says how to resume.
pub fn op_error(e: &RsbError, t: &Translations) -> String {
    if matches!(e, RsbError::Cancelled) {
        t.kl_stopped.to_string()
    } else {
        format!("❌ {}", e)
    }
}

/// `rsb key init`: creates `keys/repo.key`. Returns the message to show
/// (with the reminder to export the key) and the new status.
pub async fn init_key(
    storage: Arc<dyn Storage>,
    new: &str,
    confirm: &str,
    params: KdfParams,
    t: &Translations,
) -> Result<(String, KeyStatus), String> {
    check_new_password(new, confirm, t)?;
    if keyfile::repo_key_exists(&*storage)
        .await
        .map_err(|e| op_error(&e, t))?
    {
        return Err(format!("❌ {}", t.kl_init_exists));
    }
    keyfile::init_repo_key(storage.clone(), new.as_bytes(), params)
        .await
        .map_err(|e| op_error(&e, t))?;
    let status = lifecycle::key_status(&*storage)
        .await
        .map_err(|e| op_error(&e, t))?;
    let mut msg = format!("{}\n{}", t.kl_init_done, t.kl_export_reminder);
    if !status.legacy.is_empty() {
        msg.push('\n');
        msg.push_str(&legacy_counts(t.kl_status_legacy, &status));
        msg.push('\n');
        msg.push_str(t.kl_init_legacy);
    }
    Ok((msg, status))
}

/// Result message of a migration.
pub fn migration_message(rep: &MigrationReport, t: &Translations) -> String {
    let mut msg = tr(
        t.kl_migrate_done,
        &[
            &rep.objects_migrated.to_string(),
            &rep.objects_already_v3.to_string(),
            &rep.manifests_rewritten.to_string(),
            &rep.objects_verified.to_string(),
        ],
    );
    if rep.legacy_secret_removed {
        msg.push('\n');
        msg.push_str(t.kl_migrate_secret_removed);
    }
    if !rep.unreferenced_legacy.is_empty() {
        msg.push('\n');
        msg.push_str(&tr(
            t.kl_migrate_unreferenced,
            &[&rep.unreferenced_legacy.len().to_string()],
        ));
    }
    if !rep.unreadable_legacy.is_empty() {
        msg.push('\n');
        msg.push_str(&tr(
            t.kl_migrate_unreadable,
            &[&rep.unreadable_legacy.len().to_string()],
        ));
        for p in rep.unreadable_legacy.iter().take(10) {
            msg.push_str("\n   ");
            msg.push_str(p);
        }
    }
    msg
}

/// Result message of a rotation, with the reminder to export the new key.
pub fn rotation_message(rep: &RotationReport, t: &Translations) -> String {
    let mut msg = tr(
        t.kl_rotate_done,
        &[
            &rep.old_key_id,
            &rep.new_key_id,
            &rep.objects_rotated.to_string(),
            &rep.manifests_rewritten.to_string(),
            &rep.objects_verified.to_string(),
        ],
    );
    if !rep.unreadable_unreferenced.is_empty() {
        msg.push('\n');
        msg.push_str(&tr(
            t.kl_rotate_unreadable,
            &[&rep.unreadable_unreferenced.len().to_string()],
        ));
    }
    msg.push('\n');
    msg.push_str(t.kl_rotate_export_reminder);
    msg
}

/// `rsb key migrate` (resumable; `token` stops it after the current
/// object).
pub async fn migrate(
    storage: Arc<dyn Storage>,
    password: &str,
    progress: Option<ProgressCallback>,
    token: Option<CancellationToken>,
    t: &Translations,
) -> Result<String, String> {
    lifecycle::migrate_legacy_objects(storage, password.as_bytes(), progress, token)
        .await
        .map(|rep| migration_message(&rep, t))
        .map_err(|e| op_error(&e, t))
}

/// `rsb key rotate` (resumable; `token` stops it after the current
/// object).
pub async fn rotate(
    storage: Arc<dyn Storage>,
    password: &str,
    progress: Option<ProgressCallback>,
    token: Option<CancellationToken>,
    t: &Translations,
) -> Result<String, String> {
    lifecycle::rotate_repo_key(storage, password.as_bytes(), progress, token)
        .await
        .map(|rep| rotation_message(&rep, t))
        .map_err(|e| op_error(&e, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::i18n::{EN_STRINGS, PT_STRINGS};
    use rsb_sdk::crypto::lifecycle::LegacySummary;
    use rsb_sdk::storage::LocalStorage;
    use std::sync::Mutex;

    const T: &Translations = &EN_STRINGS;

    fn local_repo(dir: &tempfile::TempDir) -> Arc<dyn Storage> {
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        Arc::new(LocalStorage::new(&repo.to_string_lossy()))
    }

    #[test]
    fn previews_show_the_counts() {
        let s = KeyStatus {
            key_file: true,
            key_id: Some("abcd".into()),
            legacy: LegacySummary {
                objects: 7,
                bytes: 2048,
                manifests: 2,
            },
            encrypted_objects: 12,
            encrypted_bytes: 4096,
            encrypted_manifests: 3,
            ..Default::default()
        };
        let m = migrate_preview(&s, T);
        assert!(m.contains("7 v1/v2 object(s)") && m.contains("2 v1/v2 manifest"));
        let r = rotate_preview(&s, T);
        assert!(
            r.contains("12 object(s)") && r.contains("3 manifest(s)"),
            "{r}"
        );
        assert!(!r.contains("resumed"));
        let st = status_text(&s, T);
        assert!(st.contains("abcd") && st.contains("7 v1/v2"), "{st}");

        let resuming = KeyStatus {
            rotating: true,
            legacy: LegacySummary::default(),
            ..s.clone()
        };
        assert!(rotate_preview(&resuming, T).contains("resumed"));
        assert_eq!(migrate_preview(&resuming, T), T.kl_migrate_nothing);
        // Portuguese texts carry the same numbers.
        assert!(migrate_preview(&s, &PT_STRINGS).contains('7'));
    }

    #[test]
    fn progress_and_password_checks() {
        assert_eq!(progress_fraction(0, 0), 0.0);
        assert_eq!(progress_fraction(0, 4), 0.25);
        assert_eq!(progress_fraction(3, 4), 1.0);
        assert!(check_new_password("", "", T).is_err());
        assert!(check_new_password("a", "b", T).is_err());
        assert!(check_new_password("a", "a", T).is_ok());
        assert_eq!(op_error(&RsbError::Cancelled, T), T.kl_stopped);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn init_migrate_rotate_on_a_local_repository() {
        let dir = tempfile::tempdir().unwrap();
        let storage = local_repo(&dir);

        // Mismatched confirmation: nothing is created.
        assert!(
            init_key(storage.clone(), "pw", "typo", KdfParams::MIN_CREATE, T)
                .await
                .is_err()
        );
        assert!(!keyfile::repo_key_exists(&*storage).await.unwrap());

        let (msg, status) = init_key(storage.clone(), "pw", "pw", KdfParams::MIN_CREATE, T)
            .await
            .unwrap();
        assert!(msg.contains("export a copy"), "{msg}");
        assert!(status.key_file && !status.can_init() && status.can_rotate());
        // A second init is refused.
        assert!(
            init_key(storage.clone(), "pw", "pw", KdfParams::MIN_CREATE, T)
                .await
                .unwrap_err()
                .contains("already exists")
        );

        // Some v3 data, written by a backup with the same password.
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"alpha ".repeat(300)).unwrap();
        std::fs::write(src.join("b.txt"), b"bravo").unwrap();
        let cfg = rsb_sdk::Config {
            source_path: src.to_string_lossy().into_owned(),
            destination_path: dir.path().join("repo").to_string_lossy().into_owned(),
            ..Default::default()
        };
        let r = rsb_sdk::backup::perform_backup(
            &cfg,
            rsb_sdk::BackupOptions {
                mode: Some("incremental".into()),
                ..Default::default()
            }
            .with_encryption_key(Some("pw")),
        )
        .await
        .unwrap();
        assert_eq!(r.files_with_errors, 0, "{:?}", r.errors);
        let status = lifecycle::key_status(&*storage).await.unwrap();
        assert!(status.encrypted_objects >= 2 && status.encrypted_manifests == 1);
        assert!(!status.needs_migration());

        // Nothing to migrate: it verifies and succeeds.
        let msg = migrate(storage.clone(), "pw", None, None, T).await.unwrap();
        assert!(msg.contains("Migration complete"), "{msg}");

        // A stopped rotation says how to resume and is reported as
        // interrupted; running it again completes it off the calling
        // thread, with progress.
        let token = CancellationToken::new();
        token.cancel();
        let stopped = rotate(storage.clone(), "pw", None, Some(token), T).await;
        assert_eq!(stopped.unwrap_err(), T.kl_stopped);
        let status = lifecycle::key_status(&*storage).await.unwrap();
        assert!(status.rotating);
        assert!(rotate_preview(&status, T).contains("resumed"));

        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        let cb: ProgressCallback = Arc::new(move |i, total, _msg| {
            seen2.lock().unwrap().push(progress_fraction(i, total));
        });
        let before = keyfile::read_key_file(&*storage)
            .await
            .unwrap()
            .unwrap()
            .key_id()
            .map(str::to_string);
        let st = storage.clone();
        let msg = off_ui_thread(move || async move { rotate(st, "pw", Some(cb), None, T).await })
            .await
            .unwrap()
            .unwrap();
        assert!(
            msg.contains("Key rotated") && msg.contains("export"),
            "{msg}"
        );
        let after = lifecycle::key_status(&*storage).await.unwrap();
        assert!(!after.rotating);
        assert_ne!(after.key_id, before);
        let fractions = seen.lock().unwrap().clone();
        assert!(!fractions.is_empty());
        assert!(fractions.iter().all(|p| (0.0..=1.0).contains(p)));

        // Wrong password: refused, nothing changes.
        assert!(
            rotate(storage.clone(), "nope", None, None, T)
                .await
                .is_err()
        );
        assert_eq!(
            lifecycle::key_status(&*storage).await.unwrap().key_id,
            after.key_id
        );
    }
}
