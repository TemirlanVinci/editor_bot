use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn};

/// Запрос к microservice voice_service для преобразования тембра и очистки речи
#[derive(Debug, Serialize)]
pub struct VoiceConvertRequest<'a> {
    pub source_audio_path: &'a str,
    pub output_audio_path: &'a str,
    pub voice: &'a str,
    pub enable_demucs: bool,
}

/// Ответ от voice_service
#[derive(Debug, Deserialize)]
pub struct VoiceConvertResponse {
    pub status: String,
    pub output_path: Option<String>,
    pub voice_applied: Option<String>,
    pub demucs_applied: bool,
    pub message: Option<String>,
}

/// URL сервиса обработки голоса (Demucs v4 + CosyVoice 3)
pub fn voice_service_url() -> String {
    env::var("VOICE_SERVICE_URL").unwrap_or_else(|_| "http://voice_service:8082".to_string())
}

/// Проверяет, включена ли переозвучка (по умолчанию включена)
pub fn is_voice_conversion_enabled() -> bool {
    env::var("ENABLE_VOICE_CONVERSION")
        .map(|v| v != "false" && v != "0" && v != "no")
        .unwrap_or(true)
}

/// Проверяет, включена ли очистка вокала через Demucs v4 (по умолчанию включена)
pub fn is_demucs_enabled() -> bool {
    env::var("ENABLE_DEMUCS")
        .map(|v| v != "false" && v != "0" && v != "no")
        .unwrap_or(true)
}

/// Выполняет переозвучку аудиофрагмента через CosyVoice 3 (с опциональной очисткой Demucs v4).
///
/// Если голос не найден в `media/voices/` или сервис недоступен,
/// безопасно возвращает исходный/очищенный звук без прерывания монтажа.
pub async fn convert_voice(
    source_audio_path: &Path,
    output_audio_path: &Path,
    voice: &str,
    enable_demucs: bool,
) -> Result<PathBuf, AppError> {
    if !is_voice_conversion_enabled() {
        info!("ℹ️ Voice conversion is disabled via ENABLE_VOICE_CONVERSION. Using source audio.");
        tokio::fs::copy(source_audio_path, output_audio_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to copy source audio: {}", e)))?;
        return Ok(output_audio_path.to_path_buf());
    }

    let source_str = source_audio_path
        .to_str()
        .ok_or_else(|| AppError::Internal("Invalid source audio path".to_string()))?;
    let output_str = output_audio_path
        .to_str()
        .ok_or_else(|| AppError::Internal("Invalid output audio path".to_string()))?;

    let url = format!("{}/api/v1/voice/convert", voice_service_url());
    let payload = VoiceConvertRequest {
        source_audio_path: source_str,
        output_audio_path: output_str,
        voice,
        enable_demucs,
    };

    info!(
        voice = %voice,
        demucs = %enable_demucs,
        "🎙️ Sending voice conversion request to {}...",
        url
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| AppError::Internal(format!("Failed to build HTTP client: {}", e)))?;

    let response = match client.post(&url).json(&payload).send().await {
        Ok(resp) => resp,
        Err(e) => {
            warn!(
                "⚠️ Voice service connection failed ({}). Gracefully falling back to source audio.",
                e
            );
            tokio::fs::copy(source_audio_path, output_audio_path)
                .await
                .map_err(|err| {
                    AppError::Internal(format!("Failed to copy source audio on fallback: {}", err))
                })?;
            return Ok(output_audio_path.to_path_buf());
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let err_body = response.text().await.unwrap_or_default();
        warn!(
            "⚠️ Voice service returned error status {}: {}. Falling back to source audio.",
            status, err_body
        );
        tokio::fs::copy(source_audio_path, output_audio_path)
            .await
            .map_err(|err| {
                AppError::Internal(format!("Failed to copy source audio on fallback: {}", err))
            })?;
        return Ok(output_audio_path.to_path_buf());
    }

    let result: VoiceConvertResponse = match response.json().await {
        Ok(r) => r,
        Err(e) => {
            warn!(
                "⚠️ Failed to parse voice service response ({}). Falling back to source audio.",
                e
            );
            tokio::fs::copy(source_audio_path, output_audio_path)
                .await
                .map_err(|err| {
                    AppError::Internal(format!("Failed to copy source audio on fallback: {}", err))
                })?;
            return Ok(output_audio_path.to_path_buf());
        }
    };

    info!(
        status = %result.status,
        voice_applied = ?result.voice_applied,
        demucs = %result.demucs_applied,
        "✅ Voice service response: {}",
        result.message.as_deref().unwrap_or("")
    );

    if tokio::fs::try_exists(output_audio_path)
        .await
        .unwrap_or(false)
    {
        Ok(output_audio_path.to_path_buf())
    } else {
        warn!(
            "⚠️ Expected output file {:?} was not produced by voice service. Copying source audio.",
            output_audio_path
        );
        tokio::fs::copy(source_audio_path, output_audio_path)
            .await
            .map_err(|e| {
                AppError::Internal(format!("Failed to copy source audio on fallback: {}", e))
            })?;
        Ok(output_audio_path.to_path_buf())
    }
}
