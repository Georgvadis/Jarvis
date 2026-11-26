//! Режим «Спор / Мини-подкаст двух ассистентов»: 2 профиля по очереди спорят на заданную тему,
//! переключая голоса и цвет LED-кольца на каждой реплике без изменения активного профиля в настройках.

use crate::{
    engine::Engine,
    settings::{LedSettings, Profile, Settings},
};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

static DEBATE_ACTIVE: AtomicBool = AtomicBool::new(false);

pub struct DebateParams {
    pub topic: String,
    pub p1_id: Option<String>,
    pub p2_id: Option<String>,
    pub rounds: usize,
}

pub struct NotebookParams {
    pub url: String,
    pub text: String,
    pub title: String,
    pub p1_id: Option<String>,
    pub p2_id: Option<String>,
    pub turns: usize,
}

impl DebateParams {
    pub fn from_topic(topic: String) -> Self {
        Self {
            topic,
            p1_id: None,
            p2_id: None,
            rounds: 2,
        }
    }
}

pub fn is_active() -> bool {
    DEBATE_ACTIVE.load(Ordering::Relaxed)
}

pub fn spawn_debate(engine: Arc<Engine>, params: DebateParams) {
    tokio::spawn(async move {
        if let Err(e) = run_debate(engine.clone(), params).await {
            log::warn!("подкаст/спор: {e}");
            engine.journal.fail(&format!("Спор не запустился: {e}"));
        }
    });
}

pub fn spawn_notebook(engine: Arc<Engine>, params: NotebookParams) {
    tokio::spawn(async move {
        if let Err(e) = run_notebook(engine.clone(), params).await {
            log::warn!("notebooklm: {e}");
            engine.journal.fail(&format!("Подкаст по источнику не запустился: {e}"));
        }
    });
}

fn effective_voice(p: &Profile) -> &str {
    if !p.tts_voice.is_empty() {
        p.tts_voice.as_str()
    } else {
        p.voice.as_str()
    }
}

/// Подобрать пару отличающихся голосов/профилей и LED-цветов для 2 ведущих (спор, NotebookLM, книга).
pub async fn resolve_duo(
    engine: &Engine,
    s: &Settings,
    p1_id: Option<&str>,
    p2_id: Option<&str>,
    alt_fallback_name: &str,
) -> (Profile, Profile) {
    let p1 = p1_id
        .and_then(|id| s.profiles.iter().find(|p| p.id == id))
        .cloned()
        .unwrap_or_else(|| s.active_profile());

    let mut p2: Profile = p2_id
        .and_then(|id| s.profiles.iter().find(|p| p.id == id))
        .filter(|p| p.id != p1.id)
        .or_else(|| s.profiles.iter().find(|p| p.id != p1.id))
        .cloned()
        .unwrap_or_else(|| {
            let mut alt = p1.clone();
            alt.name = alt_fallback_name.to_string();
            alt
        });

    let same_voice = p1.effective_tts_engine() == p2.effective_tts_engine()
        && effective_voice(&p1) == effective_voice(&p2);
    if p2.id == p1.id || same_voice {
        let tts_eng = p2.effective_tts_engine().to_string();
        if tts_eng == "openai" {
            let next_v = if effective_voice(&p1) == "onyx" { "nova" } else { "onyx" };
            p2.tts_voice = next_v.into();
            p2.voice = next_v.into();
        } else if p2.is_classic() {
            let mut switched = false;
            if let Ok(voices_val) = engine.host.plugin_tts_voices(&tts_eng).await
                && let Some(arr) = voices_val.as_array()
            {
                let v1 = effective_voice(&p1);
                if let Some(alt_id) = arr
                    .iter()
                    .filter_map(|item| item["id"].as_str())
                    .find(|id| !id.is_empty() && *id != v1)
                {
                    p2.tts_voice = alt_id.to_string();
                    p2.voice = alt_id.to_string();
                    switched = true;
                }
            }
            if !switched {
                p2.tts_speed = if p1.tts_speed < 1.05 { 1.15 } else { 0.89 };
            }
        }
    }

    if p2.led.is_none() || p2.id == p1.id {
        p2.led = Some(LedSettings {
            enabled: s.led.enabled,
            brightness: p1.led.as_ref().map_or(s.led.brightness, |l| l.brightness).max(40),
            listening: "#38bdf8".into(),
            thinking: "#0ea5e9".into(),
            speaking: "#38bdf8".into(),
        });
    }
    (p1, p2)
}

