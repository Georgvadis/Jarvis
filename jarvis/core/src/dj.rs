//! Персональный AI-DJ с характером: подбирает очередь треков YouTube под вайб пользователя,
//! делает короткие радио-подводки голосом ассистента между треками и реагирует на «лайк / дальше / поспокойнее».

use crate::engine::Engine;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Notify;

#[derive(Default)]
struct DjState {
    active: bool,
    generation: u64,
    mood: String,
    station: String,
    current_track: String,
    current_title: String,
    queue: VecDeque<String>,
    played: Vec<String>,
    likes: Vec<String>,
    sleep_until_unix: i64,
    sleep_gen: u64,
}

pub struct AiDj {
    st: Mutex<DjState>,
    wake: Notify,
}

impl AiDj {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            st: Mutex::new(DjState::default()),
            wake: Notify::new(),
        })
    }

    pub fn is_active(&self) -> bool {
        self.st.lock().unwrap().active
    }

    pub fn status(&self) -> Value {
        let now = jiff::Timestamp::now().as_second();
        let g = self.st.lock().unwrap();
        let music_sleep_s = if g.sleep_until_unix > now {
            (g.sleep_until_unix - now) as u64
        } else {
            0
        };
        json!({
            "active": g.active,
            "mood": g.mood,
            "station": g.station,
            "current_track": g.current_track,
            "current_title": g.current_title,
            "queue_len": g.queue.len(),
            "played_count": g.played.len(),
            "likes": g.likes,
            "music_sleep_s": music_sleep_s,
        })
    }

    pub fn set_music_sleep(self: &Arc<Self>, engine: Arc<Engine>, minutes: u64) -> Value {
        let s_gen = {
            let mut g = self.st.lock().unwrap();
            g.sleep_gen = g.sleep_gen.wrapping_add(1);
            if minutes == 0 {
                g.sleep_until_unix = 0;
                return self.status_locked(&g);
            }
            let mins = minutes.clamp(1, 240);
            g.sleep_until_unix = jiff::Timestamp::now().as_second() + (mins * 60) as i64;
            g.sleep_gen
        };
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let (done_fade, matches_gen) = {
                    let g = me.st.lock().unwrap();
                    let now = jiff::Timestamp::now().as_second();
                    (
                        g.sleep_until_unix > 0 && now >= g.sleep_until_unix - 20,
                        g.sleep_gen == s_gen && g.sleep_until_unix > 0,
                    )
                };
                if !matches_gen {
                    return;
                }
                if done_fade {
                    break;
                }
            }

            let orig_vol = tokio::task::spawn_blocking(|| crate::speakervol::speaker_volume().unwrap_or(5))
                .await
                .unwrap_or(5)
                .clamp(2, 10);
            if orig_vol > 1 {
                let steps = (orig_vol - 1) as u64;
                let step_ms = (20_000 / steps.max(1)).clamp(1500, 6000);
                for lvl in (1..orig_vol).rev() {
                    let still_ok = {
                        let g = me.st.lock().unwrap();
                        g.sleep_gen == s_gen && g.sleep_until_unix > 0
                    };
                    if !still_ok {
                        let _ = tokio::task::spawn_blocking(move || crate::speakervol::set_alice_volume(orig_vol)).await;
                        return;
                    }
                    let _ = tokio::task::spawn_blocking(move || crate::speakervol::set_alice_volume(lvl)).await;
                    tokio::time::sleep(Duration::from_millis(step_ms)).await;
                }
            }

            let still_ok = {
                let mut g = me.st.lock().unwrap();
                if g.sleep_gen == s_gen && g.sleep_until_unix > 0 {
                    g.sleep_until_unix = 0;
                    true
                } else {
                    false
                }
            };
            if !still_ok {
                return;
            }
            me.stop(Some(&engine));
            engine.music.stop_all(&engine.host).await;
            engine.journal.note("музыка", "Таймер сна: музыка выключена", "");
            tokio::time::sleep(Duration::from_millis(800)).await;
            let _ = tokio::task::spawn_blocking(move || crate::speakervol::set_alice_volume(orig_vol)).await;
        });
        self.status()
    }

    fn status_locked(&self, g: &DjState) -> Value {
        let now = jiff::Timestamp::now().as_second();
        let music_sleep_s = if g.sleep_until_unix > now {
            (g.sleep_until_unix - now) as u64
        } else {
            0
        };
        json!({
            "active": g.active,
            "mood": g.mood,
            "station": g.station,
            "current_track": g.current_track,
            "current_title": g.current_title,
            "queue_len": g.queue.len(),
            "played_count": g.played.len(),
            "likes": g.likes,
            "music_sleep_s": music_sleep_s,
        })
    }

    pub fn next(&self) {
        self.wake.notify_waiters();
    }

    pub fn next_with(&self, engine: &Engine) {
        let m = engine.music.clone();
        let h = engine.host.clone();
        tokio::spawn(async move {
            let _ = m.call(&h, "stop", Value::Null).await;
        });
        self.wake.notify_waiters();
    }

    pub fn stop(&self, engine: Option<&Engine>) {
        let was_active = {
            let mut g = self.st.lock().unwrap();
            let was = g.active;
            g.active = false;
            g.generation = g.generation.wrapping_add(1);
            g.queue.clear();
            g.current_track.clear();
            g.current_title.clear();
            was
        };
        if was_active {
            self.wake.notify_waiters();
            if let Some(eng) = engine {
                let m = eng.music.clone();
                let h = eng.host.clone();
                tokio::spawn(async move {
                    let _ = m.call(&h, "stop", Value::Null).await;
                });
            }
        }
    }

    pub async fn like(&self, engine: &Engine) -> String {
        let track = {
            let mut g = self.st.lock().unwrap();
            if !g.active {
                return "AI-DJ сейчас не запущен.".into();
            }
            let cur = if !g.current_title.is_empty() {
                g.current_title.clone()
            } else {
                g.current_track.clone()
            };
            if !cur.is_empty() && !g.likes.contains(&cur) {
                g.likes.push(cur.clone());
            }
            cur
        };
        if let Some(ans) = engine.music.control(&engine.host, "поставь лайк").await {
            return format!("{ans} Запомнил для подборки: {track}");
        }
        if track.is_empty() {
            "Запомнил этот вайб!".into()
        } else {
            format!("Отметил «{track}» — учту в следующих треках.")
        }
    }

    pub fn adjust_vibe(self: &Arc<Self>, engine: Arc<Engine>, hint: String) -> Result<Value, String> {
        let hint = hint.trim().to_string();
        if hint.is_empty() {
            return Err("Укажите пожелание по музыке".into());
        }
        let prev_mood = self.st.lock().unwrap().mood.clone();
        let new_mood = if prev_mood.is_empty() {
            hint
        } else {
            format!("{prev_mood}, уточнение: {hint}")
        };
        self.start(engine, new_mood)
    }

    pub fn start(self: &Arc<Self>, engine: Arc<Engine>, mood: String) -> Result<Value, String> {
        let mood = mood.trim().to_string();
        if mood.is_empty() {
            return Err("Опишите настроение или стиль музыки".into());
        }
        if engine.host.provider("youtube").is_err() {
            return Err("Для AI-DJ включите модуль YouTube на странице «Подключения»".into());
        }
        engine.tools.stop_for(&engine, "dj");
        if let Some(amb) = &engine.tools.ambient {
            amb.stop();
        }
        let generation = {
            let mut g = self.st.lock().unwrap();
            g.active = true;
            g.generation = g.generation.wrapping_add(1);
            g.mood = mood.clone();
            g.station = "Ночной эфир YaJarvis".into();
            g.queue.clear();
            g.played.clear();
            g.generation
        };
        self.wake.notify_waiters();

        let me = self.clone();
        tokio::spawn(async move {
            me.run_loop(engine, generation).await;
        });

        Ok(self.status())
    }

    async fn generate_batch(&self, engine: &Engine, mood: &str) -> (String, Vec<String>) {
        let (played, likes) = {
            let g = self.st.lock().unwrap();
            (g.played.clone(), g.likes.clone())
        };
        let s = engine.store.get();
        let p = s.active_profile();
        let played_str = if played.is_empty() { "нет".into() } else { played.join(", ") };
        let likes_str = if likes.is_empty() { "нет".into() } else { likes.join(", ") };
        let sys = "Ты музыкальный редактор и DJ. Верни ТОЛЬКО валидный JSON без markdown: \
            {\"station\":\"короткое атмосферное название станции\",\"tracks\":[\"Исполнитель - Трек 1\",...]} \
            Сгенерируй 6 реальных, узнаваемых композиций для YouTube-поиска, точно попадающих в запрошенный вайб.";
        let user = format!("Запрос вайба: {mood}\nПонравившиеся в этой сессии: {likes_str}\nУже звучали (не повторять): {played_str}");

        if let Ok(raw) = crate::llm::ask_with_pool(
            &engine.host,
            &s,
            &p,
            sys,
            &user,
            &engine.tools,
            Vec::new(),
            Some(&engine.gemini_pool),
        )
        .await
            && let Some((st, tr)) = parse_dj_json(&raw)
        {
            return (st, tr);
        }

        // Надёжный встроенный фолбэк по ключевым словам
        let low = mood.to_lowercase();
        if low.contains("токио") || low.contains("синт") || low.contains("synth") || low.contains("ноч") {
            (
                "Tokyo Midnight FM".into(),
                vec![
                    "Kavinsky Nightcall".into(),
                    "Timecop1983 On the Run".into(),
                    "The Midnight Sunset".into(),
                    "Gunship Tech Noir".into(),
                    "Perturbator Future Club".into(),
                    "Home Resonance".into(),
                ],
            )
        } else if low.contains("аниме") || low.contains("ost") {
            (
                "Otaku Late Radio".into(),
                vec![
                    "TK from Ling tosite sigure Unravel".into(),
                    "YOASOBI Idol".into(),
                    "Sawano Hiroyuki aLIEz".into(),
                    "Joe Hisaishi One Summer's Day".into(),
                    "Shiro Sagisu Thanatos".into(),
                    "Eve Kaikai Kitan".into(),
                ],
            )
        } else {
            (
                "YaJarvis Chill Mix".into(),
                vec![
                    "Nujabes Aruarian Dance".into(),
                    "Tycho Awake".into(),
                    "Khruangbin Time".into(),
                    "Daft Punk Veridis Quo".into(),
                    "M83 Midnight City".into(),
                    "Massive Attack Teardrop".into(),
                ],
            )
        }
    }

    async fn run_loop(self: Arc<Self>, engine: Arc<Engine>, generation: u64) {
        let mood = self.st.lock().unwrap().mood.clone();
        let (station, tracks) = self.generate_batch(&engine, &mood).await;
        {
            let mut g = self.st.lock().unwrap();
            if g.generation != generation {
                return;
            }
            g.station = station.clone();
            g.queue.extend(tracks);
        }

        while self.is_current(generation) {
            let next_track = {
                let mut g = self.st.lock().unwrap();
                g.queue.pop_front()
            };
            let Some(query) = next_track else {
                let cur_mood = self.st.lock().unwrap().mood.clone();
                let (_st, more) = self.generate_batch(&engine, &cur_mood).await;
                let mut g = self.st.lock().unwrap();
                if g.generation != generation || more.is_empty() {
                    break;
                }
                g.queue.extend(more);
                continue;
            };

            let (count, cur_station, cur_mood) = {
                let mut g = self.st.lock().unwrap();
                if g.generation != generation {
                    return;
                }
                g.current_track = query.clone();
                g.current_title = query.clone();
                g.played.push(query.clone());
                (g.played.len(), g.station.clone(), g.mood.clone())
            };

            // Останавливаем предыдущий трек перед подводкой, чтобы голос и музыка не накладывались
            if engine.music.state().playing {
                let _ = engine.music.call(&engine.host, "stop", Value::Null).await;
                tokio::time::sleep(Duration::from_millis(120)).await;
            }

            // Короткая живая подводка радиоведущего перед треком (каждый 3-й трек — второй персонаж в гостях!)
            let s = engine.store.get();
            let active_p = s.active_profile();
            let guest_p = if count % 3 == 0 && s.profiles.len() > 1 {
                s.profiles.iter().find(|pr| pr.id != active_p.id).cloned()
            } else {
                None
            };
            let host_p = guest_p.clone().unwrap_or_else(|| active_p.clone());
            let base = s.prompt_text(&host_p.prompt);
            let dj_sys = format!(
                "{base}\nТы ночной радио-DJ на станции «{cur_station}» (вайб: {cur_mood}). \
                Скажи короткую, тёплую подводку к следующему треку: «{query}». \
                Строго 1–2 плавных предложения по-русски без Markdown, скобок и спецсимволов. \
                Иностранные имена артистов и названия треков пиши по-русски кириллицей так, как они звучат на слух, чтобы диктор произнёс каждое слово чисто. {}",
                if count == 1 {
                    "Коротко поприветствуй слушателя и представь первый трек."
                } else if guest_p.is_some() {
                    "Ты на секунду заглянул в эфир к напарнику — в своём характере представь следующую композицию."
                } else {
                    "Плавно представь следующую композицию."
                }
            );
            let intro = crate::llm::ask_with_pool(
                &engine.host,
                &s,
                &host_p,
                &dj_sys,
                "Объяви трек.",
                &engine.tools,
                Vec::new(),
                Some(&engine.gemini_pool),
            )
            .await
            .unwrap_or_else(|_| format!("На волне {cur_station} звучит следующий трек."));

            if !self.is_current(generation) {
                break;
            }
            let clean_intro = crate::sfx::strip_tags(&intro);
            engine.journal.note("AI-DJ", &query, &clean_intro);
            let e_say = engine.clone();
            let speak_str = clean_intro.clone();
            let override_p = guest_p;
            let _ = tokio::task::spawn_blocking(move || {
                let _ = e_say.say_aloud_opts(&speak_str, Duration::from_secs(45), override_p.as_ref(), false);
            })
            .await;

            if !self.is_current(generation) {
                break;
            }

            // Небольшая пауза после голоса диджея, чтобы YouTube не срезал хвост последнего слова
            tokio::time::sleep(Duration::from_millis(250)).await;
            if !self.is_current(generation) {
                break;
            }

            let play_res = engine.music.play(&engine.host, &query, false).await;
            log::info!("AI-DJ: play({query}) -> {play_res}");

            // Ждём окончания трека или команды «Дальше / Стоп»
            let mut seen_playing = false;
            let mut wait_ticks = 0u32;
            while self.is_current(generation) {
                tokio::select! {
                    _ = self.wake.notified() => break,
                    _ = tokio::time::sleep(Duration::from_millis(1200)) => {}
                }
                if !self.is_current(generation) {
                    break;
                }
                let st = engine.music.state();
                if st.playing || st.paused {
                    seen_playing = true;
                    if let Some(t) = st.cur.get("Title").or_else(|| st.cur.get("title")).and_then(Value::as_str).filter(|x| !x.is_empty()) {
                        let mut g = self.st.lock().unwrap();
                        if g.generation == generation {
                            g.current_title = t.to_string();
                        }
                    }
                } else if seen_playing {
                    // Трек завершился — переходим к следующему
                    break;
                } else {
                    wait_ticks += 1;
                    if wait_ticks > 14 {
                        // Не удалось запустить трек за ~17 с — пропускаем к следующему
                        break;
                    }
                }
            }
        }

        let mut g = self.st.lock().unwrap();
        if g.generation == generation {
            g.active = false;
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        let g = self.st.lock().unwrap();
        g.active && g.generation == generation
    }
}

fn parse_dj_json(raw: &str) -> Option<(String, Vec<String>)> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end <= start {
        return None;
    }
    let val: Value = serde_json::from_str(&raw[start..=end]).ok()?;
    let station = val["station"].as_str().unwrap_or("Ночной эфир").trim().to_string();
    let tracks: Vec<String> = val["tracks"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if tracks.is_empty() {
        None
    } else {
        Some((station, tracks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dj_json_safely() {
        let raw = "Вот подборка:\n```json\n{\"station\": \"Neon Drive\", \"tracks\": [\"A - B\", \"C - D\"]}\n```";
        let (st, tr) = parse_dj_json(raw).unwrap();
        assert_eq!(st, "Neon Drive");
        assert_eq!(tr, vec!["A - B", "C - D"]);
    }

    #[test]
    fn dj_status_includes_music_sleep_countdown() {
        let d = AiDj::new();
        assert_eq!(d.status()["music_sleep_s"], 0);
        {
            let mut g = d.st.lock().unwrap();
            g.sleep_until_unix = jiff::Timestamp::now().as_second() + 600;
        }
        assert!(d.status()["music_sleep_s"].as_u64().unwrap_or(0) >= 595);
    }
}
