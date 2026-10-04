use crate::error::AppError;
use crate::services::video::subtitles::FullTranscript;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use tracing::{error, info, warn};

/// Заглавный вопрос или вступительный хук видео (Global Intro / Hook).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobalIntro {
    /// Начало вступительного вопроса в секундах (обычно 0.0)
    pub start_timestamp: f64,
    /// Момент завершения вопроса диктором в секундах
    pub end_timestamp: f64,
    /// Заголовок или текст заглавного вопроса
    pub title: String,
}

/// Отдельная история из видео.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorySegment {
    /// Порядковый номер истории (1, 2, ...)
    pub story_id: usize,
    /// Начало рассказа в секундах
    pub start_timestamp: f64,
    /// Конец рассказа (финальная точка мысли) в секундах
    pub end_timestamp: f64,
    /// Краткий цепляющий заголовок для ролика
    pub title: String,
    /// Флаг, если длинная история была разделена на Part 1 / Part 2
    pub is_part_of_split: bool,
}

/// Нарративный сегмент видео (для обратной совместимости с очередью и ботом).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NarrativeSegment {
    pub segment_id: usize,
    /// Тип сегмента: "hook" или "story"
    pub segment_type: String,
    /// Заголовок/тема сегмента
    pub title: String,
    /// Начало сегмента в секундах
    pub start_timestamp: f64,
    /// Конец сегмента в секундах
    pub end_timestamp: f64,
}

/// План монтажа готового клипа: Интро-заголовок (Hook) + Тело истории (если include_intro = true),
/// либо только Тело истории (если include_intro = false).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipPlan {
    /// Порядковый номер клипа (1, 2, ...)
    pub clip_index: usize,
    /// Итоговый заголовок клипа
    pub title: String,
    /// Отрезок А: Global Intro (None, если include_intro == false)
    pub intro: Option<GlobalIntro>,
    /// Отрезок Б: Story
    pub story: StorySegment,
    /// Суммарная длительность исходных отрезков (intro + story или только story)
    pub total_duration: f64,
}

/// Результат семантической нарезки транскрипта.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SegmentationResult {
    pub global_intro: GlobalIntro,
    pub stories: Vec<StorySegment>,
    /// Список сегментов для совместимости с queue.rs и downstream потребителями.
    #[serde(default)]
    pub segments: Vec<NarrativeSegment>,
}

/// Длительность паузы по умолчанию (в секундах) между вводным заголовком и началом истории.
pub const DEFAULT_INTRO_PAUSE_DURATION: f64 = 0.5;

/// Возвращает длительность паузы между заголовком и историей (из переменной INTRO_PAUSE_SECS или дефолт 0.5с).
pub fn intro_pause_duration() -> f64 {
    std::env::var("INTRO_PAUSE_SECS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(DEFAULT_INTRO_PAUSE_DURATION)
}

impl SegmentationResult {
    /// Синхронизирует `segments` для обратной совместимости на основе stories.
    pub fn sync_legacy_segments(&mut self) {
        self.segments = self
            .stories
            .iter()
            .map(|s| NarrativeSegment {
                segment_id: s.story_id,
                segment_type: "story".to_string(),
                title: s.title.clone(),
                start_timestamp: s.start_timestamp,
                end_timestamp: s.end_timestamp,
            })
            .collect();
    }

    /// Формирует планы монтажа клипов:
    /// - Если include_intro == true: каждый клип состоит из [global_intro] + [пауза 0.5с] + [story].
    /// - Если include_intro == false: каждый клип состоит только из [story].
    pub fn build_clip_plans(&self, include_intro: bool) -> Vec<ClipPlan> {
        let pause_dur = if include_intro {
            intro_pause_duration()
        } else {
            0.0
        };

        self.stories
            .iter()
            .enumerate()
            .map(|(idx, story)| {
                let story_dur = (story.end_timestamp - story.start_timestamp).max(0.0);
                if include_intro {
                    let intro_dur = (self.global_intro.end_timestamp
                        - self.global_intro.start_timestamp)
                        .max(0.0);
                    ClipPlan {
                        clip_index: idx + 1,
                        title: story.title.clone(),
                        intro: Some(self.global_intro.clone()),
                        story: story.clone(),
                        total_duration: intro_dur + pause_dur + story_dur,
                    }
                } else {
                    ClipPlan {
                        clip_index: idx + 1,
                        title: story.title.clone(),
                        intro: None,
                        story: story.clone(),
                        total_duration: story_dur,
                    }
                }
            })
            .collect()
    }
}

/// Конфигурация для взаимодействия с локальной LLM.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub api_url: String,
    pub model_name: String,
    pub api_key: Option<String>,
    pub timeout_secs: u64,
    pub num_ctx: u32,
    pub temperature: f64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        let api_url = env::var("LLM_API_URL").unwrap_or_else(|_| "http://llm:11434".to_string());
        let model_name = env::var("QWEN_MODEL_NAME")
            .or_else(|_| env::var("Qwen_MODEL_NAME"))
            .or_else(|_| env::var("LLM_MODEL_NAME"))
            .unwrap_or_else(|_| "qwen2.5:3b".to_string());
        let api_key = env::var("LLM_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty());
        let timeout_secs = env::var("LLM_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(120);
        let num_ctx = env::var("LLM_NUM_CTX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8192);
        let temperature = env::var("LLM_TEMPERATURE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.1);

        Self {
            api_url,
            model_name,
            api_key,
            timeout_secs,
            num_ctx,
            temperature,
        }
    }
}

/// Строгая JSON-схема для гарантии валидного ответа через GBNF-грамматику Ollama/OpenAI.
pub fn segmentation_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "global_intro": {
                "type": "object",
                "properties": {
                    "start_timestamp": { "type": "number" },
                    "end_timestamp": { "type": "number" },
                    "title": { "type": "string" }
                },
                "required": ["start_timestamp", "end_timestamp", "title"]
            },
            "stories": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "story_id": { "type": "integer" },
                        "start_timestamp": { "type": "number" },
                        "end_timestamp": { "type": "number" },
                        "title": { "type": "string" },
                        "is_part_of_split": { "type": "boolean" }
                    },
                    "required": ["story_id", "start_timestamp", "end_timestamp", "title", "is_part_of_split"]
                }
            }
        },
        "required": ["global_intro", "stories"]
    })
}

/// Системный промпт для семантической нарезки видео.
pub fn build_system_prompt() -> &'static str {
    r#"You are an expert video editor and narrative analyzer specializing in short-form content (TikTok, YouTube Shorts, Reels) for Reddit narration stories.
Your task is to analyze the provided video transcript with sentence timestamps and speech pauses, then segment the video into a global question/hook intro and standalone stories.

Target Structure:
1. `global_intro` (Reddit Post Title / Opening Hook Header):
   - The opening seconds of the video are ALWAYS the title/hook of the Reddit post or discussion topic.
   - Common patterns (in Russian & English):
     * AITA / МЛЯ questions: "Мудак ли я из-за того что сказал это своей девушке?", "МЛЯ за то что...", "AITA for..."
     * Story hooks & secrets: "Случайно узнал тайный секрет своей девушки и...", "Я совершил ужасную ошибку...", "История о том, как я..."
     * AskReddit questions / prompts: "Расскажите о случаях когда вы...", "Что вы сделали, когда узнали...", "What is the creepiest thing you've experienced?"
     * Catchy discussion openers or Reddit thread titles.
   - DURATION & BOUNDARY RULES:
     * The title is typically short: 3.0 to 10.0 seconds (very rarely up to 12.0 seconds). Starts at 0.0s.
     * Find the EXACT end timestamp where the narrator finishes reading ONLY the title sentence/question.
     * STRICT NEGATIVE RULE: DO NOT capture the story body! The story body starts immediately when the narrator begins setting the scene or giving details (e.g., "Итак...", "Мне 25 лет...", "Всё началось...", "Это произошло...", "Я парень, мне...", "Моя девушка...", "Первая история:...", "Первый комментарий:...").
     * The moment the narrator starts detailing the situation, the title has already ended!