pub async fn run_debate(engine: Arc<Engine>, params: DebateParams) -> Result<(), String> {
    let topic = params.topic.trim().to_string();
    if topic.is_empty() {
        return Err("Укажите тему спора".into());
    }
    if DEBATE_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
        .is_err()
    {
        return Err("Спор уже идёт".into());
    }
    struct ActiveGuard;
    impl Drop for ActiveGuard {
        fn drop(&mut self) {
            DEBATE_ACTIVE.store(false, Ordering::SeqCst);
        }
    }
    let _active_guard = ActiveGuard;

    let s = engine.store.get();
    let (p1, p2) = resolve_duo(
        &engine,
        &s,
        params.p1_id.as_deref(),
        params.p2_id.as_deref(),
        "Оппонент",
    )
    .await;

    let p1_prompt = s
        .prompts
        .iter()
        .find(|pr| pr.id == p1.prompt)
        .map(|pr| pr.text.as_str())
        .unwrap_or(crate::settings::DEFAULT_PROMPT);
    let p2_prompt = if p2.id != p1.id {
        s.prompts
            .iter()
            .find(|pr| pr.id == p2.prompt)
            .map(|pr| pr.text.as_str())
            .unwrap_or(crate::settings::DEFAULT_PROMPT)
    } else {
        "Ты остроумный оппонент в споре. Всегда отстаивай противоположную точку зрения, живо и с лёгкой иронией."
    };

    engine.journal.note("подкаст", &format!("Тема спора: {topic}"), "");
    let mut transcript: Vec<(String, String)> = Vec::new();
    let rounds = params.rounds.clamp(1, 4);
    let total_turns = rounds * 2;
    let epoch = engine.speech_epoch_now();

    for idx in 0..total_turns {
        if engine.speech_epoch_now() != epoch {
            break;
        }
        let is_first_speaker = idx % 2 == 0;
        let (prof, base_prompt) = if is_first_speaker {
            (&p1, p1_prompt)
        } else {
            (&p2, p2_prompt)
        };
        let role_hint = if idx == 0 {
            "Ты начинаешь спор и отстаиваешь первую точку зрения."
        } else if idx + 1 == total_turns {
            "Сделай короткий остроумный финал спора."
        } else if is_first_speaker {
            "Ответь оппоненту с иронией и приведи сильный аргумент."
        } else {
            "Ты оппонент: возрази собеседнику и приведи свой яркий аргумент."
        };

        let history_str = if transcript.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = transcript
                .iter()
                .map(|(who, text)| format!("{who}: {text}"))
                .collect();
            format!("\n\nПредыдущие реплики спора:\n{}", lines.join("\n"))
        };
        let instructions = format!(
            "{base_prompt}\n\nТы участвуешь в коротком голосовом споре-подкасте на умной колонке. {role_hint} Отвечай по-русски, живо, без Markdown, строго 1–2 коротких предложения (до 25 слов).{history_str}"
        );
        let user_msg = if idx == 0 {
            format!("Тема спора: {topic}. Начинай!")
        } else {
            format!("Твоя очередь ответить в споре на тему «{topic}».")
        };

        let reply = crate::llm::ask_with_pool(
            &engine.host,
            &s,
            prof,
            &instructions,
            &user_msg,
            &engine.tools,
            Vec::new(),
            Some(&engine.gemini_pool),
        )
        .await?;
        let reply = reply.trim().to_string();
        if reply.is_empty() || engine.speech_epoch_now() != epoch {
            break;
        }

        let clean = crate::sfx::strip_tags(&reply);
        engine.journal.note("подкаст", "", &format!("{}: {clean}", prof.name));
        transcript.push((prof.name.clone(), clean));

        let e = engine.clone();
        let line = reply.clone();
        let prof_clone = prof.clone();
        let res = tokio::task::spawn_blocking(move || {
            e.say_aloud_opts(&line, Duration::from_secs(40), Some(&prof_clone), false)
        })
        .await
        .map_err(|e| e.to_string())?;
        if res.is_err() {
            break;
        }
    }

    Ok(())
}

