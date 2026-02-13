use std::collections::HashSet;
use validator::ValidationError;

const MAX_MESSAGE_LEN: usize = 4000;

pub fn validate_username(value: &str) -> Result<(), ValidationError> {
    let trimmed = value.trim();
    if trimmed.len() < 3 || trimmed.len() > 32 {
        return Err(ValidationError::new("username_length"));
    }

    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ValidationError::new("username_chars"));
    }

    Ok(())
}

pub fn validate_server_name(value: &str) -> Result<(), ValidationError> {
    let trimmed = value.trim();
    if trimmed.len() < 2 || trimmed.len() > 100 {
        return Err(ValidationError::new("server_name_length"));
    }
    Ok(())
}

pub fn validate_channel_name(value: &str) -> Result<(), ValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(ValidationError::new("channel_name_length"));
    }

    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ValidationError::new("channel_name_chars"));
    }

    Ok(())
}

pub fn validate_message_content(value: &str) -> Result<(), ValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_MESSAGE_LEN {
        return Err(ValidationError::new("message_content_length"));
    }
    Ok(())
}

pub fn validate_avatar_url(value: &str) -> Result<(), ValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::new("avatar_url_empty"));
    }
    if trimmed.len() > 1024 {
        return Err(ValidationError::new("avatar_url_length"));
    }
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(ValidationError::new("avatar_url_scheme"));
    }
    Ok(())
}

pub fn validate_emoji(value: &str) -> Result<(), ValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 32 {
        return Err(ValidationError::new("emoji_length"));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(ValidationError::new("emoji_whitespace"));
    }
    Ok(())
}

pub fn normalize_email(value: &str) -> String {
    value.trim().to_lowercase()
}

pub fn normalize_username(value: &str) -> String {
    value.trim().to_string()
}

pub fn extract_mentions(content: &str) -> Vec<String> {
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'@' {
            i += 1;
            continue;
        }

        let start = i + 1;
        let mut end = start;
        while end < bytes.len() {
            let c = bytes[end] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                end += 1;
            } else {
                break;
            }
        }

        if end > start {
            let mention = &content[start..end];
            if (3..=32).contains(&mention.len()) {
                let key = mention.to_ascii_lowercase();
                if seen.insert(key) {
                    out.push(mention.to_string());
                }
            }
        }

        i = end;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_validation_allows_expected_chars() {
        assert!(validate_username("alice_01").is_ok());
        assert!(validate_username("bob-the-builder").is_ok());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("bad name").is_err());
        assert!(validate_username("bad!name").is_err());
    }

    #[test]
    fn mention_extraction_deduplicates_and_preserves_order() {
        let mentions = extract_mentions("Hey @Alice and @bob, ping @alice again and @carol_2");
        assert_eq!(mentions, vec!["Alice", "bob", "carol_2"]);
    }

    #[test]
    fn message_content_validation_rejects_empty() {
        assert!(validate_message_content("hello").is_ok());
        assert!(validate_message_content("   ").is_err());
    }
}