2. `stories` (Story Clips):
   - In 10-20 minute compilation videos, there is either one long story or multiple distinct stories/comments.
   - For multiple stories: segment each separate story at its thematic boundary into an individual clip (target duration: 60 to 120 seconds).
   - For a single long story: split it into cohesive, dramatic parts (Part 1, Part 2, Part 3...) of 60 to 100 seconds at suspense points or cliffhangers, and set `is_part_of_split: true`.
   - Do NOT cut mid-sentence! The end timestamp must correspond to a natural speech pause after the resolution/climax.
   - The first story starts immediately where the narrator starts telling the story (strictly at or right after `global_intro.end_timestamp`).

Critical Timestamp Rules:
- Choose timestamps grounded in the provided transcript lines: `[start - end] (pause: ...)`.
- Stories must be chronological and continuous without overlapping.

Example 1 (Reddit Story / AITA):
Transcript:
[0.00s - 4.50s] (pause: 0.80s) Мудак ли я из-за того что сказал это своей девушке?
[5.30s - 12.10s] (pause: 0.40s) Итак, мне 25 лет, моей девушке 23 года. Мы встречаемся уже два года.
[12.50s - 78.40s] (pause: 1.00s) Вчера произошла крупная ссора из-за пустяка, и всё перевернулось с ног на голову.
[79.40s - 148.00s] (pause: 0.00s) В итоге она собрала вещи и ушла, а я остался один думать над своими словами.
Total Video Duration: 148.00s

Output:
{
  "global_intro": {
    "start_timestamp": 0.0,
    "end_timestamp": 4.5,
    "title": "Мудак ли я из-за того что сказал это девушке?"
  },
  "stories": [
    {
      "story_id": 1,
      "start_timestamp": 5.3,
      "end_timestamp": 78.4,
      "title": "Ссора с девушкой (Part 1)",
      "is_part_of_split": true
    },
    {
      "story_id": 2,
      "start_timestamp": 79.4,
      "end_timestamp": 148.0,
      "title": "Она собрала вещи (Part 2)",
      "is_part_of_split": true
    }
  ]
}

Example 2 (AskReddit Multiple Stories):
Transcript:
[0.00s - 5.20s] (pause: 0.90s) Расскажите о самых странных случаях на вашей первой работе.
[6.10s - 75.30s] (pause: 1.10s) Первая история: Когда мне было 18, я устроился ночным сторожем на склад старой мебели...
[76.40s - 150.00s] (pause: 0.00s) Вторая история: Моя первая работа была баристой в крошечной кофейне на вокзале...
Total Video Duration: 150.00s

Output:
{
  "global_intro": {
    "start_timestamp": 0.0,
    "end_timestamp": 5.2,
    "title": "Странные случаи на первой работе"
  },
  "stories": [
    {
      "story_id": 1,
      "start_timestamp": 6.1,
      "end_timestamp": 75.3,
      "title": "Ночной сторож на складе",
      "is_part_of_split": false
    },
    {
      "story_id": 2,
      "start_timestamp": 76.4,
      "end_timestamp": 150.0,
      "title": "Бариста в кофейне на вокзале",
      "is_part_of_split": false
    }
  ]
}"#
}

