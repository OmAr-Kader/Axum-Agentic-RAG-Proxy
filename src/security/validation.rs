use crate::error::AppError;

/// Validate category name: ^[a-zA-Z0-9_\-]+$
pub fn validate_category(category: &str) -> Result<(), AppError> {
    if category.is_empty()
        || !category
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::Validation(format!(
            "Invalid category name '{}': must match ^[a-zA-Z0-9_\\-]+$",
            category
        )));
    }
    Ok(())
}

/// Validate filename: ^[a-zA-Z0-9_\-\.]+\.md$
pub fn validate_filename(filename: &str) -> Result<(), AppError> {
    if !filename.ends_with(".md") {
        return Err(AppError::Validation(format!(
            "Invalid filename '{}': must end with .md",
            filename
        )));
    }
    let stem = &filename[..filename.len() - 3];
    if stem.is_empty()
        || !stem
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(AppError::Validation(format!(
            "Invalid filename '{}': must match ^[a-zA-Z0-9_\\-\\.]+\\.md$",
            filename
        )));
    }
    Ok(())
}

/// Validate content size against MAX_RULE_CONTENT_BYTES
pub fn validate_content_size(content: &[u8], max_bytes: usize) -> Result<(), AppError> {
    if content.len() > max_bytes {
        return Err(AppError::PayloadTooLarge(format!(
            "Content size {} bytes exceeds maximum {} bytes",
            content.len(),
            max_bytes
        )));
    }
    Ok(())
}
