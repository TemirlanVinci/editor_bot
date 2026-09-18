use crate::error::AppError;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use tokio::process::Command;
use tracing::{error, info};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Слово с таймингом в секундах, отсчитанным от начала аудиофрагмента
/// (исходная, ещё не ускоренная временная шкала).
#[derive(Debug, Clone)]
pub struct TimedWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Максимум слов в одной "строке" субтитров (1 — вывод по одному слову на экран).
const MAX_WORDS_PER_CHUNK: usize = 1;
/// Если пауза между словами больше этого значения (сек) — считаем это
/// границей фразы и начинаем новую строку субтитров.
const MAX_GAP_SECONDS: f64 = 0.5;

/// Путь к ggml-модели Whisper. Переопределяется через WHISPER_MODEL_PATH.
fn model_path() -> PathBuf {
    env::var("WHISPER_MODEL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/app/models/ggml-small.bin"))
}

/// Язык распознавания речи (ISO 639-1). Переопределяется через WHISPER_LANGUAGE.
pub fn default_language() -> String {
    env::var("WHISPER_LANGUAGE").unwrap_or_else(|_| "ru".to_string())
}

static MODEL: OnceLock<Result<Arc<WhisperContext>, String>> = OnceLock::new();

fn get_whisper_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Возвращает разделяемый (одна на весь процесс) контекст Whisper.
/// Модель грузится с диска лениво, один раз, при первом вызове.
pub fn get_context() -> Result<Arc<WhisperContext>, AppError> {
    let result = MODEL.get_or_init(|| {
        let path = model_path();
        info!("🧠 Loading Whisper model from {:?}", path);

        let path_str = match path.to_str() {
            Some(s) => s,
            None => return Err("Invalid Whisper model path (not valid UTF-8)".to_string()),
        };

        WhisperContext::new_with_params(path_str, WhisperContextParameters::default())
            .map(Arc::new)
            .map_err(|e| format!("Failed to load Whisper model from {:?}: {}", path, e))
    });

    result.clone().map_err(AppError::Validation)
}

/// Конвертирует произвольный аудиофайл в 16kHz mono WAV (формат, нужный Whisper'у).
async fn convert_to_wav(input_path: &Path, output_path: &Path) -> Result<(), AppError> {
    let output = Command::new("ffmpeg")
        .stdin(Stdio::null())
        .args(["-nostdin", "-y", "-i"])
        .arg(input_path)
        .args(["-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", "-f", "wav"])
        .arg(output_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            AppError::Validation(format!("Failed to execute ffmpeg (wav convert): {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("FFmpeg WAV conversion error: {}", stderr);
        return Err(AppError::Validation(
            "Failed to convert audio to WAV for transcription".to_string(),
        ));
    }

    Ok(())
}

/// Читает 16-bit mono WAV и возвращает нормализованные f32-сэмплы для Whisper.
fn read_wav_samples(path: &Path) -> Result<Vec<f32>, AppError> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| AppError::Validation(format!("Failed to open WAV file: {}", e)))?;

    let spec = reader.spec();
    if spec.channels != 1 {
        return Err(AppError::Validation(format!(
            "Expected mono WAV for transcription, got {} channels",
            spec.channels
        )));
    }

    let samples: Vec<i16> = reader
        .samples::<i16>()
        .collect::<Result<Vec<i16>, _>>()
        .map_err(|e| AppError::Validation(format!("Failed to read WAV samples: {}", e)))?;

    Ok(samples.iter().map(|&s| s as f32 / 32768.0).collect())
}

/// Расщепляет элементы TimedWord, если в text содержалось несколько слов
/// (например, если whisper вернул весь сегмент разом), пропорционально распределяя тайминги.
fn split_multi_word_tokens(words: &[TimedWord]) -> Vec<TimedWord> {
    let mut result = Vec::new();

    for tw in words {
        let sub_words: Vec<&str> = tw.text.split_whitespace().collect();
        if sub_words.len() <= 1 {
            result.push(tw.clone());
            continue;
        }

        let total_duration = tw.end - tw.start;
        let word_duration = if total_duration > 0.0 {
            total_duration / (sub_words.len() as f64)
        } else {
            0.0
        };

        for (i, &w) in sub_words.iter().enumerate() {
            let start = tw.start + (i as f64 * word_duration);
            let end = start + word_duration;
            result.push(TimedWord {
                text: w.to_string(),
                start,
                end,
            });
        }
    }

    result
}

