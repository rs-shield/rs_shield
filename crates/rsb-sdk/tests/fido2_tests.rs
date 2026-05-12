use rsb_sdk::credentials::Fido2Manager;
use tempfile::NamedTempFile;

#[test]
fn test_fido2_manager_creation() {
    let manager = Fido2Manager::new("http://localhost:3000", "localhost", "Test FIDO2");

    assert!(manager.is_ok());
}

#[test]
fn test_fido2_list_credentials_empty() {
    let manager = Fido2Manager::new("http://localhost:3000", "localhost", "Test FIDO2").unwrap();

    let creds = manager.list_credentials();
    assert_eq!(creds.len(), 0);
}

#[test]
fn test_fido2_storage_path() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();

    let mut manager =
        Fido2Manager::new("http://localhost:3000", "localhost", "Test FIDO2").unwrap();

    // Save the current state first to ensure the file is not empty/invalid for reading
    manager
        .save_to_file(&path)
        .expect("Failed to initialize storage file");
    let result = manager.load_from_file(&path);

    assert!(result.is_ok());
}

#[test]
fn test_fido2_get_nonexistent_credential() {
    let manager = Fido2Manager::new("http://localhost:3000", "localhost", "Test FIDO2").unwrap();

    let result = manager.get_credential("nonexistent");
    assert!(result.is_none());
}

#[test]
fn test_fido2_error_display() {
    use rsb_sdk::credentials::Fido2Error;

    assert_eq!(
        Fido2Error::CredentialNotFound.to_string(),
        "Credential not found"
    );
    assert_eq!(
        Fido2Error::RegistrationFailed("already exists".into()).to_string(),
        "Registration failed: already exists"
    );
    assert!(
        Fido2Error::NoRegistrationInProgress
            .to_string()
            .contains("No registration")
    );
}
