/// User-friendly error message handler
/// Masks technical errors and provides localized messages
use crate::ui::i18n::{Translations, tr};
use tracing::warn;

fn is_security_key_error(e: &str) -> bool {
    [
        "fido",
        "webauthn",
        "ctap",
        "u2f",
        "security key",
        "security-key",
        "authenticator",
    ]
    .iter()
    .any(|k| e.contains(k))
}

/// Convert technical error messages to user-friendly messages
/// Technical errors are logged but not shown to the user
pub fn format_user_error(t: &Translations, error: impl std::fmt::Display, context: &str) -> String {
    let error_str = error.to_string().to_lowercase();

    // Log the real error for debugging
    warn!("[{}] Technical error: {}", context, error_str);

    // Return user-friendly message based on error type
    match error_str.as_str() {
        // Hardware security key (FIDO2/WebAuthn/CTAP) errors. Checked first so
        // e.g. "FIDO2 device timeout" or "WebAuthn credential not found" are not
        // reported as generic timeout/path errors.
        e if is_security_key_error(e) => t.errh_security_key.to_string(),

        // Network/Connection errors
        e if e.contains("connection refused") || e.contains("connection reset") => {
            t.errh_connection.to_string()
        }
        e if e.contains("timeout") => t.errh_timeout.to_string(),

        // Authentication errors
        e if e.contains("unauthorized") || e.contains("invalid token") => {
            t.errh_auth_expired.to_string()
        }
        e if e.contains("authentication failed") => t.errh_auth_failed.to_string(),

        // File/Path errors
        e if e.contains("not found") || e.contains("no such file") => t.errh_not_found.to_string(),
        e if e.contains("permission denied") => t.errh_permission_denied.to_string(),
        e if e.contains("disk space") || e.contains("no space") => t.errh_disk_space.to_string(),

        // Encryption/Decryption errors
        e if e.contains("decryption") || e.contains("decrypt") => t.errh_decrypt.to_string(),
        e if e.contains("encryption") || e.contains("encrypt") => t.errh_encrypt.to_string(),

        // Backup/Restore errors
        e if e.contains("backup") && e.contains("failed") => t.errh_backup_failed.to_string(),
        e if e.contains("restore") && e.contains("failed") => t.errh_restore_failed.to_string(),

        // JSON/Format errors
        e if e.contains("json") || e.contains("parse") => t.errh_parse.to_string(),

        // Hardware/FIDO2 credential errors
        e if e.contains("credential") => t.errh_security_key.to_string(),

        // Recovery code errors
        e if e.contains("recovery") && e.contains("code") => t.errh_recovery_code.to_string(),

        // Server/API errors
        e if e.contains("500") || e.contains("internal server error") => t.errh_server.to_string(),
        e if e.contains("503") || e.contains("service unavailable") => {
            t.errh_unavailable.to_string()
        }

        // Default: Generic error
        _ => t.errh_unexpected.to_string(),
    }
}

/// Format error message with specific operation context
#[allow(dead_code)] // usado só em testes, por agora
pub fn format_operation_error(
    t: &Translations,
    error: impl std::fmt::Display,
    operation: &str,
) -> String {
    let friendly_msg = format_user_error(t, &error, operation);

    // Add specific operation context if the message does not already name it
    let (prefix, already_named) = match operation {
        "backup" => (t.errh_backup_prefix, friendly_msg == t.errh_backup_failed),
        "restore" => (t.errh_restore_prefix, friendly_msg == t.errh_restore_failed),
        "verify" => (t.errh_verify_prefix, false),
        "auth" | "authentication" => (
            t.errh_auth_prefix,
            friendly_msg == t.errh_auth_expired || friendly_msg == t.errh_auth_failed,
        ),
        "fido2" => (t.errh_fido2_prefix, friendly_msg == t.errh_security_key),
        _ => return friendly_msg,
    };
    if already_named {
        friendly_msg
    } else {
        tr(prefix, &[&friendly_msg.replace("❌ ", "")])
    }
}

/// Extract just the user-friendly message without the emoji prefix
#[allow(dead_code)] // usado só em testes, por agora
pub fn get_error_message_only(
    t: &Translations,
    error: impl std::fmt::Display,
    context: &str,
) -> String {
    format_user_error(t, error, context)
        .replace("❌ ", "")
        .replace("🔐 ", "")
        .replace("📁 ", "")
        .replace("⏱️ ", "")
        .replace("💾 ", "")
        .replace("🔒 ", "")
        .replace("📂 ", "")
        .replace("🔑 ", "")
        .replace("📄 ", "")
        .replace("🖥️ ", "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::i18n::{Language, get_texts};

    fn en() -> &'static Translations {
        get_texts(Language::En)
    }

    #[test]
    fn test_connection_error() {
        let error = "Connection refused";
        let result = format_user_error(en(), error, "test");
        assert!(result.contains("Connection error"));
    }

    #[test]
    fn test_file_not_found() {
        let error = "No such file or directory";
        let result = format_user_error(en(), error, "test");
        assert!(result.contains("not found"));
    }

    #[test]
    fn test_fido2_error() {
        let error = "FIDO2 credential failed";
        let result = format_user_error(en(), error, "test");
        assert!(result.contains("security key"));
    }

    #[test]
    fn test_webauthn_errors_map_to_security_key() {
        for err in [
            "WebAuthn assertion timeout",
            "fido2 authentication failed",
            "CTAP2 device not found",
            "Security-Key unplugged",
        ] {
            let result = format_user_error(en(), err, "test");
            assert!(result.contains("security key"), "{err} -> {result}");
        }
        // fido2 operation context must not double-prefix the message.
        let msg = format_operation_error(en(), "FIDO2 credential failed", "fido2");
        assert!(!msg.contains("Security key error: "));
    }
}
