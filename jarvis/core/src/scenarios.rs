//! Пользовательские сценарии (цепочки действий) с запуском голосом, через панель, расписание и присутствие.

use crate::engine::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

static SCENARIO_GEN: AtomicU64 = AtomicU64::new(0);

/// Отменяет все текущие выполняющиеся шаги сценариев (`wait`, `sunrise` и т.д.).
pub fn cancel_running() {
    SCENARIO_GEN.fetch_add(1, Ordering::SeqCst);
}

async fn sleep_cancelable(secs: u64, run_gen: u64) -> bool {
    for _ in 0..secs {
        if SCENARIO_GEN.load(Ordering::SeqCst) != run_gen {
            return false;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    SCENARIO_GEN.load(Ordering::SeqCst) == run_gen
}

const VALID_KINDS: &[&str] = &[
    "stop_all", "volume", "ambient", "dj", "music", "focus", "timer", "wait",
    "say", "ai", "briefing", "alice", "telegram", "led", "mode", "sunrise", "course", "pc",
];
const VALID_ICONS: &[&str] = &[
    "flame", "sparkles", "sun", "moon", "briefcase", "coffee", "bolt", "music",
    "monitor", "lock", "radio", "disc", "clock", "home", "heart", "gamepad", "bell", "wand",
];
const VALID_PC_ACTIONS: &[&str] = &[
    "media_play_pause",
    "media_next",
    "media_prev",
    "volume_up",
    "volume_down",
    "volume_mute",
    "lock_screen",
    "sleep_timer",
    "cancel_sleep",
    "screenshot_save",
    "active_window",
];

fn default_icon() -> String {
    "sparkles".into()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioStep {
    pub kind: String,
    #[serde(default)]
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomScenario {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub phrase: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default)]
    pub steps: Vec<ScenarioStep>,
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedScenarios {
    #[serde(default)]
    custom: Vec<CustomScenario>,
}

pub struct ScenarioStore {
    path: PathBuf,
    custom: Mutex<Vec<CustomScenario>>,
}

impl ScenarioStore {
    pub fn load(data_dir: PathBuf) -> Arc<Self> {
        let path = data_dir.join("scenarios.json");
        let custom = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PersistedScenarios>(&bytes).ok())
            .map(|p| p.custom)
            .unwrap_or_default();
        Arc::new(Self {
            path,
            custom: Mutex::new(custom),
        })
    }

    fn save(&self, list: &[CustomScenario]) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let data = PersistedScenarios {
            custom: list.to_vec(),
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(&data) {
            let _ = jarvis_station::atomic(&self.path, &bytes);
        }
    }

    pub fn builtin_json() -> Value {
        json!([
            {"id": "morning", "name": "Утро", "icon": "sun", "builtin": true},
            {"id": "work", "name": "Работа", "icon": "briefcase", "builtin": true},
            {"id": "evening", "name": "Вечер", "icon": "sparkles", "builtin": true},
            {"id": "sleep", "name": "Сон", "icon": "moon", "builtin": true},
        ])
    }

    pub fn list_json(&self) -> Value {
        let g = self.custom.lock().unwrap();
        json!({
            "builtin": Self::builtin_json(),
            "custom": *g,
        })
    }

    pub fn custom_summary_json(&self) -> Value {
        let g = self.custom.lock().unwrap();
        Value::Array(
            g.iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "name": s.name,
                        "phrase": s.phrase,
                        "icon": s.icon,
                        "steps_count": s.steps.len(),
                    })
                })
                .collect(),
        )
    }

    pub fn create(
        &self,
        name: String,
        phrase: String,
        icon: String,
        steps: Vec<ScenarioStep>,
    ) -> Result<Value, String> {
        let (clean_name, clean_phrase, clean_icon, clean_steps) =
            normalize_scenario(name, phrase, icon, steps)?;
        let entry = CustomScenario {
            id: format!("sc_{}", crate::settings::rand_hex(4)),
            name: clean_name,
            phrase: clean_phrase,
            icon: clean_icon,
            steps: clean_steps,
        };
        {
            let mut g = self.custom.lock().unwrap();
            if g.len() >= 24 {
                return Err("Максимум 24 пользовательских сценария".into());
            }
            g.push(entry);
            self.save(&g);
        }
        Ok(self.list_json())
    }

    pub fn update(
        &self,
        id: &str,
        name: String,
        phrase: String,
        icon: String,
        steps: Vec<ScenarioStep>,
    ) -> Result<Value, String> {
        let (clean_name, clean_phrase, clean_icon, clean_steps) =
            normalize_scenario(name, phrase, icon, steps)?;
        {
            let mut g = self.custom.lock().unwrap();
            let Some(sc) = g.iter_mut().find(|s| s.id == id) else {
                return Err("Сценарий не найден".into());
            };
            sc.name = clean_name;
            sc.phrase = clean_phrase;
            sc.icon = clean_icon;
            sc.steps = clean_steps;
            self.save(&g);
        }
        Ok(self.list_json())
    }

    pub fn delete(&self, id: &str) -> Result<Value, String> {
        {
            let mut g = self.custom.lock().unwrap();
            let before = g.len();
            g.retain(|s| s.id != id);
            if g.len() == before {
                return Err("Сценарий не найден".into());
            }
            self.save(&g);
        }
        Ok(self.list_json())
    }

    pub fn find_by_query(&self, q: &str) -> Option<CustomScenario> {
        let key = q.trim().to_lowercase();
        if key.is_empty() {
            return None;
        }
        let g = self.custom.lock().unwrap();
        g.iter()
            .find(|s| s.id.to_lowercase() == key)
            .or_else(|| {
                g.iter().find(|s| {
                    s.name.trim().to_lowercase() == key
                        || (!s.phrase.trim().is_empty() && s.phrase.trim().to_lowercase() == key)
                })
            })
            .cloned()
    }

    pub fn match_voice(&self, text: &str) -> Option<CustomScenario> {
        let norm = normalize_voice(text);
        if norm.is_empty() {
            return None;
        }
        let g = self.custom.lock().unwrap();
        for s in g.iter() {
            let p = normalize_voice(&s.phrase);
            if p.chars().count() >= 3 && norm.contains(&p) {
                return Some(s.clone());
            }
        }
        for s in g.iter() {
            let n = normalize_voice(&s.name);
            if n.chars().count() >= 3
                && (norm == n
                    || norm.contains(&format!("сценарий {n}"))
                    || norm.contains(&format!("режим {n}")))
            {
                return Some(s.clone());
            }
        }
        None
    }

    pub fn execute(engine: Arc<Engine>, sc: CustomScenario) {
        engine.journal.note("сценарий", &sc.name, "");
        let run_gen = SCENARIO_GEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        tokio::spawn(async move {
            for step in sc.steps {
                if SCENARIO_GEN.load(Ordering::SeqCst) != run_gen {
                    break;
                }
                match step.kind.as_str() {
                    "stop_all" => {
                        // Не сбрасываем собственный run_gen, чтобы последующие шаги этого же сценария выполнились
                        engine.tools.stop_all_activities_keep_scenario(Some(&engine), true);
                        if engine.music.playing() {
                            engine.music.stop_all(&engine.host).await;
                        }
                    }
                    "volume" => {
                        let lvl = step.number.unwrap_or(5).clamp(0, 10);
                        let e = engine.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            if crate::speakervol::set_alice_volume(lvl).is_ok() {
                                crate::engine::flash_volume_clock(&e, lvl);
                            }
                        })
                        .await;
                    }
                    "ambient" => {
                        if let Some(amb) = &engine.tools.ambient {
                            let id = if step.value.trim().is_empty() {
                                "rain"
                            } else {
                                step.value.trim()
                            };
                            let _ = amb.start(id);
                            if let Some(mins) = step.number.filter(|&n| n > 0) {
                                amb.set_sleep((mins.clamp(1, 720)) as u64);
                            }
                        }
                    }
                    "dj" => {
                        if let Some(dj) = &engine.tools.dj {
                            let mood = if step.value.trim().is_empty() {
                                "ночной Токио чилл синтвейв".to_string()
                            } else {
                                step.value.clone()
                            };
                            let _ = dj.start(engine.clone(), mood);
                        }
                    }
                    "music" => {
                        let query = step.value.trim();
                        if !query.is_empty() {
                            let _ = engine.music.play(&engine.host, query, false).await;
                        }
                    }
                    "wait" => {
                        let secs = step.number.unwrap_or(10).clamp(1, 7200) as u64;
                        if !sleep_cancelable(secs, run_gen).await {
                            break;
                        }
                    }
                    "ai" => {
                        let prompt = step.value.trim();
                        if !prompt.is_empty() {
                            let s = engine.store.get();
                            let p = s.active_profile();
                            let sys = format!(
                                "{}\nОтвечай коротко и естественно для озвучки вслух на умной колонке (2–4 предложения, без markdown и списков).",
                                s.prompt_text(&p.prompt)
                            );
                            if let Ok(answer) = crate::llm::ask_with_pool(
                                &engine.host,
                                &s,
                                &p,
                                &sys,
                                prompt,
                                &engine.tools,
                                Vec::new(),
                                Some(&engine.gemini_pool),
                            )
                            .await
                            {
                                let clean = answer.trim().to_string();
                                if !clean.is_empty() {
                                    let e = engine.clone();
                                    let _ = tokio::task::spawn_blocking(move || {
                                        let _ = e.say_aloud(&clean, Duration::from_secs(45));
                                    })
                                    .await;
                                }
                            }
                        }
                    }
                    "alice" => {
                        let cmd = step.value.trim().to_string();
                        if !cmd.is_empty() {
                            let _ = tokio::task::spawn_blocking(move || {
                                let _ = crate::alice::text(&cmd, true);
                            })
                            .await;
                        }
                    }
                    "telegram" => {
                        let msg = if step.value.trim().is_empty() {
                            format!("Сценарий «{}» выполнен", sc.name)
                        } else {
                            step.value.trim().to_string()
                        };
                        crate::telegram_bot::notify_telegram(&engine.host, &engine.proxy, &msg).await;
                    }
                    "led" => {
                        let raw = step.value.trim();
                        let mins = step.number.unwrap_or(30).clamp(0, 480) as u64;
                        if raw.eq_ignore_ascii_case("off") || mins == 0 {
                            engine.leds.set(crate::led::Mode::Off);
                        } else {
                            let hex = if raw.starts_with('#') && raw.len() == 7 {
                                raw
                            } else {
                                "#ff9900"
                            };
                            engine.leds.show(
                                crate::led::Mode::Listening,
                                hex,
                                70,
                                Duration::from_secs(mins * 60),
                            );
                        }
                    }
                    "mode" => {
                        if let Some(modes) = &engine.tools.modes {
                            let raw = step.value.trim();
                            let (kind, arg) = raw
                                .split_once(':')
                                .map(|(a, b)| (a.trim().to_lowercase(), b.trim().to_string()))
                                .unwrap_or_else(|| (raw.to_lowercase(), String::new()));
                            let target = match kind.as_str() {
                                "rpg" => Some(crate::modes::SpecialMode::Rpg(arg)),
                                "quiz" => Some(crate::modes::SpecialMode::Quiz(arg)),
                                "chat" => Some(crate::modes::SpecialMode::Chat),
                                "translator" => Some(crate::modes::SpecialMode::Translator(
                                    if arg.is_empty() { "английский".into() } else { arg },
                                )),
                                "normal" | "off" => Some(crate::modes::SpecialMode::Normal),
                                _ => None,
                            };
                            if let Some(m) = target {
                                let greeting = modes.activate(m, engine.tools.ambient.as_deref());
                                let e = engine.clone();
                                let _ = tokio::task::spawn_blocking(move || {
                                    e.say_text(&greeting);
                                })
                                .await;
                            }
                        }
                    }
                    "sunrise" => {
                        // Плавный «рассвет»: кольцо разгорается от глубокого янтарного к тёплому белому
                        let mins = step.number.unwrap_or(10).clamp(1, 30) as u64;
                        let stages: [(&str, i64); 5] = [
                            ("#ff4500", 12),
                            ("#ff6b1a", 28),
                            ("#ff9933", 48),
                            ("#ffc266", 68),
                            ("#ffe6b3", 88),
                        ];
                        let step_secs = (mins * 60 / stages.len() as u64).max(8);
                        if let Some(amb) = &engine.tools.ambient {
                            let _ = amb.start("ocean");
                            amb.set_volume(20);
                        }
                        let mut finished = true;
                        for (i, (hex, bright)) in stages.iter().enumerate() {
                            if SCENARIO_GEN.load(Ordering::SeqCst) != run_gen {
                                finished = false;
                                break;
                            }
                            engine.leds.show(crate::led::Mode::Listening, hex, *bright, Duration::from_secs(step_secs + 5));
                            if let Some(amb) = &engine.tools.ambient {
                                amb.set_volume((20 + i * 12) as i64);
                            }
                            if !sleep_cancelable(step_secs, run_gen).await {
                                finished = false;
                                break;
                            }
                        }
                        if !finished {
                            engine.leds.set(crate::led::Mode::Off);
                            break;
                        }
                        let _ = crate::briefing::run_briefing(engine.clone(), "Доброе утро! Сделай тёплую утреннюю сводку.", true).await;
                    }
                    "course" => {
                        if let (Some(courses), Some(modes)) = (&engine.tools.courses, &engine.tools.modes) {
                            let q = step.value.trim().to_lowercase();
                            let found = courses.find_by_id_or_name(&q);
                            if let Some(c) = found
                                && let Ok((course, idx)) = courses.mark_lesson_started(&c.id, c.current_lesson)
                            {
                                let prompt = crate::courses::CourseStore::build_tutor_prompt(&course, idx);
                                let title = course.lessons[idx].title.clone();
                                let greeting = modes.activate(
                                    crate::modes::SpecialMode::Tutor {
                                        course_id: course.id.clone(),
                                        lesson_idx: idx,
                                        title,
                                        prompt,
                                        bilingual: course.bilingual,
                                    },
                                    engine.tools.ambient.as_deref(),
                                );
                                let e = engine.clone();
                                let _ = tokio::task::spawn_blocking(move || {
                                    e.say_text(&greeting);
                                    if e.mic.online() {
                                        e.trigger("урок по сценарию", 1.0);
                                    }
                                })
                                .await;
                            }
                        }
                    }
                    "focus" => {
                        if let Some(focus) = &engine.tools.focus {
                            let mins = step.number.unwrap_or(25).clamp(1, 99) as u64;
                            let _ = focus.start(engine.clone(), mins);
                        }
                    }
                    "timer" => {
                        if let Some(sched) = &engine.tools.scheduler {
                            let mins = step.number.unwrap_or(5).clamp(1, 1440) as u64;
                            let label = if step.value.trim().is_empty() {
                                sc.name.clone()
                            } else {
                                step.value.clone()
                            };
                            let _ = sched.add_timer(label, mins * 60);
                        }
                    }
                    "say" => {
                        let msg = if step.value.trim().is_empty() {
                            sc.name.clone()
                        } else {
                            step.value.clone()
                        };
                        let speech = format!("[sfx:chime] {msg}");
                        let e = engine.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            let _ = e.say_aloud(&speech, Duration::from_secs(25));
                        })
                        .await;
                    }
                    "briefing" => {
                        let _ = crate::briefing::run_briefing(engine.clone(), &step.value, true).await;
                    }
                    "pc" => {
                        let act = step.value.trim();
                        if VALID_PC_ACTIONS.contains(&act) {
                            let mins = step.number.unwrap_or(30).clamp(1, 720);
                            let _ = crate::tools::call_pc_action(&engine.store, act, mins).await;
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}

fn normalize_scenario(
    name: String,
    phrase: String,
    icon: String,
    steps: Vec<ScenarioStep>,
) -> Result<(String, String, String, Vec<ScenarioStep>), String> {
    let clean_name: String = name.trim().chars().take(60).collect();
    if clean_name.is_empty() {
        return Err("Укажите название сценария".into());
    }
    let clean_phrase: String = phrase.trim().chars().take(80).collect();
    let icon_trim = icon.trim();
    let clean_icon = if VALID_ICONS.contains(&icon_trim) {
        icon_trim.to_string()
    } else {
        default_icon()
    };
    if steps.is_empty() || steps.len() > 10 {
        return Err("Укажите от 1 до 10 шагов сценария".into());
    }
    let mut clean_steps = Vec::with_capacity(steps.len());
    for s in steps {
        let kind = s.kind.trim().to_lowercase();
        if !VALID_KINDS.contains(&kind.as_str()) {
            return Err(format!("Неизвестный шаг сценария: {kind}"));
        }
        let val: String = s.value.trim().chars().take(300).collect();
        if kind == "pc" && !VALID_PC_ACTIONS.contains(&val.as_str()) {
            return Err(format!("Недопустимое действие ПК в сценарии: {val}"));
        }
        let number = match kind.as_str() {
            "volume" => Some(s.number.unwrap_or(5).clamp(0, 10)),
            "focus" => Some(s.number.unwrap_or(25).clamp(1, 99)),
            "timer" => Some(s.number.unwrap_or(5).clamp(1, 1440)),
            "wait" => Some(s.number.unwrap_or(10).clamp(1, 7200)),
            "led" => Some(s.number.unwrap_or(30).clamp(0, 480)),
            "sunrise" => Some(s.number.unwrap_or(10).clamp(1, 30)),
            "ambient" => s.number.filter(|&n| n > 0).map(|n| n.clamp(1, 720)),
            "pc" if val == "sleep_timer" => Some(s.number.unwrap_or(30).clamp(1, 720)),
            _ => s.number,
        };
        clean_steps.push(ScenarioStep {
            kind,
            value: val,
            number,
        });
    }
    Ok((clean_name, clean_phrase, clean_icon, clean_steps))
}

fn normalize_voice(text: &str) -> String {
    let cleaned: String = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crud_persists_and_validates_steps() {
        let dir = std::env::temp_dir().join(format!("jarvis-scen-{}", crate::settings::rand_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ScenarioStore::load(dir.clone());

        assert!(store
            .create(
                "Плохой".into(),
                "".into(),
                "sparkles".into(),
                vec![ScenarioStep {
                    kind: "pc".into(),
                    value: "rm_rf".into(),
                    number: None,
                }],
            )
            .is_err());

        let created = store
            .create(
                "Кинотеатр".into(),
                "включи кинотеатр".into(),
                "moon".into(),
                vec![
                    ScenarioStep {
                        kind: "volume".into(),
                        value: String::new(),
                        number: Some(7),
                    },
                    ScenarioStep {
                        kind: "pc".into(),
                        value: "media_play_pause".into(),
                        number: None,
                    },
                ],
            )
            .unwrap();
        let id = created["custom"][0]["id"].as_str().unwrap().to_string();
        assert_eq!(created["custom"][0]["name"], "Кинотеатр");

        let updated = store
            .update(
                &id,
                "Домашний кинотеатр".into(),
                "кино".into(),
                "sparkles".into(),
                vec![ScenarioStep {
                    kind: "stop_all".into(),
                    value: String::new(),
                    number: None,
                }],
            )
            .unwrap();
        assert_eq!(updated["custom"][0]["name"], "Домашний кинотеатр");

        let reloaded = ScenarioStore::load(dir.clone());
        assert_eq!(reloaded.list_json()["custom"].as_array().unwrap().len(), 1);
        reloaded.delete(&id).unwrap();
        assert_eq!(reloaded.list_json()["custom"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn matches_voice_phrase_and_scenario_name() {
        let dir = std::env::temp_dir().join(format!("jarvis-scen-v-{}", crate::settings::rand_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ScenarioStore::load(dir.clone());
        store
            .create(
                "Кодинг".into(),
                "пора кодить".into(),
                "bolt".into(),
                vec![ScenarioStep {
                    kind: "focus".into(),
                    value: String::new(),
                    number: Some(25),
                }],
            )
            .unwrap();

        assert_eq!(
            store.match_voice("Джарвис, пора кодить!").map(|s| s.name),
            Some("Кодинг".into())
        );
        assert_eq!(
            store.match_voice("Включи сценарий кодинг").map(|s| s.name),
            Some("Кодинг".into())
        );
        assert!(store.match_voice("какая сегодня погода").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
