use rsb_sdk::credentials::Fido2Manager;

#[test]
fn test_fido2_manager_basic_operations() {
    let manager = Fido2Manager::new("http://localhost:3000", "localhost", "RSB Test").unwrap();

    // manager inicalized with empty credentials
    assert_eq!(manager.list_credentials().len(), 0);

    // credential retrieval returns None for non-existent credential
    assert!(manager.list_user_credentials("alice").is_empty());

    // has_credential
    assert!(!manager.has_credential("alice"));

    println!("✅ Basic manager tests passed");
}

#[test]
fn test_authentication_without_registration() {
    let mut manager = Fido2Manager::new("http://localhost:3000", "localhost", "RSB Test").unwrap();

    let result = manager.start_authentication("nonexistent");

    assert!(result.is_err());

    println!("✅ Nonexistent user correctly rejected");
}

#[test]
fn test_registration_challenge_creation() {
    let mut manager = Fido2Manager::new("http://localhost:3000", "localhost", "RSB Test").unwrap();

    let challenge = manager
        .start_registration("alice", "alice@example.com", "Alice")
        .unwrap();

    // challenge created with expected fields
    assert!(!challenge.public_key.challenge.as_ref().is_empty());

    println!("✅ Registration challenge created");
}
