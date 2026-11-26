//! Таймеры, голосовые напоминания и расписание ежедневных рутин с хранением в `schedules.json`.

use crate::engine::Engine;
use crate::settings::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Debug, Serialize)]
pub struct TimerEntry {
    pub id: String,
    pub label: String,
    pub fire_at_unix: i64,
    pub total_s: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoutineEntry {
    pub id: String,
    pub title: String,
    pub enabled: bool,
    pub time: String,
    #[serde(default)]
    pub days: Vec<u8>,
    pub action: String,
    #[serde(default)]
    pub payload: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<i64>,
    #[serde(default)]
    pub last_fired_day: String,
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedSchedules {
    #[serde(default)]
    routines: Vec<RoutineEntry>,
}

struct Inner {
    timers: Vec<TimerEntry>,
    routines: Vec<RoutineEntry>,
}

pub struct Scheduler {
    path: PathBuf,
    store: Store,
    inner: Mutex<Inner>,
    showing_led: AtomicBool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VoiceTimerCmd {
    Set { secs: u64, label: String },
    List,
    Cancel,
}

impl Scheduler {
    pub fn load(data_dir: PathBuf, store: Store) -> Arc<Self> {
        let path = data_dir.join("schedules.json");
        let routines = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PersistedSchedules>(&bytes).ok())
            .map(|p| p.routines)
            .unwrap_or_default();
        Arc::new(Self {
            path,
            store,
            inner: Mutex::new(Inner {
                timers: Vec::new(),
                routines,
            }),
            showing_led: AtomicBool::new(false),
        })
    }

    fn save_routines(&self, routines: &[RoutineEntry]) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let data = PersistedSchedules {
            routines: routines.to_vec(),
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(&data) {
            let _ = jarvis_station::atomic(&self.path, &bytes);
        }
    }

    pub fn active_timers_json(&self) -> Value {
        let now = jiff::Timestamp::now().as_second();
        let g = self.inner.lock().unwrap();
        let mut sorted: Vec<&TimerEntry> = g.timers.iter().filter(|t| t.fire_at_unix >= now).collect();
        sorted.sort_by_key(|t| t.fire_at_unix);
        Value::Array(
            sorted
                .into_iter()
                .map(|t| {
                    let rem = (t.fire_at_unix - now).max(0) as u64;
                    json!({
                        "id": t.id,
                        "label": t.label,
                        "remaining_s": rem,
                        "total_s": t.total_s,
                    })
                })
                .collect(),
        )
    }

    pub fn status(&self) -> Value {
        let now = jiff::Timestamp::now().as_second();
        let g = self.inner.lock().unwrap();
        let mut timers: Vec<&TimerEntry> = g.timers.iter().filter(|t| t.fire_at_unix >= now).collect();
        timers.sort_by_key(|t| t.fire_at_unix);
        let timer_vals: Vec<Value> = timers
            .into_iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "label": t.label,
                    "remaining_s": (t.fire_at_unix - now).max(0) as u64,
                    "total_s": t.total_s,
                })
            })
            .collect();
        json!({
            "timers": timer_vals,
            "routines": g.routines,
        })
    }

    pub fn add_timer(&self, label: String, seconds: u64) -> Result<Value, String> {
        let secs = seconds.clamp(1, 86400);
        let now = jiff::Timestamp::now().as_second();
        let clean_label = label.trim().chars().take(80).collect::<String>();
        let clean_label = if clean_label.is_empty() {
            "Таймер".to_string()
        } else {
            clean_label
        };
        let entry = TimerEntry {
            id: format!("t_{}", crate::settings::rand_hex(4)),
            label: clean_label,
            fire_at_unix: now + secs as i64,
            total_s: secs,
        };
        {
            let mut g = self.inner.lock().unwrap();
            g.timers.retain(|t| t.fire_at_unix >= now);
            if g.timers.len() >= 20 {
                return Err("Слишком много активных таймеров (максимум 20)".into());
            }
            g.timers.push(entry);
        }
        Ok(self.status())
    }

    pub fn cancel_timer(&self, id: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        let before = g.timers.len();
        g.timers.retain(|t| t.id != id);
        g.timers.len() < before
    }

    pub fn cancel_all_timers(&self) -> usize {
        let mut g = self.inner.lock().unwrap();
        let n = g.timers.len();
        g.timers.clear();
        n
    }

    pub fn add_routine(
        &self,
        title: String,
        time: String,
        days: Vec<u8>,
        action: String,
        payload: String,
        volume: Option<i64>,
    ) -> Result<Value, String> {
        let (clean_title, hhmm_norm, clean_days, action, payload, volume) =
            normalize_routine_fields(title, time, days, action, payload, volume)?;
        let entry = RoutineEntry {
            id: format!("r_{}", crate::settings::rand_hex(4)),
            title: clean_title,
            enabled: true,
            time: hhmm_norm,
            days: clean_days,
            action,
            payload,
            volume,
            last_fired_day: String::new(),
        };
        {
            let mut g = self.inner.lock().unwrap();
            if g.routines.len() >= 40 {
                return Err("Максимум 40 расписаний".into());
            }
            g.routines.push(entry);
            self.save_routines(&g.routines);
        }
        Ok(self.status())
    }

    pub fn update_routine(
        &self,
        id: &str,
        title: String,
        time: String,
        days: Vec<u8>,
        action: String,
        payload: String,
        volume: Option<i64>,
    ) -> Result<Value, String> {
        let (clean_title, hhmm_norm, clean_days, action, payload, volume) =
            normalize_routine_fields(title, time, days, action, payload, volume)?;
        {
            let mut g = self.inner.lock().unwrap();
            let Some(r) = g.routines.iter_mut().find(|r| r.id == id) else {
                return Err("Расписание не найдено".into());
            };
            if r.time != hhmm_norm {
                r.last_fired_day.clear();
            }
            r.title = clean_title;
            r.time = hhmm_norm;
            r.days = clean_days;
            r.action = action;
            r.payload = payload;
            r.volume = volume;
            self.save_routines(&g.routines);
        }
        Ok(self.status())
    }

    pub fn toggle_routine(&self, id: &str, enabled: bool) -> Result<Value, String> {
        let mut g = self.inner.lock().unwrap();
        let Some(r) = g.routines.iter_mut().find(|r| r.id == id) else {
            return Err("Расписание не найдено".into());
        };
        r.enabled = enabled;
        self.save_routines(&g.routines);
        drop(g);
        Ok(self.status())
    }

    pub fn delete_routine(&self, id: &str) -> Result<Value, String> {
        let mut g = self.inner.lock().unwrap();
        let before = g.routines.len();
        g.routines.retain(|r| r.id != id);
        if g.routines.len() == before {
            return Err("Расписание не найдено".into());
        }
        self.save_routines(&g.routines);
        drop(g);
        Ok(self.status())
    }

    pub fn run_routine_now(&self, engine: Arc<Engine>, id: &str) -> Result<String, String> {
        let r = {
            let g = self.inner.lock().unwrap();
            g.routines.iter().find(|r| r.id == id).cloned()
        };
        let Some(routine) = r else {
            return Err("Расписание не найдено".into());
        };
        let title = routine.title.clone();
        tokio::spawn(async move {
            execute_routine(engine, routine).await;
        });
        Ok(format!("Запустил «{title}»"))
    }

    pub fn spawn(self: Arc<Self>, engine: Arc<Engine>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                self.tick(&engine);
            }
        });
    }

    fn tick(&self, engine: &Arc<Engine>) {
        let now_unix = jiff::Timestamp::now().as_second();
        let mut expired: Vec<TimerEntry> = Vec::new();
        let mut min_rem: Option<u64> = None;

        {
            let mut g = self.inner.lock().unwrap();
            let mut kept = Vec::with_capacity(g.timers.len());
            for t in g.timers.drain(..) {
                if t.fire_at_unix <= now_unix {
                    expired.push(t);
                } else {
                    let rem = (t.fire_at_unix - now_unix) as u64;
                    min_rem = Some(min_rem.map_or(rem, |m| m.min(rem)));
                    kept.push(t);
                }
            }
            g.timers = kept;
        }

        for t in expired {
            let speech = if t.label == "Таймер" || t.label.is_empty() {
                "[sfx:bell] Таймер вышел!".to_string()
            } else {
                format!("[sfx:bell] Напоминаю: {}!", t.label)
            };
            engine.journal.note("таймер", &t.label, "Время вышло");
            let e = engine.clone();
            tokio::task::spawn_blocking(move || {
                let _ = e.say_aloud(&speech, Duration::from_secs(20));
            });
        }

        let focus_busy = engine
            .tools
            .focus
            .as_ref()
            .is_some_and(|f| f.status()["running"] == true);
        let games_busy = engine
            .tools
            .games
            .as_ref()
            .is_some_and(|g| g.is_any_active());

        if !focus_busy && !games_busy {
            if let Some(rem) = min_rem.filter(|&r| r <= 5999) {
                let m = (rem / 60).min(99) as u8;
                let s = (rem % 60) as u8;
                let digits = [m / 10, m % 10, s / 10, s % 10];
                engine.leds.set_clock_digits(Some(digits), rem % 2 == 0);
                self.showing_led.store(true, Ordering::SeqCst);
            } else if self.showing_led.swap(false, Ordering::SeqCst) {
                engine.leds.set_clock_digits(None, false);
            }
        } else {
            self.showing_led.store(false, Ordering::SeqCst);
        }

        // Проверяем ежедневные рутины по локальному времени колонки
        let now_z = self.store.now();
        let today = now_z.strftime("%Y-%m-%d").to_string();
        let cur_hhmm = now_z.strftime("%H:%M").to_string();
        let cur_day = weekday_num(&now_z);
        let mut to_fire: Vec<RoutineEntry> = Vec::new();
        {
            let mut g = self.inner.lock().unwrap();
            let mut changed = false;
            for r in &mut g.routines {
                if !r.enabled || r.time != cur_hhmm || r.last_fired_day == today {
                    continue;
                }
                if !r.days.is_empty() && !r.days.contains(&cur_day) {
                    continue;
                }
                r.last_fired_day = today.clone();
                to_fire.push(r.clone());
                changed = true;
            }
            if changed {
                self.save_routines(&g.routines);
            }
        }

        for r in to_fire {
            let e = engine.clone();
            tokio::spawn(async move {
                execute_routine(e, r).await;
            });
        }
    }

    pub fn fast_command(self: &Arc<Self>, text: &str) -> Option<String> {
        let now = self.store.now();
        let cmd = parse_voice_timer(text, &now)?;
        match cmd {
            VoiceTimerCmd::Cancel => {
                let n = self.cancel_all_timers();
                if n == 0 {
                    Some("Активных таймеров сейчас нет.".into())
                } else {
                    Some(format!("Отменил активные таймеры ({n})."))
                }
            }
            VoiceTimerCmd::List => {
                let now_unix = jiff::Timestamp::now().as_second();
                let g = self.inner.lock().unwrap();
                let active: Vec<String> = g
                    .timers
                    .iter()
                    .filter(|t| t.fire_at_unix >= now_unix)
                    .map(|t| {
                        let rem = (t.fire_at_unix - now_unix).max(0) as u64;
                        format!("{} — через {}", t.label, format_duration_ru(rem))
                    })
                    .collect();
                if active.is_empty() {
                    Some("Сейчас нет запущенных таймеров.".into())
                } else {
                    Some(format!("Активные таймеры: {}.", active.join("; ")))
                }
            }
            VoiceTimerCmd::Set { secs, label } => match self.add_timer(label.clone(), secs) {
                Ok(_) => {
                    let dur = format_duration_ru(secs);
                    if label == "Таймер" || label == "Напоминание" {
                        Some(format!("Поставил {label} на {dur}."))
                    } else {
                        Some(format!("Напомню «{label}» через {dur}."))
                    }
                }
                Err(e) => Some(e),
            },
        }
    }
}

