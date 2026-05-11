pub fn checksum(
    data: &[u8],
    expected_size: Option<u64>,
    expected_hash: Option<&String>,
) -> Result<(), String> {
    let actual_size = data.len() as u64;

    if let Some(expected) = expected_size {
        if actual_size != expected {
            return Err(format!(
                "Size mismatch: expected {}, got {}",
                expected, actual_size
            ));
        }
    }

    if let Some(expected) = expected_hash {
        let actual_hash = crate::crypto::hash_file_content(data)
            .map_err(|e| format!("Hash calculation failed: {}", e))?;
        if actual_hash != *expected {
            return Err(format!(
                "Stored hash mismatch: expected {}, got {}",
                expected, actual_hash
            ));
        }
    }

    Ok(())
}
