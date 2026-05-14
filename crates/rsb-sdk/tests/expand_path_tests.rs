#[cfg(test)]
mod tests_expand_path {
    use rsb_sdk::utils::expand_path;
    use std::path::PathBuf;

    #[test]
    fn test_expand_home_directory() {
        let path = "~/backups";
        let expanded = expand_path(path);

        // Must start with the home directory
        if let Some(home) = dirs::home_dir() {
            assert!(
                expanded.starts_with(&home),
                "Path should start with home directory"
            );
            assert!(
                expanded.ends_with("backups"),
                "Path should end with 'backups'"
            );
        }
    }

    #[test]
    fn test_expand_home_with_multiple_segments() {
        let path = "~/Documents/projects/backup";
        let expanded = expand_path(path);

        if let Some(home) = dirs::home_dir() {
            assert!(
                expanded.starts_with(&home),
                "Path should start with home directory"
            );
            assert!(
                expanded.to_string_lossy().contains("Documents"),
                "Should contain Documents"
            );
            assert!(
                expanded.to_string_lossy().contains("backup"),
                "Should contain backup"
            );
        }
    }

    #[test]
    fn test_absolute_path_unchanged() {
        let path = "/absolute/path/to/backup";
        let expanded = expand_path(path);

        assert_eq!(
            expanded,
            PathBuf::from(path),
            "Absolute path should remain unchanged"
        );
    }

    #[test]
    fn test_expand_home_env_var() {
        // If HOME is defined
        if let Ok(home) = std::env::var("HOME") {
            let path = "$HOME/backups";
            let expanded = expand_path(path);
            let expanded_str = expanded.to_string_lossy().to_string();

            assert!(expanded_str.starts_with(&home), "Should expand $HOME");
            assert!(expanded_str.ends_with("backups"), "Should end with backups");
        }
    }

    #[test]
    fn test_expand_home_env_var_braces() {
        // If HOME is defined
        if let Ok(home) = std::env::var("HOME") {
            let path = "${HOME}/backups";
            let expanded = expand_path(path);
            let expanded_str = expanded.to_string_lossy().to_string();

            assert!(expanded_str.starts_with(&home), "Should expand braced HOME");
            assert!(expanded_str.ends_with("backups"), "Should end with backups");
        }
    }

    #[test]
    fn test_tilde_only() {
        let path = "~";
        let expanded = expand_path(path);

        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home, "Tilde only should expand to home directory");
        }
    }

    #[test]
    fn test_relative_path_unchanged() {
        let path = "relative/path/to/backup";
        let expanded = expand_path(path);

        assert_eq!(
            expanded,
            PathBuf::from(path),
            "Relative path without ~ should stay unchanged"
        );
    }

    #[test]
    #[cfg(windows)]
    fn test_windows_userprofile_expansion() {
        // On Windows, USERPROFILE should be expanded
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let path = "$USERPROFILE/backups";
            let expanded = expand_path(path);
            let expanded_str = expanded.to_string_lossy().to_string();

            assert!(
                expanded_str.starts_with(&userprofile),
                "Should expand USERPROFILE on Windows"
            );
            assert!(expanded_str.ends_with("backups"), "Should end with backups");
        }
    }

    #[test]
    #[cfg(windows)]
    fn test_windows_home_fallback_to_userprofile() {
        // On Windows, if HOME is not defined but USERPROFILE is,
        // $HOME should fallback to USERPROFILE
        let path = "$HOME/backups";
        let expanded = expand_path(path);

        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy().to_string();
            let expanded_str = expanded.to_string_lossy().to_string();

            // Should expand to the same as home_dir() returns
            assert!(
                expanded_str.starts_with(&home_str),
                "Should expand HOME to user home on Windows"
            );
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_macos_home_expansion() {
        let path = "$HOME/backups";
        let expanded = expand_path(path);

        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy().to_string();
            let expanded_str = expanded.to_string_lossy().to_string();

            assert!(
                expanded_str.starts_with(&home_str),
                "Should expand HOME on macOS"
            );
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_home_expansion() {
        let path = "$HOME/backups";
        let expanded = expand_path(path);

        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy().to_string();
            let expanded_str = expanded.to_string_lossy().to_string();

            assert!(
                expanded_str.starts_with(&home_str),
                "Should expand HOME on Linux"
            );
        }
    }

    #[test]
    fn test_multiple_env_vars() {
        // Test with multiple variables
        unsafe {
        std::env::set_var("TEST_VAR1", "value1");
        std::env::set_var("TEST_VAR2", "value2");
        }
        let path = "$TEST_VAR1/backup/$TEST_VAR2";
        let expanded = expand_path(path);
        let expanded_str = expanded.to_string_lossy().to_string();

        assert!(expanded_str.contains("value1"), "Should expand TEST_VAR1");
        assert!(expanded_str.contains("value2"), "Should expand TEST_VAR2");
    }

    #[test]
    fn test_mixed_tilde_and_env_vars() {
        unsafe {
        std::env::set_var("BACKUP_TYPE", "daily");
        }
        let path = "~/backups/$BACKUP_TYPE";
        let expanded = expand_path(path);
        let expanded_str = expanded.to_string_lossy().to_string();

        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy().to_string();
            assert!(expanded_str.starts_with(&home_str), "Should expand tilde");
            assert!(expanded_str.contains("daily"), "Should expand env var");
        }
    }

    #[test]
    fn test_undefined_env_var_kept_as_is() {
        let path = "$UNDEFINED_VAR_12345/backups";
        let expanded = expand_path(path);
        let expanded_str = expanded.to_string_lossy().to_string();

        // Undefined variable should be kept as-is
        assert!(
            expanded_str.contains("UNDEFINED_VAR_12345"),
            "Undefined var should be kept as-is"
        );
    }
}