fn normalize_routine_fields(
    title: String,
    time: String,
    days: Vec<u8>,
    action: String,
    payload: String,
    volume: Option<i64>,
) -> Result<(String, String, Vec<u8>, String, String, Option<i64>), String> {
    let Some(mins) = crate::settings::hhmm(&time) else {
        return Err("Укажите время в формате ЧЧ:ММ".into());
    };
    let hhmm_norm = format!("{:02}:{:02}", mins / 60, mins % 60);
    let action = action.trim().to_lowercase();
    if !matches!(action.as_str(), "scenario" | "briefing" | "say" | "dj" | "ambient") {
        return Err("Неизвестный тип действия рутины".into());
    }
    let mut clean_days: Vec<u8> = days.into_iter().filter(|d| (1..=7).contains(d)).collect();
    clean_days.sort_unstable();
    clean_days.dedup();
    let clean_title = title.trim().chars().take(80).collect::<String>();
    let clean_title = if clean_title.is_empty() {
        routine_default_title(&action).to_string()
    } else {
        clean_title
    };
    let clean_payload = payload.trim().chars().take(300).collect();
    let clean_vol = volume.map(|v| v.clamp(1, 10));
    Ok((clean_title, hhmm_norm, clean_days, action, clean_payload, clean_vol))
}