/// Распознаёт речь в аудиофрагменте и возвращает список слов с таймингами
/// (тайминги — на исходной, ещё не ускоренной шкале времени фрагмента).
pub async fn transcribe_words(
    ctx: Arc<WhisperContext>,
    audio_path: &Path,
    language: &str,
) -> Result<Vec<TimedWord>, AppError> {
    // Сериализуем транскрибацию Whisper: whisper.cpp / GGML не потокобезопасен
    // при одновременном запуске нескольких процессов распознавания на одном контексте.
    let _guard = get_whisper_lock().lock().await;

    let wav_path = audio_path.with_extension("wav");
    convert_to_wav(audio_path, &wav_path).await?;

    let language = language.to_string();
    let wav_path_for_blocking = wav_path.clone();

    let words = tokio::task::spawn_blocking(move || -> Result<Vec<TimedWord>, AppError> {
        let samples = read_wav_samples(&wav_path_for_blocking)?;

        let mut state = ctx
            .create_state()
            .map_err(|e| AppError::Validation(format!("Failed to create Whisper state: {}", e)))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(&language));
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        // Ключевая связка для пословных таймкодов (пословная разбивка):
        params.set_token_timestamps(true);
        params.set_split_on_word(true);
        params.set_max_len(1); // 1 = принудительно резать сегменты по 1 слову
        params.set_n_threads(
            std::thread::available_parallelism()
                .map(|n| n.get() as i32)
                .unwrap_or(4)
                .min(4),
        );

        state
            .full(params, &samples)
            .map_err(|e| AppError::Validation(format!("Whisper transcription failed: {}", e)))?;

        let mut raw_words = Vec::new();
        for segment in state.as_iter() {
            let text = segment
                .to_str()
                .map_err(|e| AppError::Validation(format!("Failed to get segment text: {}", e)))?;

            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }

            // start_timestamp()/end_timestamp() у whisper.cpp — в центисекундах (1/100 сек).
            raw_words.push(TimedWord {
                text: trimmed.to_lowercase(),
                start: segment.start_timestamp() as f64 / 100.0,
                end: segment.end_timestamp() as f64 / 100.0,
            });
        }

        // Защитный слой: гарантируем, что даже слипшиеся токены разобьются на отдельные слова
        Ok(split_multi_word_tokens(&raw_words))
    })
    .await
    .map_err(|e| AppError::Validation(format!("Transcription task panicked: {}", e)))??;

    if let Err(e) = tokio::fs::remove_file(&wav_path).await {
        error!("Failed to remove temporary WAV file {:?}: {}", wav_path, e);
    }

    Ok(words)
}