/// Извлекает JSON из ответа модели, поддерживая как новый формат (global_intro + stories),
/// так и legacy-формат (segments).
pub fn parse_llm_json(raw_text: &str) -> Result<SegmentationResult, String> {
    let trimmed = raw_text.trim();

    // Проверяем наличие кодовых блоков markdown
    let json_str = if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + 7..];
        if let Some(end) = after.find("```") {
            after[..end].trim()
        } else {
            after.trim()
        }
    } else if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        if let Some(end) = after.find("```") {
            after[..end].trim()
        } else {
            after.trim()
        }
    } else if let (Some(first_brace), Some(last_brace)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if last_brace >= first_brace {
            &trimmed[first_brace..=last_brace]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RawParsed {
        NewFormat {
            global_intro: GlobalIntro,
            stories: Vec<StorySegment>,
            #[serde(default)]
            segments: Vec<NarrativeSegment>,
        },
        LegacyFormat {
            segments: Vec<NarrativeSegment>,
        },
    }

    match serde_json::from_str::<RawParsed>(json_str) {
        Ok(RawParsed::NewFormat {
            global_intro,
            stories,
            mut segments,
        }) => {
            if segments.is_empty() {
                segments = stories
                    .iter()
                    .map(|s| NarrativeSegment {
                        segment_id: s.story_id,
                        segment_type: "story".to_string(),
                        title: s.title.clone(),
                        start_timestamp: s.start_timestamp,
                        end_timestamp: s.end_timestamp,
                    })
                    .collect();
            }
            Ok(SegmentationResult {
                global_intro,
                stories,
                segments,
            })
        }
        Ok(RawParsed::LegacyFormat { segments }) => {
            if segments.is_empty() {
                return Err("Empty segments array in LLM response".to_string());
            }
            let first = &segments[0];
            let global_intro = GlobalIntro {
                start_timestamp: first.start_timestamp,
                end_timestamp: first.end_timestamp,
                title: first.title.clone(),
            };
            let stories = segments[1..]
                .iter()
                .enumerate()
                .map(|(idx, s)| StorySegment {
                    story_id: idx + 1,
                    start_timestamp: s.start_timestamp,
                    end_timestamp: s.end_timestamp,
                    title: s.title.clone(),
                    is_part_of_split: false,
                })
                .collect();
            let mut res = SegmentationResult {
                global_intro,
                stories,
                segments: Vec::new(),
            };
            res.sync_legacy_segments();
            Ok(res)
        }
        Err(e) => Err(format!(
            "Failed to parse SegmentationResult JSON: {e}. Raw: {json_str}"
        )),
    }
}

/// Минимальная длительность истории для предотвращения создания слишком коротких роликов (35 сек).
pub const MIN_STORY_DURATION: f64 = 35.0;

/// Максимальная длительность клипа (120 сек) для формата Shorts / TikTok / Reels
/// и гарантированного соблюдения лимита Telegram (50 МБ).
pub const MAX_STORY_DURATION: f64 = 120.0;

/// Целевая длительность части при разбиении длинных историй (75 сек).
pub const TARGET_STORY_SPLIT_DURATION: f64 = 75.0;

/// Валидирует и исправляет разметку от LLM (Defensive Post-Processing):
/// 1. Привязка границ (snapping) к реальным паузам речи Whisper.
/// 2. Защита от наложения: story.start >= global_intro.end.
/// 3. Склейка коротких историй (< 35s) со следующим (или предыдущим) сегментом.
/// 4. Разделение чрезмерно длинных историй (> 120s) на связные части (Part 1, Part 2, ...).
pub fn validate_and_sanitize_segmentation(
    mut result: SegmentationResult,
    total_duration: f64,
    candidate_cut_points: Option<&[f64]>,
) -> SegmentationResult {
    let total_duration = total_duration.max(1.0);

    // 1. Валидация и snapping для global_intro
    result.global_intro.start_timestamp = 0.0;

    if result.global_intro.end_timestamp <= 1.0
        || result.global_intro.end_timestamp > 15.0
        || result.global_intro.end_timestamp > total_duration
    {
        result.global_intro.end_timestamp =
            (total_duration * 0.05).clamp(3.5, 10.0).min(total_duration);
    }

    if let Some(cuts) = candidate_cut_points {
        let target = result.global_intro.end_timestamp;
        let mut best_cut = None;
        let mut min_diff = 0.8;

        for &cut in cuts {
            if (2.0..=15.0).contains(&cut) && cut < total_duration - 5.0 {
                let diff = (cut - target).abs();
                if diff < min_diff {
                    min_diff = diff;
                    best_cut = Some(cut);
                }
            }
        }

        if let Some(snapped) = best_cut {
            result.global_intro.end_timestamp = snapped;
        }
    }

    result.global_intro.end_timestamp = result
        .global_intro
        .end_timestamp
        .clamp(1.0, (total_duration - 1.0).max(1.0));

    if result.global_intro.title.trim().is_empty() {
        result.global_intro.title = "Global Intro".to_string();
    }

    // 2. Обработка stories: если пусто, создаем дефолтную историю
    if result.stories.is_empty() {
        result.stories.push(StorySegment {
            story_id: 1,
            start_timestamp: result.global_intro.end_timestamp,
            end_timestamp: total_duration,
            title: "Story 1".to_string(),
            is_part_of_split: false,
        });
    }

    // Сортируем истории по времени старта
    result.stories.sort_by(|a, b| {
        a.start_timestamp
            .partial_cmp(&b.start_timestamp)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 3. Защита от наложения и обеспечение непрерывности
    let mut current_start = result.global_intro.end_timestamp;
    let num_stories = result.stories.len();
    for (idx, story) in result.stories.iter_mut().enumerate() {
        story.start_timestamp = current_start;

        if story.end_timestamp <= story.start_timestamp {
            story.end_timestamp = (story.start_timestamp + 45.0).min(total_duration);
        }

        // Snapping к паузам Whisper для промежуточных историй
        let is_last = idx + 1 == num_stories;
        if !is_last && let Some(cuts) = candidate_cut_points {
            let target = story.end_timestamp;
            let mut best_cut = None;
            let mut min_diff = 2.5;

            for &cut in cuts {
                if cut > story.start_timestamp + 5.0 && cut < total_duration - 5.0 {
                    let diff = (cut - target).abs();
                    if diff < min_diff {
                        min_diff = diff;
                        best_cut = Some(cut);
                    }
                }
            }

            if let Some(snapped) = best_cut {
                story.end_timestamp = snapped;
            }
        }

        current_start = story.end_timestamp;
    }

    // 4. Склейка коротких фрагментов (Merge short stories < MIN_STORY_DURATION)
    let mut merged: Vec<StorySegment> = Vec::new();
    let mut i = 0;
    while i < result.stories.len() {
        let mut cur = result.stories[i].clone();

        // Если текущий сегмент короче минимальной длительности и есть следующий сегмент,
        // объединяем его со следующим (сдвигая end_timestamp до конца следующего сегмента)
        while (cur.end_timestamp - cur.start_timestamp) < MIN_STORY_DURATION
            && i + 1 < result.stories.len()
        {
            let next = &result.stories[i + 1];
            cur.end_timestamp = next.end_timestamp;
            cur.is_part_of_split = cur.is_part_of_split || next.is_part_of_split;
            if cur.title.trim().is_empty() || cur.title.starts_with("Story") {
                cur.title = next.title.clone();
            }
            i += 1;
        }

        merged.push(cur);
        i += 1;
    }

    // Если последний оставшийся сегмент всё ещё короче 35 секунд и есть предыдущий,
    // склеиваем его с предыдущим
    if merged.len() > 1 {
        let last_idx = merged.len() - 1;
        if (merged[last_idx].end_timestamp - merged[last_idx].start_timestamp) < MIN_STORY_DURATION
        {
            let last = merged.pop().unwrap();
            let prev = merged.last_mut().unwrap();
            prev.end_timestamp = last.end_timestamp;
            prev.is_part_of_split = prev.is_part_of_split || last.is_part_of_split;
        }
    }

    // 5. Разделение чрезмерно длинных историй (> MAX_STORY_DURATION) на связные части (Part 1, Part 2, ...)
    let mut split_stories: Vec<StorySegment> = Vec::new();
    for story in merged {
        let dur = story.end_timestamp - story.start_timestamp;
        if dur <= MAX_STORY_DURATION {
            split_stories.push(story);
            continue;
        }

        let mut cur_start = story.start_timestamp;
        let story_end = story.end_timestamp;
        let mut part_num = 1;
        let base_title = story
            .title
            .trim_end_matches(|c: char| c.is_whitespace() || c == '.' || c == ':')
            .to_string();

        while cur_start < story_end {
            let remaining = story_end - cur_start;
            if remaining <= MAX_STORY_DURATION {
                let title = if part_num > 1 || story.is_part_of_split {
                    format!("{} (Part {})", base_title, part_num)
                } else {
                    story.title.clone()
                };
                split_stories.push(StorySegment {
                    story_id: split_stories.len() + 1,
                    start_timestamp: cur_start,
                    end_timestamp: story_end,
                    title,
                    is_part_of_split: part_num > 1 || story.is_part_of_split,
                });
                break;
            }

            let target_cut = cur_start + TARGET_STORY_SPLIT_DURATION;
            let mut best_cut = target_cut;

            if let Some(cuts) = candidate_cut_points {
                let mut min_diff = f64::MAX;
                for &cut in cuts {
                    if cut >= cur_start + 45.0 && cut <= cur_start + 110.0 && cut < story_end - 20.0 {
                        let diff = (cut - target_cut).abs();
                        if diff < min_diff {
                            min_diff = diff;
                            best_cut = cut;
                        }
                    }
                }
            }

            if best_cut <= cur_start + 30.0 || best_cut >= story_end - 15.0 {
                best_cut = (cur_start + TARGET_STORY_SPLIT_DURATION).min(story_end);
            }

            split_stories.push(StorySegment {
                story_id: split_stories.len() + 1,
                start_timestamp: cur_start,
                end_timestamp: best_cut,
                title: format!("{} (Part {})", base_title, part_num),
                is_part_of_split: true,
            });

            cur_start = best_cut;
            part_num += 1;
        }
    }

    // Если всё ещё пусто (крайний случай), создаём единую историю
    if split_stories.is_empty() {
        split_stories.push(StorySegment {
            story_id: 1,
            start_timestamp: result.global_intro.end_timestamp,
            end_timestamp: total_duration,
            title: "Story 1".to_string(),
            is_part_of_split: false,
        });
    }

    // Последний сегмент обязательно растягиваем до total_duration
    if let Some(last) = split_stories.last_mut() {
        last.end_timestamp = total_duration.max(last.start_timestamp + 0.5);
    }

    // Перенумеровываем story_id от 1 до N
    for (idx, story) in split_stories.iter_mut().enumerate() {
        story.story_id = idx + 1;
        if story.title.trim().is_empty() {
            story.title = format!("Story {}", idx + 1);
        }
    }

    result.stories = split_stories;
    result.sync_legacy_segments();
    result
}

/// Валидирует и исправляет сегменты от LLM, гарантируя корректность, непрерывность
/// и привязку границ к реальным паузам/окончаниям предложений (snapping).
pub fn validate_and_sanitize_segments_with_snapping(
    mut segments: Vec<NarrativeSegment>,
    total_duration: f64,
    candidate_cut_points: Option<&[f64]>,
) -> Vec<NarrativeSegment> {
    if segments.is_empty() {
        return vec![NarrativeSegment {
            segment_id: 1,
            segment_type: "story".to_string(),
            title: "Full Story".to_string(),
            start_timestamp: 0.0,
            end_timestamp: total_duration.max(1.0),
        }];
    }

    // Сортируем по времени старта
    segments.sort_by(|a, b| {
        a.start_timestamp
            .partial_cmp(&b.start_timestamp)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut sanitized: Vec<NarrativeSegment> = Vec::new();
    let mut current_start = 0.0;
    let num_segments = segments.len();

    for (i, mut seg) in segments.into_iter().enumerate() {
        let is_last = i + 1 == num_segments;

        // Первый сегмент всегда стартует с 0.0, остальные с конца предыдущего
        seg.start_timestamp = current_start;

        if is_last {
            seg.end_timestamp = total_duration.max(current_start + 0.5);
        } else {
            // Защита от нулевой или отрицательной длительности
            if seg.end_timestamp <= seg.start_timestamp {
                seg.end_timestamp = (seg.start_timestamp + 10.0).min(total_duration);
            }

            // Snapping: привязываем конец сегмента к ближайшему окончанию фразы Whisper
            if let Some(cuts) = candidate_cut_points {
                let target = seg.end_timestamp;
                let mut best_cut = None;
                let mut min_diff = 2.5;

                for &cut in cuts {
                    if cut > current_start + 2.0 && cut < total_duration - 2.0 {
                        let diff = (cut - target).abs();
                        if diff < min_diff {
                            min_diff = diff;
                            best_cut = Some(cut);
                        }
                    }
                }

                if let Some(snapped) = best_cut {
                    seg.end_timestamp = snapped;
                }
            }
        }

        // Валидация типа сегмента
        let lower_type = seg.segment_type.to_lowercase();
        seg.segment_type = if lower_type.contains("hook") || lower_type.contains("teaser") {
            "hook".to_string()
        } else {
            "story".to_string()
        };

        // Защита от сверхкоротких сегментов (менее 2.0 секунд) путем слияния с предыдущим
        if let Some(prev) = sanitized.last_mut()
            && seg.end_timestamp - seg.start_timestamp < 2.0
        {
            prev.end_timestamp = seg.end_timestamp.max(prev.end_timestamp);
            current_start = prev.end_timestamp;
            continue;
        }

        current_start = seg.end_timestamp;
        sanitized.push(seg);
    }

    if sanitized.is_empty() {
        sanitized.push(NarrativeSegment {
            segment_id: 1,
            segment_type: "story".to_string(),
            title: "Full Story".to_string(),
            start_timestamp: 0.0,
            end_timestamp: total_duration,
        });
    }

    // Последний сегмент обязательно растягиваем до total_duration
    if let Some(last) = sanitized.last_mut() {
        last.end_timestamp = total_duration.max(last.start_timestamp + 0.5);
    }

    // Перенумеровываем segment_id от 1 до N
    for (idx, seg) in sanitized.iter_mut().enumerate() {
        seg.segment_id = idx + 1;
        if seg.title.trim().is_empty() {
            seg.title = if seg.segment_type == "hook" {
                format!("Hook {}", idx + 1)
            } else {
                format!("Story {}", idx + 1)
            };
        }
    }

    sanitized
}

/// Валидирует и исправляет сегменты от LLM (обратная совместимость).
pub fn validate_and_sanitize_segments(
    segments: Vec<NarrativeSegment>,
    total_duration: f64,
) -> Vec<NarrativeSegment> {
    validate_and_sanitize_segments_with_snapping(segments, total_duration, None)
}

/// Проверяет, начинается ли текст сегмента с характерных фраз начала тела истории на Реддите.
pub fn is_story_start_text(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    let clean = lower.trim_start_matches(|c: char| {
        c.is_ascii_punctuation() || matches!(c, '«' | '»' | '“' | '”' | '—' | ' ' | '\t')
    });

    clean.starts_with("итак")
        || clean.starts_with("так вот")
        || clean.starts_with("короче")
        || clean.starts_with("в общем")
        || clean.starts_with("всё началось")
        || clean.starts_with("все началось")
        || clean.starts_with("началось всё")
        || clean.starts_with("началось все")
        || clean.starts_with("это произошло")
        || clean.starts_with("это было")
        || clean.starts_with("дело было")
        || clean.starts_with("история произошла")
        || clean.starts_with("первая история")
        || clean.starts_with("первый комментарий")
        || clean.starts_with("история 1")
        || clean.starts_with("история один")
        || clean.starts_with("комментарий 1")
        || clean.starts_with("пользователь пишет")
        || clean.starts_with("автор пишет")
        || clean.starts_with("реддитор пишет")
        || clean.starts_with("один реддитор")
        || clean.starts_with("один пользователь")
        || clean.starts_with("одна девушка")
        || clean.starts_with("один парень")
        || clean.starts_with("недавно я")
        || clean.starts_with("около года назад")
        || clean.starts_with("несколько месяцев назад")
        || clean.starts_with("пару лет назад")
        || clean.starts_with("пару месяцев назад")
        || clean.starts_with("когда мне было")
        || clean.starts_with("моя девушка")
        || clean.starts_with("мой парень")
        || clean.starts_with("мой муж")
        || clean.starts_with("моя жена")
        || clean.starts_with("я парень")
        || clean.starts_with("я девушка")
        || clean.starts_with("я мужчина")
        || clean.starts_with("я женщина")
        || (clean.starts_with("мне ")
            && clean[4..]
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false))
        || clean.starts_with("so, ")
        || clean.starts_with("well, ")
        || clean.starts_with("it all started")
        || clean.starts_with("this happened")
        || clean.starts_with("first story")
        || clean.starts_with("story 1")
        || clean.starts_with("user ")
        || clean.starts_with("i am a ")
        || clean.starts_with("i'm a ")
}

/// Проверяет, похож ли текст сегмента на заголовок / вступительный хук Реддита.
pub fn is_title_text(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    lower.contains("мудак ли я")
        || lower.contains("мля")
        || lower.contains("aita")
        || lower.contains("am i the asshole")
        || lower.contains("тайный секрет")
        || lower.contains("секрет своей девушки")
        || lower.contains("секрет моего парня")
        || lower.contains("узнал тайный секрет")
        || lower.contains("расскажите о")
        || lower.contains("поделитесь историями")
        || lower.contains("что вы сделали")
        || lower.contains("что делать если")
        || lower.contains("истории с реддит")
        || lower.contains("тред реддит")
        || lower.contains("вопрос с реддит")
        || lower.ends_with('?')
        || lower.ends_with("...")
        || lower.ends_with('…')
}

/// Уточняет границу вступительного заголовка (Global Intro) и первой истории:
/// 1. Проверяет сегменты Whisper в окне 0..25с на наличие маркеров заголовка и начала истории.
/// 2. Если модель захватила лишнее (например, включила "Итак, мне 25 лет..."),
///    откатывает границу назад до реального окончания заголовка.
/// 3. Привязывает конец интро к точному окончанию последнего слова заголовка (Whisper words).
/// 4. Привязывает начало первой истории к первому реальному слову истории.
pub fn refine_intro_boundary_with_transcript(
    result: &mut SegmentationResult,
    transcript: &FullTranscript,
) {
    if transcript.segments.is_empty() {
        return;
    }

    let mut detected_intro_end: Option<f64> = None;
    let mut detected_story_start: Option<f64> = None;

    // Сканируем сегменты Whisper в первых 25 секундах видео
    for (i, seg) in transcript.segments.iter().enumerate() {
        if seg.start > 25.0 {
            break;
        }

        // Если есть следующий сегмент, и он начинается со слов истории ("Итак...", "Мне 25 лет...", "Всё началось...")
        if i + 1 < transcript.segments.len() {
            let next_seg = &transcript.segments[i + 1];
            if is_story_start_text(&next_seg.text) && seg.end <= 15.0 && seg.end >= 2.0 {
                info!(
                    "🎯 Found story transition at seg #{}: '{:.2}s' -> next: '{:.2}s' ({})",
                    i, seg.end, next_seg.start, next_seg.text
                );
                detected_intro_end = Some(seg.end);
                detected_story_start = Some(next_seg.start);
                break;
            }
        }

        // Если сегмент заканчивается вопросом '?' или многоточием '...' и имеет паузу после себя
        let trimmed = seg.text.trim();
        let ends_punct = trimmed.ends_with('?') || trimmed.ends_with("...") || trimmed.ends_with('…');
        if ends_punct && seg.pause_after >= 0.20 && seg.end <= 14.0 && seg.end >= 2.0 {
            detected_intro_end = Some(seg.end);
            if i + 1 < transcript.segments.len() {
                detected_story_start = Some(transcript.segments[i + 1].start);
            }
            break;
        }

        // Если первый сегмент явно содержит маркер заголовка ("Мудак ли я...", "Секрет...")
        if i == 0 && is_title_text(&seg.text) && seg.end <= 12.0 && seg.end >= 2.0 {
            detected_intro_end = Some(seg.end);
            if i + 1 < transcript.segments.len() {
                detected_story_start = Some(transcript.segments[i + 1].start);
            }
            break;
        }
    }

    // Применяем обнаруженную границу, если модель ошиблась или вернула слишком длинное значение
    let current_end = result.global_intro.end_timestamp;
    let chosen_end = if let Some(det_end) = detected_intro_end {
        if current_end > 12.0 || (current_end - det_end).abs() > 1.2 {
            info!(
                "✂️ Correcting over-extended LLM intro from {:.2}s to detected title end {:.2}s",
                current_end, det_end
            );
            det_end
        } else {
            det_end
        }
    } else {
        current_end
    };

    // Точная пословная привязка (Sub-second word snapping):
    // Находим точное окончание последнего слова заголовка в transcript.words
    let mut snapped_intro_end = chosen_end;
    let mut best_word_diff = 1.0;

    for w in &transcript.words {
        if w.end <= 20.0 {
            let diff = (w.end - chosen_end).abs();
            if diff < best_word_diff {
                best_word_diff = diff;
                snapped_intro_end = w.end;
            }
        }
    }

    result.global_intro.end_timestamp = snapped_intro_end.clamp(2.0, 15.0);

    // Привязываем начало первой истории к первому произнесенному слову истории
    let target_story_start = detected_story_start.unwrap_or(result.global_intro.end_timestamp);
    let mut actual_first_story_word_start = target_story_start;

    for w in &transcript.words {
        if w.start >= result.global_intro.end_timestamp - 0.05 {
            actual_first_story_word_start = w.start;
            break;
        }
    }

    if let Some(first_story) = result.stories.first_mut() {
        first_story.start_timestamp = actual_first_story_word_start;
    }
}

/// Уточняет начало произнесения речи для каждой истории по таймкодам слов Whisper.
/// Это исключает пустые паузы в начале нарезанных отрезков перед началом речи.
pub fn refine_story_speech_starts(
    result: &mut SegmentationResult,
    transcript: &FullTranscript,
) {
    if transcript.words.is_empty() {
        return;
    }

    for story in &mut result.stories {
        let nominal_start = story.start_timestamp;
        for w in &transcript.words {
            if w.start >= nominal_start - 0.10 && w.start < nominal_start + 3.0 {
                story.start_timestamp = w.start;
                break;
            }
        }
    }
}

/// Умная эвристическая нарезка на случай недоступности или ошибки LLM.
/// Находит заглавный вопрос (хук) и делит оставшуюся историю на логичные части.
pub fn fallback_narrative_segmentation(transcript: &FullTranscript) -> SegmentationResult {
    info!(
        "🛡️ Generating rule-based narrative fallback segmentation for duration {:.2}s",
        transcript.total_duration
    );
    let total_duration = transcript.total_duration.max(1.0);
    let candidate_cut_points: Vec<f64> = transcript.segments.iter().map(|s| s.end).collect();

    if transcript.segments.is_empty() {
        let mut res = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: (total_duration * 0.05).clamp(3.5, 8.0).min(total_duration),
                title: "Opening Question".to_string(),
            },
            stories: vec![StorySegment {
                story_id: 1,
                start_timestamp: (total_duration * 0.05).clamp(3.5, 8.0).min(total_duration),
                end_timestamp: total_duration,
                title: "Story Part 1".to_string(),
                is_part_of_split: false,
            }],
            segments: Vec::new(),
        };
        res.sync_legacy_segments();
        return res;
    }

    // 1. Поиск хука: берем первый сегмент как основу
    let first_seg = &transcript.segments[0];
    let hook_end = first_seg.end.clamp(2.5, 10.0);
    let snippet = first_seg.text.chars().take(50).collect::<String>();
    let hook_title = snippet;

    let mut raw_res = SegmentationResult {
        global_intro: GlobalIntro {
            start_timestamp: 0.0,
            end_timestamp: hook_end,
            title: hook_title,
        },
        stories: Vec::new(),
        segments: Vec::new(),
    };

    // Применяем обнаружение границ заголовка по маркерам и паузам
    refine_intro_boundary_with_transcript(&mut raw_res, transcript);
    let hook_end = raw_res.global_intro.end_timestamp;

    // 2. Нарезка оставшейся части на истории
    let mut stories = Vec::new();
    let remaining_duration = total_duration - hook_end;

    if remaining_duration <= 90.0 {
        stories.push(StorySegment {
            story_id: 1,
            start_timestamp: hook_end,
            end_timestamp: total_duration,
            title: "Story 1".to_string(),
            is_part_of_split: false,
        });
    } else {
        let mut story_start = hook_end;
        let mut story_idx = 1;

        while story_start < total_duration {
            let target_end = story_start + 75.0;
            if target_end >= total_duration - 25.0 {
                stories.push(StorySegment {
                    story_id: stories.len() + 1,
                    start_timestamp: story_start,
                    end_timestamp: total_duration,
                    title: format!("Story Part {story_idx}"),
                    is_part_of_split: false,
                });
                break;
            }

            let mut best_cut = target_end;
            let mut found_pause = false;

            for seg in &transcript.segments {
                if seg.end >= story_start + 45.0 && seg.end <= story_start + 110.0 {
                    if seg.pause_after >= 0.5 {
                        best_cut = seg.end;
                        found_pause = true;
                        break;
                    } else if (seg.end - target_end).abs() < (best_cut - target_end).abs() {
                        best_cut = seg.end;
                    }
                }
            }

            if !found_pause && best_cut > total_duration - 20.0 {
                best_cut = total_duration;
            }

            stories.push(StorySegment {
                story_id: stories.len() + 1,
                start_timestamp: story_start,
                end_timestamp: best_cut,
                title: format!("Story Part {story_idx}"),
                is_part_of_split: false,
            });

            story_start = best_cut;
            story_idx += 1;
        }
    }

    raw_res.stories = stories;

    let mut res =
        validate_and_sanitize_segmentation(raw_res, total_duration, Some(&candidate_cut_points));
    refine_story_speech_starts(&mut res, transcript);
    res
}

/// Выполняет запрос к Ollama API (/api/chat) или OpenAI-совместимому эндпоинту (/v1/chat/completions).
async fn query_llm_api(
    client: &reqwest::Client,
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let base_url = config.api_url.trim_end_matches('/');
    let is_ollama =
        config.api_key.is_none() && (base_url.contains(":11434") || base_url.contains("ollama"));

    // Попытка 1: Нативный Ollama API (/api/chat) со строгой JSON-схемой и опциями контекста
    if is_ollama {
        let ollama_url = format!("{base_url}/api/chat");
        let ollama_body = serde_json::json!({
            "model": config.model_name,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "stream": false,
            "format": segmentation_json_schema(),
            "options": {
                "temperature": config.temperature,
                "num_ctx": config.num_ctx
            }
        });

        info!(
            "🤖 Sending segmentation request to Ollama /api/chat at {} (model: {}, num_ctx: {}, temp: {})...",
            ollama_url, config.model_name, config.num_ctx, config.temperature
        );

        match client
            .post(&ollama_url)
            .header("Content-Type", "application/json")
            .json(&ollama_body)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let res_json: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse Ollama JSON response: {e}"))?;

                if let Some(content) = res_json["message"]["content"].as_str() {
                    return Ok(content.to_string());
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                warn!(
                    "Ollama /api/chat returned status {status}: {body}. Trying OpenAI fallback..."
                );
            }
            Err(e) => {
                warn!("Request to {ollama_url} failed: {e}. Trying OpenAI fallback...");
            }
        }
    }

    // Попытка 2: OpenAI-совместимый эндпоинт (/v1/chat/completions)
    let openai_url = if base_url.ends_with("/v1") {
        format!("{base_url}/chat/completions")
    } else {
        format!("{base_url}/v1/chat/completions")
    };

    let openai_body = serde_json::json!({
        "model": config.model_name,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "temperature": config.temperature,
        "response_format": {"type": "json_object"},
        "options": {
            "num_ctx": config.num_ctx,
            "temperature": config.temperature
        }
    });

    let mut request_builder = client
        .post(&openai_url)
        .header("Content-Type", "application/json")
        .json(&openai_body);

    if let Some(key) = &config.api_key {
        request_builder = request_builder.header("Authorization", format!("Bearer {key}"));
    }

    info!(
        "🤖 Sending segmentation request to OpenAI-compatible endpoint at {} (model: {})...",
        openai_url, config.model_name
    );

    let resp = request_builder
        .send()
        .await
        .map_err(|e| format!("Request to {openai_url} failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("LLM API returned error {status}: {body}"));
    }

    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenAI JSON response: {e}"))?;

    if let Some(content) = res_json["choices"][0]["message"]["content"].as_str() {
        Ok(content.to_string())
    } else {
        Err(format!("Missing content in LLM response: {res_json:?}"))
    }
}

/// Анализирует транскрипт видео с помощью LLM (Qwen) и возвращает структуру сегментов.
/// В случае сбоя или недоступности модели безопасно переключается на эвристический fallback.
pub async fn analyze_narrative(
    transcript: &FullTranscript,
) -> Result<SegmentationResult, AppError> {
    let config = LlmConfig::default();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout_secs))
        .build()
        .map_err(|e| AppError::Internal(format!("Failed to build HTTP client: {e}")))?;

    let system_prompt = build_system_prompt();
    let transcript_context = transcript.to_llm_context();
    let user_prompt = format!(
        "Total Video Duration: {:.2}s\n\n{}\nAnalyze the narrative context and return the structured JSON segmentation.",
        transcript.total_duration, transcript_context
    );

    let candidate_cut_points: Vec<f64> = transcript.segments.iter().map(|s| s.end).collect();
    let start_time = std::time::Instant::now();

    // Запрос к LLM с повторной попыткой при невалидном JSON
    match query_llm_api(&client, &config, system_prompt, &user_prompt).await {
        Ok(raw_reply) => match parse_llm_json(&raw_reply) {
            Ok(mut parsed) => {
                info!(
                    "✅ LLM ({}) segmentation completed in {:.2}s. Received {} story/stories.",
                    config.model_name,
                    start_time.elapsed().as_secs_f32(),
                    parsed.stories.len()
                );
                refine_intro_boundary_with_transcript(&mut parsed, transcript);
                parsed = validate_and_sanitize_segmentation(
                    parsed,
                    transcript.total_duration,
                    Some(&candidate_cut_points),
                );
                refine_story_speech_starts(&mut parsed, transcript);
                return Ok(parsed);
            }
            Err(parse_err) => {
                warn!("LLM output parsing error: {parse_err}. Attempting correction retry...");
                let retry_user_prompt = format!(
                    "Your previous response was not valid JSON:\n{}\n\nError: {}\n\nPlease return ONLY valid JSON matching the schema for this transcript:\n{}",
                    raw_reply, parse_err, user_prompt
                );

                if let Ok(retry_reply) =
                    query_llm_api(&client, &config, system_prompt, &retry_user_prompt).await
                    && let Ok(mut retry_parsed) = parse_llm_json(&retry_reply)
                {
                    info!("✅ LLM segmentation succeeded on retry!");
                    refine_intro_boundary_with_transcript(&mut retry_parsed, transcript);
                    retry_parsed = validate_and_sanitize_segmentation(
                        retry_parsed,
                        transcript.total_duration,
                        Some(&candidate_cut_points),
                    );
                    refine_story_speech_starts(&mut retry_parsed, transcript);
                    return Ok(retry_parsed);
                }
            }
        },
        Err(e) => {
            error!(
                "⚠️ Failed to reach LLM endpoint ({:?}): {e}. Proceeding with graceful fallback.",
                config.api_url
            );
        }
    }

    // Graceful fallback
    info!("🛡️ Using narrative fallback segmentation due to LLM unavailability or parse error.");
    Ok(fallback_narrative_segmentation(transcript))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::video::subtitles::{TimedWord, TranscriptSegment};

    #[test]
    fn test_parse_llm_json_new_schema() {
        let json = r#"{
            "global_intro": {
                "start_timestamp": 0.0,
                "end_timestamp": 8.5,
                "title": "AITA for canceling my wedding?"
            },
            "stories": [
                {
                    "story_id": 1,
                    "start_timestamp": 8.5,
                    "end_timestamp": 75.0,
                    "title": "The Venue Catastrophe",
                    "is_part_of_split": false
                },
                {
                    "story_id": 2,
                    "start_timestamp": 75.0,
                    "end_timestamp": 150.0,
                    "title": "Family Fallout",
                    "is_part_of_split": true
                }
            ]
        }"#;

        let res = parse_llm_json(json).expect("Must parse new schema JSON");
        assert_eq!(res.global_intro.start_timestamp, 0.0);
        assert_eq!(res.global_intro.end_timestamp, 8.5);
        assert_eq!(res.global_intro.title, "AITA for canceling my wedding?");
        assert_eq!(res.stories.len(), 2);
        assert_eq!(res.stories[0].title, "The Venue Catastrophe");
        assert!(!res.stories[0].is_part_of_split);
        assert!(res.stories[1].is_part_of_split);
        assert_eq!(res.segments.len(), 2);
    }

    #[test]
    fn test_parse_llm_json_markdown() {
        let json = r#"Here is the segmentation for the video:
```json
{
    "global_intro": {
        "start_timestamp": 0.0,
        "end_timestamp": 6.5,
        "title": "The Hook"
    },
    "stories": [
        {
            "story_id": 1,
            "start_timestamp": 6.5,
            "end_timestamp": 80.0,
            "title": "First Tale",
            "is_part_of_split": false
        }
    ]
}
```
Hope this helps!"#;

        let res = parse_llm_json(json).expect("Must parse markdown wrapped JSON");
        assert_eq!(res.global_intro.title, "The Hook");
        assert_eq!(res.stories.len(), 1);
        assert_eq!(res.stories[0].title, "First Tale");
    }

    #[test]
    fn test_parse_llm_json_legacy_segments_compatibility() {
        let json = r#"{
            "segments": [
                {
                    "segment_id": 1,
                    "segment_type": "hook",
                    "title": "AITA for wedding",
                    "start_timestamp": 0.0,
                    "end_timestamp": 7.5
                },
                {
                    "segment_id": 2,
                    "segment_type": "story",
                    "title": "Story 1",
                    "start_timestamp": 7.5,
                    "end_timestamp": 60.0
                }
            ]
        }"#;

        let res = parse_llm_json(json).expect("Must parse legacy segments JSON");
        assert_eq!(res.global_intro.end_timestamp, 7.5);
        assert_eq!(res.stories.len(), 1);
        assert_eq!(res.stories[0].title, "Story 1");
        assert_eq!(res.stories[0].start_timestamp, 7.5);
        assert_eq!(res.stories[0].end_timestamp, 60.0);
    }

    #[test]
    fn test_validate_and_sanitize_no_overlap_with_global_intro() {
        let raw = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 10.0,
                title: "Intro Question".into(),
            },
            stories: vec![
                StorySegment {
                    story_id: 1,
                    start_timestamp: 4.0, // Hallucinated overlap: starts inside intro
                    end_timestamp: 60.0,
                    title: "Story 1".into(),
                    is_part_of_split: false,
                },
                StorySegment {
                    story_id: 2,
                    start_timestamp: 55.0, // Overlap with story 1
                    end_timestamp: 120.0,
                    title: "Story 2".into(),
                    is_part_of_split: false,
                },
            ],
            segments: Vec::new(),
        };

        let sanitized = validate_and_sanitize_segmentation(raw, 120.0, None);
        assert_eq!(sanitized.global_intro.start_timestamp, 0.0);
        assert_eq!(sanitized.global_intro.end_timestamp, 10.0);
        assert!(sanitized.stories[0].start_timestamp >= sanitized.global_intro.end_timestamp);
        assert_eq!(sanitized.stories[0].start_timestamp, 10.0);
        assert_eq!(
            sanitized.stories[1].start_timestamp,
            sanitized.stories[0].end_timestamp
        );
        assert_eq!(sanitized.stories.last().unwrap().end_timestamp, 120.0);
    }

    #[test]
    fn test_merge_short_stories_under_35_sec() {
        // Story 1: 10.0 -> 25.0 (15 sec < 35s)
        // Story 2: 25.0 -> 90.0 (65 sec)
        // Story 1 should be merged into Story 2 (with end_timestamp stretched to 90.0)
        let raw = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 10.0,
                title: "Intro".into(),
            },
            stories: vec![
                StorySegment {
                    story_id: 1,
                    start_timestamp: 10.0,
                    end_timestamp: 25.0,
                    title: "Short Comment 1".into(),
                    is_part_of_split: false,
                },
                StorySegment {
                    story_id: 2,
                    start_timestamp: 25.0,
                    end_timestamp: 90.0,
                    title: "Main Story".into(),
                    is_part_of_split: false,
                },
            ],
            segments: Vec::new(),
        };

        let sanitized = validate_and_sanitize_segmentation(raw, 90.0, None);
        assert_eq!(sanitized.stories.len(), 1);
        assert_eq!(sanitized.stories[0].start_timestamp, 10.0);
        assert_eq!(sanitized.stories[0].end_timestamp, 90.0);
        assert_eq!(sanitized.stories[0].story_id, 1);
    }

    #[test]
    fn test_merge_short_last_story() {
        // Story 1: 10.0 -> 70.0 (60 sec)
        // Story 2: 70.0 -> 85.0 (15 sec < 35s, is last)
        // Story 2 should be merged into previous story
        let raw = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 10.0,
                title: "Intro".into(),
            },
            stories: vec![
                StorySegment {
                    story_id: 1,
                    start_timestamp: 10.0,
                    end_timestamp: 70.0,
                    title: "First Long Story".into(),
                    is_part_of_split: false,
                },
                StorySegment {
                    story_id: 2,
                    start_timestamp: 70.0,
                    end_timestamp: 85.0,
                    title: "Trailing Short Remark".into(),
                    is_part_of_split: false,
                },
            ],
            segments: Vec::new(),
        };

        let sanitized = validate_and_sanitize_segmentation(raw, 85.0, None);
        assert_eq!(sanitized.stories.len(), 1);
        assert_eq!(sanitized.stories[0].start_timestamp, 10.0);
        assert_eq!(sanitized.stories[0].end_timestamp, 85.0);
    }

    #[test]
    fn test_snapping_to_whisper_pauses() {
        let raw = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 7.2, // Hallucinated, real whisper pause is at 6.8
                title: "Intro".into(),
            },
            stories: vec![
                StorySegment {
                    story_id: 1,
                    start_timestamp: 7.2,
                    end_timestamp: 58.2, // Hallucinated, real whisper pause is at 60.0
                    title: "Story 1".into(),
                    is_part_of_split: false,
                },
                StorySegment {
                    story_id: 2,
                    start_timestamp: 58.2,
                    end_timestamp: 115.0,
                    title: "Story 2".into(),
                    is_part_of_split: false,
                },
            ],
            segments: Vec::new(),
        };

        let candidate_cuts = vec![6.8, 60.0, 100.0];
        let sanitized = validate_and_sanitize_segmentation(raw, 115.0, Some(&candidate_cuts));
        assert_eq!(sanitized.global_intro.end_timestamp, 6.8); // Snapped to 6.8
        assert_eq!(sanitized.stories[0].start_timestamp, 6.8);
        assert_eq!(sanitized.stories[0].end_timestamp, 60.0); // Snapped to 60.0
        assert_eq!(sanitized.stories[1].start_timestamp, 60.0);
        assert_eq!(sanitized.stories[1].end_timestamp, 115.0);
    }

    #[test]
    fn test_build_clip_plans_with_intro() {
        let seg = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 8.0,
                title: "Reddit Question".into(),
            },
            stories: vec![
                StorySegment {
                    story_id: 1,
                    start_timestamp: 8.0,
                    end_timestamp: 70.0,
                    title: "Story Part 1".into(),
                    is_part_of_split: true,
                },
                StorySegment {
                    story_id: 2,
                    start_timestamp: 70.0,
                    end_timestamp: 130.0,
                    title: "Story Part 2".into(),
                    is_part_of_split: true,
                },
            ],
            segments: Vec::new(),
        };

        let plans = seg.build_clip_plans(true);
        let pause_dur = intro_pause_duration();
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].clip_index, 1);
        assert_eq!(plans[0].intro.as_ref().unwrap().end_timestamp, 8.0);
        assert_eq!(plans[0].story.title, "Story Part 1");
        assert_eq!(plans[0].total_duration, 8.0 + pause_dur + (70.0 - 8.0)); // 70.5
        assert_eq!(plans[1].clip_index, 2);
        assert_eq!(plans[1].total_duration, 8.0 + pause_dur + (130.0 - 70.0)); // 68.5
    }

    #[test]
    fn test_build_clip_plans_without_intro() {
        let seg = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 8.0,
                title: "Reddit Question".into(),
            },
            stories: vec![
                StorySegment {
                    story_id: 1,
                    start_timestamp: 8.0,
                    end_timestamp: 70.0,
                    title: "Story Part 1".into(),
                    is_part_of_split: true,
                },
                StorySegment {
                    story_id: 2,
                    start_timestamp: 70.0,
                    end_timestamp: 130.0,
                    title: "Story Part 2".into(),
                    is_part_of_split: true,
                },
            ],
            segments: Vec::new(),
        };

        let plans = seg.build_clip_plans(false);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].clip_index, 1);
        assert!(plans[0].intro.is_none());
        assert_eq!(plans[0].story.start_timestamp, 8.0);
        assert_eq!(plans[0].story.end_timestamp, 70.0);
        assert_eq!(plans[0].story.title, "Story Part 1");
        assert_eq!(plans[0].total_duration, 70.0 - 8.0); // 62.0 (no intro duration added)

        assert_eq!(plans[1].clip_index, 2);
        assert!(plans[1].intro.is_none());
        assert_eq!(plans[1].story.start_timestamp, 70.0);
        assert_eq!(plans[1].story.end_timestamp, 130.0);
        assert_eq!(plans[1].total_duration, 130.0 - 70.0); // 60.0
    }

    #[test]
    fn test_fallback_narrative_segmentation() {
        let transcript = FullTranscript {
            total_duration: 120.0,
            words: vec![],
            segments: vec![
                TranscriptSegment {
                    index: 1,
                    start: 0.0,
                    end: 6.0,
                    text: "Am I the asshole for refusing to attend?".into(),
                    pause_after: 0.8,
                },
                TranscriptSegment {
                    index: 2,
                    start: 6.8,
                    end: 65.0,
                    text: "Story text part 1...".into(),
                    pause_after: 0.9,
                },
                TranscriptSegment {
                    index: 3,
                    start: 65.9,
                    end: 120.0,
                    text: "Story text part 2...".into(),
                    pause_after: 0.0,
                },
            ],
        };

        let res = fallback_narrative_segmentation(&transcript);
        assert_eq!(res.global_intro.start_timestamp, 0.0);
        assert_eq!(res.global_intro.end_timestamp, 6.0);
        assert!(!res.stories.is_empty());
        assert!(res.stories[0].start_timestamp >= 6.0);
        assert_eq!(res.stories.last().unwrap().end_timestamp, 120.0);
    }

    #[test]
    fn test_segmentation_json_schema() {
        let schema = segmentation_json_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["global_intro"].is_object());
        assert!(schema["properties"]["stories"].is_object());
    }

    #[test]
    fn test_validate_and_sanitize_segments_legacy_compatibility() {
        let segments = vec![
            NarrativeSegment {
                segment_id: 1,
                segment_type: "hook".into(),
                title: "Hook".into(),
                start_timestamp: 1.0,
                end_timestamp: 8.0,
            },
            NarrativeSegment {
                segment_id: 2,
                segment_type: "story".into(),
                title: "Story".into(),
                start_timestamp: 10.0,
                end_timestamp: 50.0,
            },
        ];

        let sanitized = validate_and_sanitize_segments(segments, 60.0);
        assert_eq!(sanitized.len(), 2);
        assert_eq!(sanitized[0].start_timestamp, 0.0);
        assert_eq!(sanitized[0].end_timestamp, 8.0);
        assert_eq!(sanitized[1].start_timestamp, 8.0);
        assert_eq!(sanitized[1].end_timestamp, 60.0);
    }

    #[test]
    fn test_split_overlong_story_exceeding_max_duration() {
        // Video of 600 seconds with a single giant story from 10.0 to 600.0 (590s > 120s)
        let raw = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 10.0,
                title: "Intro Question".into(),
            },
            stories: vec![StorySegment {
                story_id: 1,
                start_timestamp: 10.0,
                end_timestamp: 600.0,
                title: "One Giant Story".into(),
                is_part_of_split: false,
            }],
            segments: Vec::new(),
        };

        let candidate_cuts = vec![85.0, 160.0, 235.0, 310.0, 385.0, 460.0, 535.0];
        let sanitized = validate_and_sanitize_segmentation(raw, 600.0, Some(&candidate_cuts));

        assert!(sanitized.stories.len() > 1, "Must split long story into multiple parts");
        for s in &sanitized.stories {
            let dur = s.end_timestamp - s.start_timestamp;
            assert!(
                dur <= MAX_STORY_DURATION + 5.0,
                "Each part duration ({dur:.1}s) must not exceed MAX_STORY_DURATION ({MAX_STORY_DURATION}s)"
            );
            assert!(s.is_part_of_split, "Split parts must have is_part_of_split = true");
        }
        assert_eq!(sanitized.stories[0].start_timestamp, 10.0);
        assert_eq!(sanitized.stories.last().unwrap().end_timestamp, 600.0);
    }

    #[test]
    fn test_clamp_overlong_global_intro() {
        // LLM hallucinated an 84-second intro on a 1200-second video
        let raw = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 84.4,
                title: "AskReddit Question".into(),
            },
            stories: vec![StorySegment {
                story_id: 1,
                start_timestamp: 84.4,
                end_timestamp: 300.0,
                title: "Story".into(),
                is_part_of_split: false,
            }],
            segments: Vec::new(),
        };

        let candidate_cuts = vec![7.5, 12.0, 84.4];
        let sanitized = validate_and_sanitize_segmentation(raw, 300.0, Some(&candidate_cuts));
        assert!(
            sanitized.global_intro.end_timestamp <= 25.0,
            "global_intro must be clamped to <= 25s, got {:.1}s",
            sanitized.global_intro.end_timestamp
        );
        assert_eq!(sanitized.stories[0].start_timestamp, sanitized.global_intro.end_timestamp);
    }

    #[test]
    fn test_refine_intro_boundary_aita_russian() {
        let transcript = FullTranscript {
            total_duration: 120.0,
            words: vec![
                TimedWord { text: "Мудак".into(), start: 0.2, end: 0.8 },
                TimedWord { text: "ли".into(), start: 0.82, end: 1.0 },
                TimedWord { text: "я".into(), start: 1.02, end: 1.2 },
                TimedWord { text: "из-за".into(), start: 1.25, end: 1.6 },
                TimedWord { text: "того".into(), start: 1.62, end: 1.9 },
                TimedWord { text: "что".into(), start: 1.92, end: 2.1 },
                TimedWord { text: "сказал".into(), start: 2.15, end: 2.6 },
                TimedWord { text: "это".into(), start: 2.62, end: 2.8 },
                TimedWord { text: "своей".into(), start: 2.82, end: 3.2 },
                TimedWord { text: "девушке?".into(), start: 3.22, end: 3.8 },
                TimedWord { text: "Итак,".into(), start: 4.6, end: 5.0 },
                TimedWord { text: "мне".into(), start: 5.05, end: 5.3 },
                TimedWord { text: "25".into(), start: 5.35, end: 5.7 },
                TimedWord { text: "лет.".into(), start: 5.75, end: 6.1 },
            ],
            segments: vec![
                TranscriptSegment {
                    index: 1,
                    start: 0.2,
                    end: 3.8,
                    text: "Мудак ли я из-за того что сказал это своей девушке?".into(),
                    pause_after: 0.8,
                },
                TranscriptSegment {
                    index: 2,
                    start: 4.6,
                    end: 15.0,
                    text: "Итак, мне 25 лет. Мы встречаемся уже два года.".into(),
                    pause_after: 0.5,
                },
            ],
        };

        // Модель ошиблась и включила 15.0с
        let mut raw = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 15.0,
                title: "Мудак ли я".into(),
            },
            stories: vec![StorySegment {
                story_id: 1,
                start_timestamp: 15.0,
                end_timestamp: 120.0,
                title: "Story".into(),
                is_part_of_split: false,
            }],
            segments: Vec::new(),
        };

        refine_intro_boundary_with_transcript(&mut raw, &transcript);
        assert_eq!(raw.global_intro.end_timestamp, 3.8);
        assert_eq!(raw.stories[0].start_timestamp, 4.6);
    }

    #[test]
    fn test_refine_intro_boundary_secret_russian() {
        let transcript = FullTranscript {
            total_duration: 100.0,
            words: vec![
                TimedWord { text: "Случайно".into(), start: 0.1, end: 0.7 },
                TimedWord { text: "узнал".into(), start: 0.72, end: 1.1 },
                TimedWord { text: "тайный".into(), start: 1.15, end: 1.6 },
                TimedWord { text: "секрет".into(), start: 1.65, end: 2.1 },
                TimedWord { text: "своей".into(), start: 2.12, end: 2.5 },
                TimedWord { text: "девушки".into(), start: 2.52, end: 3.0 },
                TimedWord { text: "и...".into(), start: 3.05, end: 3.5 },
                TimedWord { text: "Всё".into(), start: 4.3, end: 4.6 },
                TimedWord { text: "началось".into(), start: 4.65, end: 5.2 },
                TimedWord { text: "в".into(), start: 5.25, end: 5.4 },
                TimedWord { text: "прошлую".into(), start: 5.45, end: 5.9 },
                TimedWord { text: "субботу.".into(), start: 5.95, end: 6.5 },
            ],
            segments: vec![
                TranscriptSegment {
                    index: 1,
                    start: 0.1,
                    end: 3.5,
                    text: "Случайно узнал тайный секрет своей девушки и...".into(),
                    pause_after: 0.8,
                },
                TranscriptSegment {
                    index: 2,
                    start: 4.3,
                    end: 18.0,
                    text: "Всё началось в прошлую субботу когда она оставила телефон.".into(),
                    pause_after: 0.6,
                },
            ],
        };

        let mut raw = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 18.0,
                title: "Секрет девушки".into(),
            },
            stories: vec![StorySegment {
                story_id: 1,
                start_timestamp: 18.0,
                end_timestamp: 100.0,
                title: "Story".into(),
                is_part_of_split: false,
            }],
            segments: Vec::new(),
        };

        refine_intro_boundary_with_transcript(&mut raw, &transcript);
        assert_eq!(raw.global_intro.end_timestamp, 3.5);
        assert_eq!(raw.stories[0].start_timestamp, 4.3);
    }

    #[test]
    fn test_refine_intro_boundary_askreddit_prompt() {
        let transcript = FullTranscript {
            total_duration: 150.0,
            words: vec![
                TimedWord { text: "Расскажите".into(), start: 0.2, end: 0.9 },
                TimedWord { text: "о".into(), start: 0.92, end: 1.0 },
                TimedWord { text: "случаях".into(), start: 1.05, end: 1.6 },
                TimedWord { text: "когда".into(), start: 1.62, end: 1.9 },
                TimedWord { text: "вы".into(), start: 1.92, end: 2.1 },
                TimedWord { text: "чуть".into(), start: 2.15, end: 2.4 },
                TimedWord { text: "не".into(), start: 2.42, end: 2.6 },
                TimedWord { text: "уволились.".into(), start: 2.65, end: 3.3 },
                TimedWord { text: "Первая".into(), start: 4.1, end: 4.6 },
                TimedWord { text: "история:".into(), start: 4.65, end: 5.2 },
                TimedWord { text: "Мне".into(), start: 5.25, end: 5.5 },
                TimedWord { text: "было".into(), start: 5.55, end: 5.8 },
                TimedWord { text: "19.".into(), start: 5.85, end: 6.2 },
            ],
            segments: vec![
                TranscriptSegment {
                    index: 1,
                    start: 0.2,
                    end: 3.3,
                    text: "Расскажите о случаях когда вы чуть не уволились.".into(),
                    pause_after: 0.8,
                },
                TranscriptSegment {
                    index: 2,
                    start: 4.1,
                    end: 65.0,
                    text: "Первая история: Мне было 19 лет и я работал курьером.".into(),
                    pause_after: 0.9,
                },
            ],
        };

        let mut raw = SegmentationResult {
            global_intro: GlobalIntro {
                start_timestamp: 0.0,
                end_timestamp: 20.0,
                title: "Истории с работы".into(),
            },
            stories: vec![StorySegment {
                story_id: 1,
                start_timestamp: 20.0,
                end_timestamp: 150.0,
                title: "Story".into(),
                is_part_of_split: false,
            }],
            segments: Vec::new(),
        };

        refine_intro_boundary_with_transcript(&mut raw, &transcript);
        assert_eq!(raw.global_intro.end_timestamp, 3.3);
        assert_eq!(raw.stories[0].start_timestamp, 4.1);
    }
}