fn routine_default_title(action: &str) -> &'static str {
    match action {
        "scenario" => "Сценарий",
        "briefing" => "Утренний брифинг",
        "dj" => "AI-DJ по расписанию",
        "ambient" => "Фоновый звук",
        _ => "Озвучка по расписанию",
    }
}

fn weekday_num(z: &jiff::Zoned) -> u8 {
    match z.weekday() {
        jiff::civil::Weekday::Monday => 1,
        jiff::civil::Weekday::Tuesday => 2,
        jiff::civil::Weekday::Wednesday => 3,
        jiff::civil::Weekday::Thursday => 4,
        jiff::civil::Weekday::Friday => 5,
        jiff::civil::Weekday::Saturday => 6,
        jiff::civil::Weekday::Sunday => 7,
    }
}

async fn execute_routine(engine: Arc<Engine>, r: RoutineEntry) {
    engine.journal.note("расписание", &r.title, "");
    if let Some(vol) = r.volume {
        std::thread::spawn(move || {
            let _ = crate::speakervol::set_alice_volume(vol.clamp(1, 10));
        });
    }
    let payload = r.payload.trim().to_string();
    match r.action.as_str() {
        "scenario" => {
            let id = if payload.is_empty() { "morning" } else { payload.as_str() };
            if let Err(e) = crate::tools::run_scenario(engine.clone(), id) {
                engine.journal.fail(&format!("Рутина «{}»: {e}", r.title));
            }
        }
        "briefing" => {
            let extra = if payload.is_empty() {
                String::new()
            } else {
                format!(" Учти пожелание: {payload}.")
            };
            let prompt = format!(
                "Дай короткий бодрый утренний или дневной брифинг на 3–4 живых предложения: текущее время и дата, погода (если знаешь город), и короткое напутствие.{extra}"
            );
            let _ = engine.ask_text(prompt, true).await;
        }
        "dj" => {
            let mood = if payload.is_empty() {
                "бодрый утренний инди".to_string()
            } else {
                payload
            };
            if let Some(dj) = &engine.tools.dj {
                let _ = dj.start(engine.clone(), mood);
            }
        }
        "ambient" => {
            let id = if payload.is_empty() { "rain" } else { payload.as_str() };
            if let Some(amb) = &engine.tools.ambient {
                let _ = amb.start(id);
            }
        }
        _ => {
            let msg = if payload.is_empty() {
                r.title.clone()
            } else {
                payload
            };
            let speech = format!("[sfx:chime] {msg}");
            let _ = tokio::task::spawn_blocking(move || {
                let _ = engine.say_aloud(&speech, Duration::from_secs(30));
            })
            .await;
        }
    }
}

