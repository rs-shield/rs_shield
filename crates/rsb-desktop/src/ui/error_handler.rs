/// User-friendly error message handler
/// Masks technical errors and provides localized messages in English

use tracing::warn;

/// Convert technical error messages to user-friendly messages
/// Technical errors are logged but not shown to the user
pub fn format_user_error(error: impl std::fmt::Display, context: &str) -> String {
    let error_str = error.to_string().to_lowercase();

    // Log the real error for debugging
    warn!("[{}] Technical error: {}", context, error_str);

    // Return user-friendly message based on error type
    match error_str.as_str() {
        // Network/Connection errors
        e if e.contains("connection refused") || e.contains("connection reset") => {
            "❌ Connection error. Please check your network and try again.".to_string()
        }
        e if e.contains("timeout") => {
            "⏱️ The operation took too long. Please try again.".to_string()
        }

        // Authentication errors
        e if e.contains("unauthorized") || e.contains("invalid token") => {
            "🔐 Authentication expired. Please authenticate again.".to_string()
        }
        e if e.contains("authentication failed") => {
            "❌ Authentication failed. Please verify your security key.".to_string()
        }

        // File/Path errors
        e if e.contains("not found") || e.contains("no such file") => {
            "📁 File or folder not found. Please verify the path.".to_string()
        }
        e if e.contains("permission denied") => {
            "🔒 Permission denied. Please check the folder permissions.".to_string()
        }
        e if e.contains("disk space") || e.contains("no space") => {
            "💾 Insufficient disk space. Free up some space and try again.".to_string()
        }

        // Encryption/Decryption errors
        e if e.contains("decryption") || e.contains("decrypt") => {
            "🔐 Failed to decrypt data. Please verify the encryption key.".to_string()
        }
        e if e.contains("encryption") || e.contains("encrypt") => {
            "🔐 Failed to encrypt data. Please try again.".to_string()
        }

        // Backup/Restore errors
        e if e.contains("backup") && e.contains("failed") => {
            "💾 Backup failed. Please check the destination and try again.".to_string()
        }
        e if e.contains("restore") && e.contains("failed") => {
            "📂 Restore failed. Please verify the snapshot and try again.".to_string()
        }

        // JSON/Format errors
        e if e.contains("json") || e.contains("parse") => {
            "📄 Failed to process data. Please try reloading the application.".to_string()
        }

        // Hardware/FIDO2 errors
        e if e.contains("fido2") || e.contains("security key") || e.contains("credential") => {
            "🔑 Security key error. Please reconnect it or verify the connection.".to_string()
        }

        // Recovery code errors
        e if e.contains("recovery") && e.contains("code") => {
            "🔐 Invalid or already used recovery code.".to_string()
        }

        // Server/API errors
        e if e.contains("500") || e.contains("internal server error") => {
            "🖥️ Server error. Please try again in a few moments.".to_string()
        }
        e if e.contains("503") || e.contains("service unavailable") => {
            "🖥️ Service temporarily unavailable. Please try again later.".to_string()
        }

        // Default: Generic error
        _ => {
            "❌ An unexpected error occurred. Please try again.".to_string()
        }
    }
}

/// Format error message with specific operation context
pub fn format_operation_error(error: impl std::fmt::Display, operation: &str) -> String {
    let friendly_msg = format_user_error(&error, operation);

    // Add specific operation context if not already in the message
    match operation {
        "backup" => {
            if friendly_msg.contains("Backup") {
                friendly_msg
            } else {
                format!("💾 Backup error: {}", friendly_msg.replace("❌ ", ""))
            }
        }
        "restore" => {
            if friendly_msg.contains("Restore") {
                friendly_msg
            } else {
                format!("📂 Restore error: {}", friendly_msg.replace("❌ ", ""))
            }
        }
        "verify" => {
            if friendly_msg.contains("verification") {
                friendly_msg
            } else {
                format!("🔍 Verification error: {}", friendly_msg.replace("❌ ", ""))
            }
        }
        "auth" | "authentication" => {
            if friendly_msg.contains("Authentication") {
                friendly_msg
            } else {
                format!("🔐 Authentication error: {}", friendly_msg.replace("❌ ", ""))
            }
        }
        "fido2" => {
            if friendly_msg.contains("security key") {
                friendly_msg
            } else {
                format!("🔑 Security key error: {}", friendly_msg.replace("❌ ", ""))
            }
        }
        _ => friendly_msg,
    }
}

/// Extract just the user-friendly message without the emoji prefix
pub fn get_error_message_only(error: impl std::fmt::Display, context: &str) -> String {
    format_user_error(error, context)
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

    #[test]
    fn test_connection_error() {
        let error = "Connection refused";
        let result = format_user_error(error, "test");
        assert!(result.contains("Connection error"));
    }

    #[test]
    fn test_file_not_found() {
        let error = "No such file or directory";
        let result = format_user_error(error, "test");
        assert!(result.contains("not found"));
    }

    #[test]
    fn test_fido2_error() {
        let error = "FIDO2 credential failed";
        let result = format_user_error(error, "test");
        assert!(result.contains("security key"));
    }
}