use serde::Deserialize;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct DownloadVideoRequest {
    #[validate(custom(function = "validate_youtube_url"))]
    pub url: String,
}

pub fn validate_youtube_url(url_str: &str) -> Result<(), validator::ValidationError> {
    let trimmed = url_str.trim();
    let lower = trimmed.to_lowercase();

    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(validator::ValidationError::new("invalid_url_scheme"));
    }

    let is_valid = lower.contains("youtube.com/watch")
        || lower.contains("youtube.com/shorts/")
        || lower.contains("youtu.be/")
        || lower.contains("m.youtube.com/watch")
        || lower.contains("m.youtube.com/shorts/")
        || lower.contains("youtube.com/v/")
        || lower.contains("youtube.com/embed/");

    if is_valid {
        Ok(())
    } else {
        Err(validator::ValidationError::new("invalid_youtube_url"))
    }
}