pub fn format_duration_ru(secs: u64) -> String {
    if secs < 60 {
        return format!("{secs} сек.");
    }
    let mins = secs / 60;
    let rem_s = secs % 60;
    if mins < 60 {
        return if rem_s > 0 {
            format!("{mins} мин. {rem_s} сек.")
        } else {
            format!("{mins} мин.")
        };
    }
    let hours = mins / 60;
    let rem_m = mins % 60;
    if rem_m > 0 {
        format!("{hours} ч. {rem_m} мин.")
    } else {
        format!("{hours} ч.")
    }
}

fn ru_number_word(w: &str) -> Option<u64> {
    match w {
        "один" | "одна" | "одну" => Some(1),
        "два" | "две" => Some(2),
        "три" => Some(3),
        "четыре" => Some(4),
        "пять" => Some(5),
        "шесть" => Some(6),
        "семь" => Some(7),
        "восемь" => Some(8),
        "девять" => Some(9),
        "десять" => Some(10),
        "пятнадцать" => Some(15),
        "двадцать" => Some(20),
        "тридцать" => Some(30),
        "сорок" => Some(40),
        "пятьдесят" => Some(50),
        _ => None,
    }
}

/// Извлечь длительность в секундах из русской фразы ("5 минут", "полчаса", "две минуты", "30 секунд").
pub fn parse_duration_secs(text: &str) -> Option<u64> {
    let low = text.trim().to_lowercase();
    if low.contains("полчаса") {
        return Some(1800);
    }
    if low.contains("полтора часа") {
        return Some(5400);
    }
    let words: Vec<&str> = low.split_whitespace().collect();
    for (idx, w) in words.iter().enumerate() {
        let num = w
            .parse::<u64>()
            .ok()
            .or_else(|| ru_number_word(w.trim_matches(|c: char| !c.is_alphabetic())));
        if let Some(n) = num {
            let next = words.get(idx + 1).copied().unwrap_or("");
            if next.starts_with("сек") {
                return Some(n.clamp(1, 86400));
            }
            if next.starts_with("мин") {
                return Some((n * 60).clamp(1, 86400));
            }
            if next.starts_with("час") {
                return Some((n * 3600).clamp(1, 86400));
            }
        }
        if w.starts_with("мин") && idx == 0 {
            return Some(60);
        }
        if w.starts_with("час") && idx == 0 {
            return Some(3600);
        }
    }
    None
}

