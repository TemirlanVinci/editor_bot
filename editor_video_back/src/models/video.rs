use serde::{Deserialize, Serialize};
use validator::Validate;

pub fn default_include_intro() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate, PartialEq)]
pub struct CutVideoRequest {
    #[serde(default = "default_include_intro")]
    pub include_intro: bool,
}

impl Default for CutVideoRequest {
    fn default() -> Self {
        Self {
            include_intro: default_include_intro(),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cut_video_request_default() {
        let req = CutVideoRequest::default();
        assert!(req.include_intro);
    }

    #[test]
    fn test_cut_video_request_serde_empty_json() {
        let req: CutVideoRequest = serde_json::from_str("{}").expect("deserialization failed");
        assert!(req.include_intro);
    }

    #[test]
    fn test_cut_video_request_serde_explicit_false() {
        let req: CutVideoRequest =
            serde_json::from_str(r#"{"include_intro": false}"#).expect("deserialization failed");
        assert!(!req.include_intro);
    }

    #[test]
    fn test_cut_video_request_serde_explicit_true() {
        let req: CutVideoRequest =
            serde_json::from_str(r#"{"include_intro": true}"#).expect("deserialization failed");
        assert!(req.include_intro);
    }
}