/// Локальный NotebookLM: превратить YouTube-видео, статью или документ в живой подкаст двух ведущих.
pub async fn run_notebook(engine: Arc<Engine>, params: NotebookParams) -> Result<(), String> {
    if DEBATE_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
        .is_err()
    {
        return Err("Подкаст или спор уже запущен".into());
    }
    struct ActiveGuard;
    impl Drop for ActiveGuard {
        fn drop(&mut self) {
            DEBATE_ACTIVE.store(false, Ordering::SeqCst);
        }
    }
    let _active_guard = ActiveGuard;

    let mut source_title = params.title.trim().to_string();
    let mut source_text = params.text.trim().to_string();

    if source_text.is_empty() {
        let url = params.url.trim();
        if url.is_empty() {
            return Err("Укажите ссылку или текст для разбора".into());
        }
        engine.journal.note("подкаст", &format!("Извлекаю материал: {url}"), "");
        let srv = crate::tools::local_voice_url(&engine.host);
        let resp = crate::net::direct()
            .post(format!("{srv}/extract"))
            .json(&json!({"url": url, "max_chars": 12000}))
            .timeout(Duration::from_secs(48))
            .send()
            .await
            .map_err(|e| format!("Сервер jarvis-voice недоступен: {e}"))?;
        let status = resp.status();
        let data: serde_json::Value = resp.json().await.unwrap_or_default();
        if !status.is_success() {
            let msg = data["detail"].as_str().unwrap_or("Не удалось извлечь материал");
            return Err(msg.to_string());
        }
        source_text = data["text"].as_str().unwrap_or("").trim().to_string();
        if source_title.is_empty() {
            source_title = data["title"].as_str().unwrap_or("Материал").trim().to_string();
        }
    }
    if source_text.is_empty() {
        return Err("Не удалось получить текст источника".into());
    }
    if source_title.is_empty() {
        source_title = "Разбор материала".into();
    }
    let clipped_source: String = source_text.chars().take(9000).collect();

    let s = engine.store.get();
    let (p1, p2) = resolve_duo(
        &engine,
        &s,
        params.p1_id.as_deref(),
        params.p2_id.as_deref(),
        "Соведущий",
    )
    .await;

    let p1_prompt = s.prompt_text(&p1.prompt);
    let p2_prompt = if p2.id != p1.id {
        s.prompt_text(&p2.prompt)
    } else {
        "Ты любопытный соведущий подкаста: задаёшь точные вопросы, подмечаешь неожиданные детали и шутишь к месту.".into()
    };

    engine.journal.note("подкаст", &format!("NotebookLM: {source_title}"), "");
    let turns = params.turns.clamp(4, 10);
    let epoch = engine.speech_epoch_now();
    let mut dialogue: Vec<(String, String)> = Vec::new();

    for idx in 0..turns {
        if engine.speech_epoch_now() != epoch {
            break;
        }
        let is_first = idx % 2 == 0;
        let (prof, base_prompt) = if is_first { (&p1, p1_prompt.as_str()) } else { (&p2, p2_prompt.as_str()) };
        let phase_hint = if idx == 0 {
            "Поприветствуй слушателей и коротко представь тему выпуска по материалу ниже, сразу зацепив главным фактом. Можешь начать с [sfx:chime] [happy]."
        } else if idx + 1 == turns {
            "Подведи яркий, запоминающийся итог выпуска и попрощайся до следующего раза."
        } else if is_first {
            "Развей мысль, объясни ключевой тезис из материала простым живым языком и передай слово."
        } else {
            "Живо отреагируй на реплику коллеги, выдели самую интересную деталь из материала или задай короткий уточняющий вопрос."
        };

        let prev_block = if dialogue.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = dialogue.iter().map(|(w, t)| format!("{w}: {t}")).collect();
            format!("\n\nУже сказано в подкасте:\n{}", lines.join("\n"))
        };

        let instructions = format!(
            "{base_prompt}\n\nТы соведущий увлекательного голосового подкаста-разбора (в духе NotebookLM). {phase_hint}\n\
            Говори по-русски, живо, естественно, 2–3 предложения (до 45 слов), БЕЗ Markdown и списков.\n\n\
            === ИСТОЧНИК: {source_title} ===\n{clipped_source}{prev_block}"
        );
        let user_msg = format!("Реплика {} из {} подкаста «{}».", idx + 1, turns, source_title);

        let reply = crate::llm::ask_with_pool(
            &engine.host,
            &s,
            prof,
            &instructions,
            &user_msg,
            &engine.tools,
            Vec::new(),
            Some(&engine.gemini_pool),
        )
        .await?;
        let reply = reply.trim().to_string();
        if reply.is_empty() || engine.speech_epoch_now() != epoch {
            break;
        }

        let clean = crate::sfx::strip_tags(&reply);
        engine.journal.note("подкаст", "", &format!("{}: {clean}", prof.name));
        dialogue.push((prof.name.clone(), clean));

        let e = engine.clone();
        let line = reply.clone();
        let prof_clone = prof.clone();
        let res = tokio::task::spawn_blocking(move || {
            e.say_aloud_opts(&line, Duration::from_secs(55), Some(&prof_clone), false)
        })
        .await
        .map_err(|e| e.to_string())?;
        if res.is_err() {
            break;
        }
    }

    Ok(())
}