/// Строит .ass-субтитры в караоке-стиле (подсветка слова по мере произнесения),
/// выводимые по одному слову по центру кадра с анимацией смены.
///
/// `speed_factor` — во сколько раз видео/аудио ускоряется при рендере
/// (см. render::SPEED_FACTOR); тайминги слов делятся на него, чтобы
/// субтитры совпадали с уже ускоренной дорожкой.
pub fn build_karaoke_ass(
    raw_words: &[TimedWord],
    width: u32,
    height: u32,
    speed_factor: f64,
) -> String {
    // Вторичная проверка разбивки слов
    let words = split_multi_word_tokens(raw_words);

    let font_size = ((height as f64) * 0.05).round().clamp(28.0, 140.0) as i64;
    let outline = ((font_size as f64) / 16.0).ceil().max(2.0) as i64;
    let shadow = (outline / 2).max(1);
    let margin_h = ((width as f64) * 0.06).round() as i64;

    let mut ass = String::new();
    ass.push_str("[Script Info]\n");
    ass.push_str("Title: Karaoke Subtitles\n");
    ass.push_str("ScriptType: v4.00+\n");
    ass.push_str(&format!("PlayResX: {}\n", width));
    ass.push_str(&format!("PlayResY: {}\n", height));
    ass.push_str("ScaledBorderAndShadow: yes\n");
    ass.push_str("WrapStyle: 0\n\n");

    ass.push_str("[V4+ Styles]\n");
    ass.push_str(
        "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n",
    );
    // PrimaryColour = цвет ПОСЛЕ подсветки (жёлтый), SecondaryColour = цвет ДО подсветки (белый).
    // Формат цвета ASS: &HAABBGGRR.
    ass.push_str(&format!(
        "Style: Karaoke,Noto Sans,{font_size},&H0000FFFF,&H00FFFFFF,&H00000000,&H64000000,-1,0,0,0,100,100,0,0,1,{outline},{shadow},5,{margin_h},{margin_h},40,1\n\n"
    ));

    ass.push_str("[Events]\n");
    ass.push_str(
        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
    );

    let scaled: Vec<TimedWord> = words
        .into_iter()
        .filter(|w| !w.text.trim().is_empty())
        .map(|w| TimedWord {
            text: w.text,
            start: w.start / speed_factor,
            end: w.end / speed_factor,
        })
        .collect();

    let mut chunks: Vec<Vec<TimedWord>> = Vec::new();
    let mut current: Vec<TimedWord> = Vec::new();

    for word in scaled {
        if let Some(last) = current.last() {
            let gap = word.start - last.end;
            if current.len() >= MAX_WORDS_PER_CHUNK || gap > MAX_GAP_SECONDS {
                chunks.push(std::mem::take(&mut current));
            }
        }
        current.push(word);
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    for chunk in chunks {
        if chunk.is_empty() {
            continue;
        }

        let chunk_start = chunk[0].start.max(0.0);
        let chunk_end = chunk[chunk.len() - 1].end.max(chunk_start + 0.05);

        let mut cursor = chunk_start;
        let mut text = String::new();
        for word in &chunk {
            let word_end = word.end.max(cursor);
            let duration_cs = ((word_end - cursor) * 100.0).round().max(1.0) as i64;
            text.push_str(&format!(
                "{{\\kf{}}}{}",
                duration_cs,
                sanitize_ass_text(&word.text)
            ));
            cursor = word_end;
        }

        // {\an5\fad(40,40)\fscx82\fscy82\t(0,90,\fscx100\fscy100)}:
        // Текст по центру (\an5), быстрый плавная прозрачность (\fad),
        // легкий pop-in масштабирование с 82% до 100% за первые 90мс (\t).
        ass.push_str(&format!(
            "Dialogue: 0,{},{},Karaoke,,0,0,0,,{{\\an5\\fad(40,40)\\fscx82\\fscy82\\t(0,90,\\fscx100\\fscy100)}}{}\n",
            format_ass_time(chunk_start),
            format_ass_time(chunk_end),
            text
        ));
    }

    ass
}

/// Убирает символы, ломающие ASS override-теги ({ } \) и переносы строк.
fn sanitize_ass_text(text: &str) -> String {
    text.replace(['\\', '{', '}'], "").replace('\n', " ")
}

/// Форматирует секунды в таймкод ASS: H:MM:SS.CS
fn format_ass_time(seconds: f64) -> String {
    let total_cs = (seconds.max(0.0) * 100.0).round() as i64;
    let cs = total_cs % 100;
    let total_secs = total_cs / 100;
    let s = total_secs % 60;
    let total_mins = total_secs / 60;
    let m = total_mins % 60;
    let h = total_mins / 60;
    format!("{}:{:02}:{:02}.{:02}", h, m, s, cs)
}

/// Экранирует путь для безопасной подстановки в filtergraph ffmpeg
/// (фильтр `ass='...'`).
pub fn escape_ffmpeg_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_ass_time() {
        assert_eq!(format_ass_time(0.0), "0:00:00.00");
        assert_eq!(format_ass_time(65.5), "0:01:05.50");
        assert_eq!(format_ass_time(3661.25), "1:01:01.25");
    }

    #[test]
    fn test_split_multi_word_tokens() {
        let multi = vec![TimedWord {
            text: "Привет весь мир".to_string(),
            start: 0.0,
            end: 0.9,
        }];
        let split = split_multi_word_tokens(&multi);
        assert_eq!(split.len(), 3);
        assert_eq!(split[0].text, "Привет");
        assert_eq!(split[1].text, "весь");
        assert_eq!(split[2].text, "мир");
        assert!((split[0].end - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_build_karaoke_ass_contains_dialogue() {
        let words = vec![
            TimedWord {
                text: "Привет".to_string(),
                start: 0.0,
                end: 0.4,
            },
            TimedWord {
                text: "мир".to_string(),
                start: 0.45,
                end: 0.8,
            },
        ];
        let ass = build_karaoke_ass(&words, 1080, 1920, 1.08);
        assert!(ass.contains("[Events]"));
        assert!(ass.contains("Dialogue:"));
        assert!(ass.contains("Привет"));
        assert!(ass.contains("мир"));
        assert!(ass.contains("\\kf"));
        assert!(ass.contains("\\fad"));
        assert!(ass.contains("\\fscx82"));
    }

    #[test]
    fn test_build_karaoke_ass_splits_on_gap() {
        let words = vec![
            TimedWord {
                text: "Первая".to_string(),
                start: 0.0,
                end: 0.4,
            },
            TimedWord {
                text: "фраза".to_string(),
                start: 0.45,
                end: 0.9,
            },
            TimedWord {
                text: "Вторая".to_string(),
                start: 3.0,
                end: 3.4,
            },
        ];
        let ass = build_karaoke_ass(&words, 1080, 1920, 1.0);
        let dialogue_count = ass.matches("Dialogue:").count();
        assert_eq!(dialogue_count, 3);
    }

    #[test]
    fn test_build_karaoke_ass_splits_per_word() {
        let words = vec![
            TimedWord {
                text: "раз".into(),
                start: 0.0,
                end: 0.2,
            },
            TimedWord {
                text: "два".into(),
                start: 0.2,
                end: 0.4,
            },
            TimedWord {
                text: "три".into(),
                start: 0.4,
                end: 0.6,
            },
            TimedWord {
                text: "четыре".into(),
                start: 0.6,
                end: 0.8,
            },
        ];
        let ass = build_karaoke_ass(&words, 1080, 1920, 1.0);
        let dialogue_count = ass.matches("Dialogue:").count();
        assert_eq!(dialogue_count, 4);
    }
}