fn strip_duration_and_lead(tail: &str) -> String {
    let stop_words = [
        "секунд", "секунду", "секунды", "сек", "минут", "минуту", "минуты", "мин", "час", "часа", "часов",
        "полчаса", "про", "что", "о", "об", "на", "через", "таймер",
    ];
    let kept: Vec<&str> = tail
        .split_whitespace()
        .skip_while(|w| {
            let clean = w.trim_matches(|c: char| !c.is_alphanumeric());
            clean.chars().all(|c| c.is_ascii_digit())
                || ru_number_word(&clean.to_lowercase()).is_some()
                || stop_words.contains(&clean.to_lowercase().as_str())
        })
        .collect();
    kept.join(" ").trim().to_string()
}

pub fn parse_voice_timer(text: &str, now: &jiff::Zoned) -> Option<VoiceTimerCmd> {
    let trimmed = text.trim();
    let low = trimmed.to_lowercase();
    if low.contains("будильник") {
        return None;
    }

    if ["отмени таймер", "сбрось таймер", "выключи таймер", "удали таймер", "стоп таймер"]
        .iter()
        .any(|p| low.contains(p))
    {
        return Some(VoiceTimerCmd::Cancel);
    }

    if ["сколько осталось по таймер", "сколько до таймер", "какие таймеры", "активные таймеры"]
        .iter()
        .any(|p| low.contains(p))
    {
        return Some(VoiceTimerCmd::List);
    }

    let after_prefix = |pat: &str| -> Option<&str> {
        let pos = low.find(pat)?;
        let start = pos + pat.len();
        Some(if trimmed.len() == low.len() {
            trimmed[start..].trim()
        } else {
            low[start..].trim()
        })
    };

    if let Some(after) = after_prefix("напомни через ") {
        let secs = parse_duration_secs(after)?;
        let raw_label = strip_duration_and_lead(after);
        let label = if raw_label.is_empty() {
            "Напоминание".into()
        } else {
            raw_label
        };
        return Some(VoiceTimerCmd::Set { secs, label });
    }

    if let Some(after) = after_prefix("напомни в ") {
        let mut parts = after.splitn(2, char::is_whitespace);
        let first = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        if let Some(target_mins) = crate::settings::hhmm(first) {
            let cur_mins = now.hour() as i64 * 60 + now.minute() as i64;
            let mut diff_mins = target_mins - cur_mins;
            if diff_mins <= 0 {
                diff_mins += 24 * 60;
            }
            let secs = (diff_mins * 60 - now.second() as i64).max(30) as u64;
            let label = strip_duration_and_lead(rest);
            let label = if label.is_empty() { "Напоминание".into() } else { label };
            return Some(VoiceTimerCmd::Set { secs, label });
        }
        if let Ok(mut h) = first.parse::<i64>() && (0..=23).contains(&h) {
            let rest_low = rest.to_lowercase();
            let mut label_tail = rest;
            if rest_low.starts_with("вечера") || rest_low.starts_with("дня") {
                if h < 12 {
                    h += 12;
                }
                label_tail = rest.split_once(char::is_whitespace).map_or("", |x| x.1);
            } else if rest_low.starts_with("утра") {
                label_tail = rest.split_once(char::is_whitespace).map_or("", |x| x.1);
            }
            let target_mins = h * 60;
            let cur_mins = now.hour() as i64 * 60 + now.minute() as i64;
            let mut diff_mins = target_mins - cur_mins;
            if diff_mins <= 0 {
                diff_mins += 24 * 60;
            }
            let secs = (diff_mins * 60 - now.second() as i64).max(30) as u64;
            let label = strip_duration_and_lead(label_tail);
            let label = if label.is_empty() { "Напоминание".into() } else { label };
            return Some(VoiceTimerCmd::Set { secs, label });
        }
    }

    if low.contains("таймер") && (low.contains("на ") || low.contains("поставь") || low.contains("запусти") || low.contains("включи")) {
        let secs = parse_duration_secs(&low)?;
        let tail = low.split_once("таймер").map_or("", |(_, r)| r).trim();
        let label_raw = strip_duration_and_lead(tail);
        let label = if label_raw.is_empty() {
            "Таймер".into()
        } else {
            label_raw
        };
        return Some(VoiceTimerCmd::Set { secs, label });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_durations_and_words() {
        assert_eq!(parse_duration_secs("5 минут"), Some(300));
        assert_eq!(parse_duration_secs("две минуты"), Some(120));
        assert_eq!(parse_duration_secs("полчаса"), Some(1800));
        assert_eq!(parse_duration_secs("30 секунд"), Some(30));
        assert_eq!(parse_duration_secs("один час"), Some(3600));
        assert_eq!(parse_duration_secs("просто текст"), None);
    }

    #[test]
    fn parses_voice_timer_and_reminder_commands() {
        let tz = jiff::tz::TimeZone::UTC;
        let dt = jiff::civil::date(2026, 11, 26).at(12, 0, 0, 0).to_zoned(tz).unwrap();

        assert_eq!(
            parse_voice_timer("поставь таймер на 5 минут пельмени", &dt),
            Some(VoiceTimerCmd::Set {
                secs: 300,
                label: "пельмени".into(),
            })
        );
        assert_eq!(
            parse_voice_timer("напомни через полчаса выключить духовку", &dt),
            Some(VoiceTimerCmd::Set {
                secs: 1800,
                label: "выключить духовку".into(),
            })
        );
        assert_eq!(
            parse_voice_timer("напомни в 12:30 позвонить", &dt),
            Some(VoiceTimerCmd::Set {
                secs: 1800,
                label: "позвонить".into(),
            })
        );
        assert_eq!(
            parse_voice_timer("отмени таймер", &dt),
            Some(VoiceTimerCmd::Cancel)
        );
        assert_eq!(
            parse_voice_timer("какие таймеры сейчас идут", &dt),
            Some(VoiceTimerCmd::List)
        );
        assert_eq!(parse_voice_timer("поставь будильник на 8 утра", &dt), None);
    }

    #[test]
    fn routines_persist_and_toggle_cleanly() {
        let dir = std::env::temp_dir().join(format!("jarvis-sched-{}", crate::settings::rand_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(dir.join("settings.json")).unwrap();
        let s1 = Scheduler::load(dir.clone(), store.clone());
        s1.add_routine(
            "Утро".into(),
            "8:5".into(),
            vec![1, 2, 3, 4, 5],
            "scenario".into(),
            "morning".into(),
            Some(5),
        )
        .unwrap();
        let st = s1.status();
        let rid = st["routines"][0]["id"].as_str().unwrap().to_string();
        assert_eq!(st["routines"][0]["time"], "08:05");
        assert_eq!(st["routines"][0]["enabled"], true);

        s1.toggle_routine(&rid, false).unwrap();
        let s2 = Scheduler::load(dir.clone(), store);
        assert_eq!(s2.status()["routines"][0]["enabled"], false);
        s2.delete_routine(&rid).unwrap();
        assert_eq!(s2.status()["routines"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_routine_preserves_enabled_and_resets_fired_day_on_time_change() {
        let dir = std::env::temp_dir().join(format!("jarvis-sched-upd-{}", crate::settings::rand_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(dir.join("settings.json")).unwrap();
        let sched = Scheduler::load(dir.clone(), store.clone());
        sched
            .add_routine(
                "Утро".into(),
                "08:00".into(),
                vec![1, 2],
                "scenario".into(),
                "morning".into(),
                Some(4),
            )
            .unwrap();
        let rid = sched.status()["routines"][0]["id"].as_str().unwrap().to_string();
        sched.toggle_routine(&rid, false).unwrap();
        {
            let mut g = sched.inner.lock().unwrap();
            g.routines[0].last_fired_day = "2026-11-26".into();
        }
        // Тот же час — last_fired_day сохраняется
        let st_same = sched
            .update_routine(
                &rid,
                "Новое утро".into(),
                "08:00".into(),
                vec![1, 3, 5],
                "say".into(),
                "Пора!".into(),
                Some(6),
            )
            .unwrap();
        assert_eq!(st_same["routines"][0]["enabled"], false);
        assert_eq!(st_same["routines"][0]["title"], "Новое утро");
        assert_eq!(st_same["routines"][0]["last_fired_day"], "2026-11-26");

        // Изменилось время — last_fired_day сбрасывается, enabled остаётся false
        let st_moved = sched
            .update_routine(
                &rid,
                "Новое утро".into(),
                "09:15".into(),
                vec![1, 3, 5],
                "say".into(),
                "Пора!".into(),
                None,
            )
            .unwrap();
        assert_eq!(st_moved["routines"][0]["time"], "09:15");
        assert_eq!(st_moved["routines"][0]["enabled"], false);
        assert_eq!(st_moved["routines"][0]["last_fired_day"], "");

        let reloaded = Scheduler::load(dir.clone(), store);
        assert_eq!(reloaded.status()["routines"][0]["time"], "09:15");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
