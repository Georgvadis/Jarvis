//! Голосовые игры для YaJarvis:
//! 1) «Угадай мелодию / Аниме-опенинг» — отрывок с YouTube + таймер на LED-дисплее + счёт;
//! 2) «Нуарный детектив» — расследование с 3 подозреваемыми с разными голосами/LED;
//! 3) «Ведущий Мафии / Бункера» — тайная раздача ролей в Telegram/панель + фазы + таймер дня.

use crate::{
    ambient::Ambient,
    engine::Engine,
    modes::ModeManager,
    plugins::Host,
    vpn::{Proxy, Route},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartyPlayer {
    pub name: String,
    #[serde(default)]
    pub chat_id: String,
}

#[derive(Clone, Debug)]
struct MelodyItem {
    query: String,
    display: String,
    aliases: Vec<String>,
}

#[derive(Default)]
struct MelodyState {
    active: bool,
    theme: String,
    tracks: Vec<MelodyItem>,
    round: usize,
    score: usize,
    awaiting_answer: bool,
    round_gen: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Suspect {
    name: String,
    role: String,
    trait_desc: String,
    alibi: String,
    secret: String,
}

#[derive(Default)]
struct DetectiveState {
    active: bool,
    title: String,
    intro: String,
    culprit_idx: usize,
    suspects: Vec<Suspect>,
    clues: Vec<String>,
    clues_shown: usize,
    current_suspect: Option<usize>,
    history: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize)]
struct DealtRole {
    name: String,
    role: String,
    detail: String,
    dm_sent: bool,
}

#[derive(Default)]
struct PartyState {
    active: bool,
    game: String, // "mafia" | "bunker"
    scenario: String,
    phase_idx: usize,
    round: usize,
    discussion_s: u64,
    roles: Vec<DealtRole>,
}

pub struct GameManager {
    modes: Arc<ModeManager>,
    ambient: Arc<Ambient>,
    melody: Mutex<MelodyState>,
    detective: Mutex<DetectiveState>,
    party: Mutex<PartyState>,
    timer_gen: AtomicU64,
}

impl GameManager {
    pub fn new(modes: Arc<ModeManager>, ambient: Arc<Ambient>) -> Arc<Self> {
        Arc::new(Self {
            modes,
            ambient,
            melody: Mutex::new(MelodyState::default()),
            detective: Mutex::new(DetectiveState::default()),
            party: Mutex::new(PartyState::default()),
            timer_gen: AtomicU64::new(0),
        })
    }

    pub fn is_any_active(&self) -> bool {
        self.melody.lock().unwrap().active
            || self.detective.lock().unwrap().active
            || self.party.lock().unwrap().active
    }

    pub fn stop_all(&self, engine: Option<&Engine>) {
        self.timer_gen.fetch_add(1, Ordering::SeqCst);
        let was_active = {
            let mut m = self.melody.lock().unwrap();
            let mut d = self.detective.lock().unwrap();
            let mut p = self.party.lock().unwrap();
            let was = m.active || d.active || p.active;
            m.active = false;
            m.awaiting_answer = false;
            m.round_gen = m.round_gen.wrapping_add(1);
            d.active = false;
            d.history.clear();
            p.active = false;
            p.roles.clear();
            was
        };
        if !was_active {
            return;
        }
        self.modes.reset(Some(&self.ambient));
        if let Some(eng) = engine {
            eng.leds.set_clock_digits(None, false);
            let music = eng.music.clone();
            let host = eng.host.clone();
            tokio::spawn(async move {
                let _ = music.call(&host, "stop", Value::Null).await;
            });
        }
    }

    pub fn status(&self) -> Value {
        let m = self.melody.lock().unwrap();
        let d = self.detective.lock().unwrap();
        let p = self.party.lock().unwrap();
        json!({
            "active_game": if m.active {
                "melody"
            } else if d.active {
                "detective"
            } else if p.active {
                "party"
            } else {
                ""
            },
            "melody": {
                "active": m.active,
                "theme": m.theme,
                "round": m.round,
                "total": m.tracks.len(),
                "score": m.score,
                "awaiting": m.awaiting_answer,
            },
            "detective": {
                "active": d.active,
                "title": d.title,
                "clues_shown": d.clues_shown,
                "clues_total": d.clues.len(),
                "suspects": d.suspects.iter().map(|s| json!({"name": s.name, "role": s.role})).collect::<Vec<_>>(),
            },
            "party": {
                "active": p.active,
                "game": p.game,
                "scenario": p.scenario,
                "round": p.round,
                "phase": party_phase_label(&p.game, p.phase_idx),
                "discussion_s": p.discussion_s,
                "roles": p.roles,
            }
        })
    }

    pub async fn start_melody(self: &Arc<Self>, engine: Arc<Engine>, theme: String) -> Result<Value, String> {
        if engine.host.provider("youtube").is_err() {
            return Err("Для игры «Угадай мелодию» нужен включённый модуль YouTube".into());
        }
        engine.tools.stop_for(&engine, "games");
        self.stop_all(None);
        self.ambient.stop();

        let theme_clean = if theme.trim().is_empty() {
            "аниме опенинги".into()
        } else {
            theme.trim().to_string()
        };
        let mut items = build_melody_pool(&engine, &theme_clean).await;
        shuffle_slice(&mut items);
        items.truncate(5);
        if items.is_empty() {
            return Err("Не удалось подобрать треки для темы".into());
        }

        let round_gen = {
            let mut m = self.melody.lock().unwrap();
            m.active = true;
            m.theme = theme_clean.clone();
            m.tracks = items;
            m.round = 0;
            m.score = 0;
            m.awaiting_answer = false;
            m.round_gen = m.round_gen.wrapping_add(1);
            m.round_gen
        };
        self.modes.set_mode(crate::modes::SpecialMode::Melody, Some(&self.ambient));
        engine.journal.note("игра", &format!("Угадай мелодию: {theme_clean}"), "");

        let me = self.clone();
        let eng = engine.clone();
        tokio::spawn(async move {
            let e0 = eng.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = e0.say_aloud(
                    "[sfx:chime] Стартуем «Угадай мелодию»! Слушай отрывок 8 секунд и называй трек.",
                    Duration::from_secs(20),
                );
            })
            .await;
            me.play_melody_round(eng, round_gen).await;
        });

        Ok(self.status())
    }

    async fn play_melody_round(self: Arc<Self>, engine: Arc<Engine>, round_gen: u64) {
        enum RoundStep {
            Stop,
            Finish(usize, usize),
            Play(MelodyItem),
        }
        let step = {
            let mut m = self.melody.lock().unwrap();
            if !m.active || m.round_gen != round_gen {
                RoundStep::Stop
            } else if m.round >= m.tracks.len() {
                let score = m.score;
                let total = m.tracks.len();
                m.active = false;
                RoundStep::Finish(score, total)
            } else {
                m.awaiting_answer = false;
                RoundStep::Play(m.tracks[m.round].clone())
            }
        };
        let track = match step {
            RoundStep::Stop => return,
            RoundStep::Finish(score, total) => {
                self.modes.reset(Some(&self.ambient));
                let msg = format!("[sfx:fanfare] Игра окончена! Твой счёт: {score} из {total}.");
                engine.journal.note("игра", "Угадай мелодию: финал", &crate::sfx::strip_tags(&msg));
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = engine.say_aloud(&msg, Duration::from_secs(20));
                })
                .await;
                return;
            }
            RoundStep::Play(t) => t,
        };

        let search_res = engine
            .music
            .call(&engine.host, "search", json!({"query": track.query, "type": "video"}))
            .await;
        let vid = search_res.ok().and_then(|v| {
            let arr = v["items"].as_array().or_else(|| v["Items"].as_array())?;
            let first = arr.first()?;
            first["id"]
                .as_str()
                .or_else(|| first["ID"].as_str())
                .or_else(|| first["videoId"].as_str())
                .or_else(|| first["video_id"].as_str())
                .map(str::to_string)
        });

        let started = if let Some(video_id) = vid {
            engine
                .music
                .call(&engine.host, "play_id", json!({"id": video_id, "title": track.display}))
                .await
        } else {
            // Если search не вернул ID — запускаем напрямую через стандартный play по запросу
            engine
                .music
                .call(&engine.host, "play", json!({"query": track.query}))
                .await
        };

        if let Err(err) = started {
            log::warn!("Угадай мелодию: не удалось включить «{}»: {err}", track.query);
            {
                let mut m = self.melody.lock().unwrap();
                if m.round_gen == round_gen {
                    m.round += 1;
                }
            }
            Box::pin(self.play_melody_round(engine, round_gen)).await;
            return;
        }

        for _ in 0..16 {
            if self.melody.lock().unwrap().round_gen != round_gen {
                let _ = engine.music.call(&engine.host, "stop", Value::Null).await;
                return;
            }
            if engine.music.state().playing {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let _ = engine.music.call(&engine.host, "seek", json!({"delta": 35.0, "abs": true})).await;

        for rem in (1..=8u8).rev() {
            if self.melody.lock().unwrap().round_gen != round_gen {
                engine.leds.set_clock_digits(None, false);
                let _ = engine.music.call(&engine.host, "stop", Value::Null).await;
                return;
            }
            engine.leds.set_clock_digits(Some([0, 0, rem / 10, rem % 10]), true);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        engine.leds.set_clock_digits(None, false);
        let _ = engine.music.call(&engine.host, "stop", Value::Null).await;

        let round_num = {
            let mut m = self.melody.lock().unwrap();
            if !m.active || m.round_gen != round_gen {
                return;
            }
            m.awaiting_answer = true;
            m.round + 1
        };
        let prompt = format!("Раунд {round_num}. Время! Как называется этот трек?");
        let eng_say = engine.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let _ = eng_say.say_aloud(&prompt, Duration::from_secs(18));
            if eng_say.mic.online() {
                eng_say.trigger("игра угадай мелодию", 1.0);
            }
        })
        .await;
    }

    pub async fn start_detective(self: &Arc<Self>, engine: Arc<Engine>, theme: String) -> Result<Value, String> {
        engine.tools.stop_for(&engine, "games");
        self.stop_all(None);
        let _ = self.ambient.start("jazz");

        let case = generate_detective_case(&engine, theme.trim()).await;
        let title = case.title.clone();
        let intro = case.intro.clone();
        let suspects_list = case
            .suspects
            .iter()
            .map(|s| format!("{} ({})", s.name, s.role))
            .collect::<Vec<_>>()
            .join(", ");

        {
            let mut d = self.detective.lock().unwrap();
            *d = case;
            d.active = true;
        }
        self.modes.set_mode(crate::modes::SpecialMode::Detective, Some(&self.ambient));

        let opening = format!(
            "[sfx:thunder] Дело «{title}». {intro} Подозреваемые: {suspects_list}. Кого допросишь первым? Можешь также попросить подсказку или сказать «обвиняю ...»."
        );
        engine.journal.note("детектив", &title, &crate::sfx::strip_tags(&opening));
        let eng = engine.clone();
        let say_str = opening.clone();
        tokio::task::spawn_blocking(move || {
            let _ = eng.say_aloud(&say_str, Duration::from_secs(45));
            if eng.mic.online() {
                eng.trigger("нуарный детектив", 1.0);
            }
        });
        Ok(self.status())
    }

    pub async fn start_party(
        self: &Arc<Self>,
        engine: Arc<Engine>,
        game: String,
        players: Vec<PartyPlayer>,
        discussion_s: u64,
    ) -> Result<Value, String> {
        if players.len() < 3 || players.len() > 16 {
            return Err("Нужно от 3 до 16 игроков".into());
        }
        engine.tools.stop_for(&engine, "games");
        self.stop_all(None);
        let game = if game == "bunker" { "bunker" } else { "mafia" }.to_string();
        let disc_s = discussion_s.clamp(30, 300);

        let (scenario, mut dealt) = if game == "bunker" {
            deal_bunker_roles(&players)
        } else {
            deal_mafia_roles(&players)
        };

        // Отправляем личные роли в Telegram всем игрокам, у кого указан chat_id
        for role_entry in &mut dealt {
            if let Some(pl) = players.iter().find(|p| p.name == role_entry.name)
                && !pl.chat_id.trim().is_empty()
            {
                let msg = if game == "mafia" {
                    format!("🎭 Мафия — твоя роль: {}\n\n{}", role_entry.role, role_entry.detail)
                } else {
                    format!("☢️ Бункер ({scenario}) — твой персонаж: {}\n\n{}", role_entry.role, role_entry.detail)
                };
                if send_tg_dm(&engine.host, &engine.proxy, pl.chat_id.trim(), &msg).await.is_ok() {
                    role_entry.dm_sent = true;
                }
            }
        }

        {
            let mut p = self.party.lock().unwrap();
            p.active = true;
            p.game = game.clone();
            p.scenario = scenario.clone();
            p.phase_idx = 0;
            p.round = 1;
            p.discussion_s = disc_s;
            p.roles = dealt;
        }
        self.modes.set_mode(crate::modes::SpecialMode::PartyHost, Some(&self.ambient));

        let intro = if game == "mafia" {
            format!(
                "[sfx:bell] Партия в «Мафию» началась, за столом {} игроков! Роли розданы. Знакомьтесь, и когда будете готовы к первой ночи — скажи «следующая фаза».",
                players.len()
            )
        } else {
            format!(
                "[sfx:alert] Игра «Бункер»! Сценарий катастрофы: {scenario}. Карты розданы {} выжившим. Для перехода к следующему шагу скажи «дальше».",
                players.len()
            )
        };
        engine.journal.note("ведущий", &game, &crate::sfx::strip_tags(&intro));
        let eng = engine.clone();
        let line = intro.clone();
        tokio::task::spawn_blocking(move || {
            let _ = eng.say_aloud(&line, Duration::from_secs(30));
        });
        Ok(self.status())
    }

    pub async fn party_next_phase(self: &Arc<Self>, engine: &Arc<Engine>) -> Result<String, String> {
        let (active, game, next_idx, round, disc_s) = {
            let mut p = self.party.lock().unwrap();
            if !p.active {
                return Err("Нет активной игры Мафии или Бункера".into());
            }
            let phases = if p.game == "mafia" { 4 } else { 3 };
            p.phase_idx = (p.phase_idx + 1) % phases;
            if p.phase_idx == 0 {
                p.round += 1;
            }
            (p.active, p.game.clone(), p.phase_idx, p.round, p.discussion_s)
        };
        if !active {
            return Err("Игра остановлена".into());
        }

        self.timer_gen.fetch_add(1, Ordering::SeqCst);
        engine.leds.set_clock_digits(None, false);

        let text = if game == "mafia" {
            match next_idx {
                1 => {
                    let _ = self.ambient.start("rain");
                    format!(
                        "[sfx:thunder] [whisper] Раунд {round}. Город засыпает... Мафия открывает глаза и выбирает цель. Когда закончите с ночью — скажи «дальше»."
                    )
                }
                2 => {
                    self.ambient.stop();
                    self.spawn_discussion_timer(engine.clone(), disc_s);
                    format!(
                        "[sfx:bell] [happy] Наступает утро раунда {round}! Город просыпается. У вас {disc_s} секунд на обсуждение — таймер запущен!"
                    )
                }
                3 => "[sfx:tick] Время обсуждения вышло! Переходите к голосованию на выбывание.".into(),
                _ => {
                    self.ambient.stop();
                    format!("[sfx:chime] Новый день, раунд {round}. Продолжаем игру!")
                }
            }
        } else {
            match next_idx {
                1 => {
                    self.spawn_discussion_timer(engine.clone(), disc_s);
                    format!("[sfx:chime] Раунд {round}: каждый по очереди открывает одну характеристику. У стола {disc_s} секунд на спор!")
                }
                2 => "[sfx:tick] Время истекло! Объявляю голосование — кто остаётся за дверью бункера?".into(),
                _ => format!("[sfx:bell] Начинаем раунд {round} обсуждения выживших."),
            }
        };

        let clean = crate::sfx::strip_tags(&text);
        engine.journal.note("ведущий", "", &clean);
        let e = engine.clone();
        let line = text.clone();
        tokio::task::spawn_blocking(move || {
            let _ = e.say_aloud(&line, Duration::from_secs(30));
        });
        Ok(clean)
    }

    fn spawn_discussion_timer(&self, engine: Arc<Engine>, seconds: u64) {
        let tgen = self.timer_gen.fetch_add(1, Ordering::SeqCst) + 1;
        tokio::spawn(async move {
            for rem in (1..=seconds).rev() {
                engine.leds.set_clock_digits(
                    Some([(rem / 600) as u8 % 10, ((rem / 60) % 10) as u8, ((rem % 60) / 10) as u8, (rem % 10) as u8]),
                    rem % 2 == 0,
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
                if engine.tools.games.as_ref().is_none_or(|gm| gm.timer_gen.load(Ordering::Relaxed) != tgen) {
                    engine.leds.set_clock_digits(None, false);
                    return;
                }
            }
            engine.leds.set_clock_digits(None, false);
            let _ = tokio::task::spawn_blocking(move || {
                let _ = engine.say_aloud("[sfx:bell] Время обсуждения вышло! Начинайте голосование.", Duration::from_secs(15));
            })
            .await;
        });
    }

    pub async fn handle_turn(self: &Arc<Self>, engine: &Arc<Engine>, user_text: &str) -> Option<String> {
        let low = user_text.trim().to_lowercase();
        if low.contains("стоп игра") || low.contains("хватит играть") || low.contains("выключи игру") || low.contains("закончи игру") {
            self.stop_all(Some(engine));
            return Some("Игру завершил, возвращаюсь в обычный режим.".into());
        }

        if self.melody.lock().unwrap().active {
            return Some(self.handle_melody_turn(engine, user_text).await);
        }
        if self.detective.lock().unwrap().active {
            return Some(self.handle_detective_turn(engine, user_text).await);
        }
        if self.party.lock().unwrap().active {
            if low.contains("дальше") || low.contains("следующ") || low.contains("ночь") || low.contains("день") || low.contains("голосовани") || low.contains("таймер") {
                return Some(self.party_next_phase(engine).await.unwrap_or_else(|e| e));
            }
            return Some("Я веду партию: когда будете готовы перейти дальше, скажи «следующая фаза» или «дальше».".into());
        }
        None
    }

    pub async fn skip_melody(self: &Arc<Self>, engine: &Arc<Engine>) -> Result<Value, String> {
        let next_gen = {
            let mut m = self.melody.lock().unwrap();
            if !m.active {
                return Err("Игра «Угадай мелодию» не запущена".into());
            }
            m.round += 1;
            m.awaiting_answer = false;
            m.round_gen = m.round_gen.wrapping_add(1);
            m.round_gen
        };
        engine.leds.set_clock_digits(None, false);
        let _ = engine.music.call(&engine.host, "stop", Value::Null).await;
        let me = self.clone();
        let eng = engine.clone();
        tokio::spawn(async move {
            me.play_melody_round(eng, next_gen).await;
        });
        Ok(self.status())
    }

    async fn handle_melody_turn(self: &Arc<Self>, engine: &Arc<Engine>, user_text: &str) -> String {
        let (track, round_gen, total) = {
            let mut m = self.melody.lock().unwrap();
            if !m.active {
                return "Игра завершена.".into();
            }
            if !m.awaiting_answer {
                return "Дослушай отрывок и потом назови трек.".into();
            }
            m.awaiting_answer = false;
            (m.tracks[m.round].clone(), m.round_gen, m.tracks.len())
        };
        let low = user_text.trim().to_lowercase();
        let skipped = is_melody_skip(&low);
        let mut matched = !skipped
            && !low.is_empty()
            && (track.aliases.iter().any(|a| !a.is_empty() && low.contains(a))
                || track.display.to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|w| w.chars().count() >= 4 && low.contains(w)));

        if !skipped && !matched && low.chars().count() >= 2 {
            let s = engine.store.get();
            let p = s.active_profile();
            let check = crate::llm::ask_with_pool(
                &engine.host,
                &s,
                &p,
                "Ты судья викторины «Угадай мелодию». Ответь ТОЛЬКО словом ДА или НЕТ: угадал ли игрок загаданный трек или аниме/исполнителя.",
                &format!("Загадано: «{}». Игрок сказал: «{}».", track.display, user_text),
                &engine.tools,
                Vec::new(),
                Some(&engine.gemini_pool),
            )
            .await
            .unwrap_or_default();
            if check.trim().to_lowercase().starts_with("да") {
                matched = true;
            }
        }

        let score = {
            let mut m = self.melody.lock().unwrap();
            if matched {
                m.score += 1;
            }
            m.round += 1;
            m.score
        };

        let reply = if skipped {
            format!("[sfx:buzz] Пропускаем! Это была «{}». Счёт: {score} из {total}.", track.display)
        } else if matched {
            format!("[sfx:fanfare] [happy] Верно! Это «{}». Счёт: {score} из {total}.", track.display)
        } else {
            format!("[sfx:buzz] [sad] Мимо! Это была «{}». Счёт: {score} из {total}.", track.display)
        };

        let pause_s = if skipped { 4 } else { 5 };
        let me = self.clone();
        let eng = engine.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(pause_s)).await;
            me.play_melody_round(eng, round_gen).await;
        });
        reply
    }

    async fn handle_detective_turn(self: &Arc<Self>, engine: &Arc<Engine>, user_text: &str) -> String {
        let low = user_text.trim().to_lowercase();
        let (title, culprit_idx, suspects, clues, clues_shown, mut cur_suspect, history) = {
            let d = self.detective.lock().unwrap();
            (
                d.title.clone(),
                d.culprit_idx,
                d.suspects.clone(),
                d.clues.clone(),
                d.clues_shown,
                d.current_suspect,
                d.history.clone(),
            )
        };

        if low.contains("подозреваем") || low.contains("список") || low.contains("кто в деле") || low.contains("напомни дело") {
            let list = suspects
                .iter()
                .map(|s| format!("{} ({})", s.name, s.role))
                .collect::<Vec<_>>()
                .join(", ");
            return format!(
                "Дело «{title}». Подозреваемые: {list}. Улик найдено: {clues_shown} из {}.",
                clues.len()
            );
        }

        if low.contains("обвиня") || low.contains("убийца") || low.contains("виновен") || low.contains("арест") {
            let guessed = suspects.iter().position(|s| {
                let first = s.name.split_whitespace().next().unwrap_or("").to_lowercase();
                (!first.is_empty() && low.contains(&first)) || low.contains(&s.role.to_lowercase())
            });
            return if let Some(idx) = guessed {
                let culprit = &suspects[culprit_idx];
                self.stop_all(Some(engine));
                if idx == culprit_idx {
                    format!(
                        "[sfx:fanfare] [excited] Блестяще, детектив! Виновный — {} ({}) действительно совершил преступление: {}. Дело «{title}» раскрыто!",
                        culprit.name, culprit.role, culprit.secret
                    )
                } else {
                    format!(
                        "[sfx:buzz] [sad] Увы, арестован не тот! Настоящим преступником был {} ({}) — {}. Попробуем новое дело в другой раз.",
                        culprit.name, culprit.role, culprit.secret
                    )
                }
            } else {
                let names = suspects.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
                format!("Кого именно ты обвиняешь? Назови имя: {names}.")
            };
        }

        if low.contains("подсказ") || low.contains("улик") || low.contains("нашли") {
            let mut d = self.detective.lock().unwrap();
            if d.clues_shown < d.clues.len() {
                let clue = d.clues[d.clues_shown].clone();
                d.clues_shown += 1;
                return format!("[sfx:chime] Новая улика по делу: {clue}");
            }
            return "Все улики уже у тебя на столе. Время сопоставить показания и назвать виновного!".into();
        }

        if let Some(idx) = suspects.iter().position(|s| {
            let first = s.name.split_whitespace().next().unwrap_or("").to_lowercase();
            (!first.is_empty() && low.contains(&first)) || low.contains(&s.role.to_lowercase())
        }) {
            cur_suspect = Some(idx);
            self.detective.lock().unwrap().current_suspect = cur_suspect;
        }

        let Some(s_idx) = cur_suspect else {
            let names = suspects.iter().map(|s| format!("{} ({})", s.name, s.role)).collect::<Vec<_>>().join(", ");
            return format!("Перед тобой {names}. Скажи, кого допросить первым (например, «допросить дворецкого») или попроси улику.");
        };

        let sp = &suspects[s_idx];
        let is_culprit = s_idx == culprit_idx;
        let recent = history
            .iter()
            .rev()
            .take(6)
            .rev()
            .map(|(w, t)| format!("{w}: {t}"))
            .collect::<Vec<_>>()
            .join("\n");
        let clues_known = clues.iter().take(clues_shown).cloned().collect::<Vec<_>>().join("; ");

        let s = engine.store.get();
        let prof = s.active_profile();
        let sys = format!(
            "Ты играешь роль персонажа «{name}» ({role}) в нуар-детективе «{title}». \
            Характер: {trait_desc}. Твоё алиби: {alibi}. Твоя тайна: {secret}. \
            {culprit_rule} \
            Известные улики: {clues_known}. Отвечай строго от первого лица (2–3 коротких живых предложения, без Markdown), \
            начни с эмоционального тега [sarcastic], [sad], [angry] или [calm] (НЕ пиши префикс имени в ответе).\nИстория:\n{recent}",
            name = sp.name,
            role = sp.role,
            trait_desc = sp.trait_desc,
            alibi = sp.alibi,
            secret = sp.secret,
            culprit_rule = if is_culprit {
                "Ты НАСТОЯЩИЙ преступник, но отчаянно выкручивайся, нервничай при неудобных вопросах и не признавайся напрямую."
            } else {
                "Ты НЕ убийца, но можешь что-то недоговаривать из-за своей тайны."
            }
        );

        let raw_ans = crate::llm::ask_with_pool(
            &engine.host,
            &s,
            &prof,
            &sys,
            user_text,
            &engine.tools,
            Vec::new(),
            Some(&engine.gemini_pool),
        )
        .await
        .unwrap_or_else(|_| "[calm] Мне больше нечего добавить к своим показаниям, детектив.".into());

        let clean_ans = crate::sfx::strip_tags(&raw_ans);
        {
            let mut d = self.detective.lock().unwrap();
            d.history.push(("Детектив".into(), user_text.to_string()));
            d.history.push((sp.name.clone(), clean_ans));
        }
        format!("[{}]: {}", sp.name, raw_ans.trim())
    }
}

pub fn fast_game_command(games: &Arc<GameManager>, engine: &Arc<Engine>, text: &str) -> Option<String> {
    let low = text.trim().to_lowercase();
    if (low.contains("угадай мелод") || low.contains("угадай опенинг") || low.contains("аниме опенинг"))
        && !games.melody.lock().unwrap().active
    {
        let gm = games.clone();
        let eng = engine.clone();
        let theme = if low.contains("аниме") || low.contains("опенинг") {
            "аниме опенинги"
        } else if low.contains("рок") {
            "рок"
        } else {
            "хиты"
        };
        tokio::spawn(async move {
            let _ = gm.start_melody(eng, theme.into()).await;
        });
        return Some(format!("Запускаю «Угадай мелодию» ({theme})!"));
    }
    if (low.contains("детектив") || low.contains("расследован"))
        && (low.contains("запусти") || low.contains("сыграем") || low.contains("начни") || low.contains("нуар"))
        && !games.detective.lock().unwrap().active
    {
        let gm = games.clone();
        let eng = engine.clone();
        tokio::spawn(async move {
            let _ = gm.start_detective(eng, String::new()).await;
        });
        return Some("Открываю новое нуарное дело...".into());
    }
    None
}

async fn build_melody_pool(engine: &Engine, theme: &str) -> Vec<MelodyItem> {
    let low = theme.to_lowercase();
    if low.contains("аниме") || low.contains("опенинг") {
        return vec![
            MelodyItem { query: "KANA-BOON Silhouette Naruto".into(), display: "Наруто — Silhouette (KANA-BOON)".into(), aliases: vec!["наруто".into(), "naruto".into(), "silhouette".into(), "силуэт".into()] },
            MelodyItem { query: "Linked Horizon Guren no Yumiya".into(), display: "Атака титанов — Guren no Yumiya".into(), aliases: vec!["атака титанов".into(), "титан".into(), "attack on titan".into(), "shingeki".into()] },
            MelodyItem { query: "Yoko Takahashi A Cruel Angel's Thesis".into(), display: "Евангелион — Cruel Angel's Thesis".into(), aliases: vec!["евангелион".into(), "evangelion".into(), "ева".into(), "ангел".into()] },
            MelodyItem { query: "TK from Ling tosite sigure Unravel".into(), display: "Токийский гуль — Unravel".into(), aliases: vec!["гуль".into(), "токийский".into(), "ghoul".into(), "unravel".into()] },
            MelodyItem { query: "LiSA Gurenge Demon Slayer".into(), display: "Клинок, рассекающий демонов — Gurenge".into(), aliases: vec!["клинок".into(), "демон".into(), "demon slayer".into(), "gurenge".into()] },
            MelodyItem { query: "Kenshi Yonezu KICK BACK Chainsaw Man".into(), display: "Человек-бензопила — KICK BACK".into(), aliases: vec!["бензопила".into(), "chainsaw".into(), "kick back".into()] },
            MelodyItem { query: "Nightmare The World Death Note".into(), display: "Тетрадь смерти — The World".into(), aliases: vec!["тетрадь смерти".into(), "death note".into(), "world".into()] },
            MelodyItem { query: "Eve Kaikai Kitan Jujutsu Kaisen".into(), display: "Магическая битва — Kaikai Kitan".into(), aliases: vec!["магическая".into(), "битва".into(), "jujutsu".into(), "годжо".into()] },
        ];
    }
    if low.contains("рок") {
        return vec![
            MelodyItem { query: "Queen Bohemian Rhapsody official".into(), display: "Queen — Bohemian Rhapsody".into(), aliases: vec!["queen".into(), "квин".into(), "богем".into()] },
            MelodyItem { query: "Nirvana Smells Like Teen Spirit".into(), display: "Nirvana — Smells Like Teen Spirit".into(), aliases: vec!["nirvana".into(), "нирван".into(), "teen spirit".into()] },
            MelodyItem { query: "Linkin Park Numb".into(), display: "Linkin Park — Numb".into(), aliases: vec!["linkin".into(), "линкин".into(), "numb".into()] },
            MelodyItem { query: "Кино Группа крови".into(), display: "Кино — Группа крови".into(), aliases: vec!["кино".into(), "цой".into(), "группа крови".into()] },
            MelodyItem { query: "Король и Шут Лесник".into(), display: "Король и Шут — Лесник".into(), aliases: vec!["киш".into(), "король".into(), "лесник".into()] },
        ];
    }
    if low.contains("игр") || low.contains("game") {
        return vec![
            MelodyItem { query: "DOOM Eternal The Only Thing They Fear Is You".into(), display: "DOOM Eternal OST".into(), aliases: vec!["дум".into(), "doom".into()] },
            MelodyItem { query: "Skyrim Dragonborn Jeremy Soule".into(), display: "The Elder Scrolls V: Skyrim".into(), aliases: vec!["скайрим".into(), "skyrim".into(), "драконорожденн".into()] },
            MelodyItem { query: "Witcher 3 Silver for Monsters".into(), display: "Ведьмак 3 — Silver for Monsters".into(), aliases: vec!["ведьмак".into(), "witcher".into()] },
            MelodyItem { query: "Cyberpunk 2077 Rebel Path".into(), display: "Cyberpunk 2077 — The Rebel Path".into(), aliases: vec!["киберпанк".into(), "cyberpunk".into()] },
            MelodyItem { query: "Nier Automata Weight of the World".into(), display: "NieR: Automata OST".into(), aliases: vec!["ниер".into(), "nier".into()] },
        ];
    }

    // Пользовательская тема — подбираем через LLM
    let s = engine.store.get();
    let p = s.active_profile();
    let sys = "Верни ТОЛЬКО JSON-массив из 5 песен по теме игрока: \
        [{\"query\":\"Artist Song\",\"display\":\"Artist — Song\",\"aliases\":[\"слово1\",\"слово2\"]}]";
    if let Ok(raw) = crate::llm::ask_with_pool(
        &engine.host,
        &s,
        &p,
        sys,
        theme,
        &engine.tools,
        Vec::new(),
        Some(&engine.gemini_pool),
    )
    .await
        && let (Some(a), Some(b)) = (raw.find('['), raw.rfind(']'))
        && let Ok(arr) = serde_json::from_str::<Vec<Value>>(&raw[a..=b])
    {
        let parsed: Vec<MelodyItem> = arr
            .into_iter()
            .filter_map(|v| {
                let q = v["query"].as_str()?.trim().to_string();
                let d = v["display"].as_str()?.trim().to_string();
                let al: Vec<String> = v["aliases"]
                    .as_array()
                    .map(|xs| xs.iter().filter_map(Value::as_str).map(|x| x.to_lowercase()).collect())
                    .unwrap_or_default();
                (!q.is_empty() && !d.is_empty()).then_some(MelodyItem { query: q, display: d, aliases: al })
            })
            .collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }

    vec![
        MelodyItem { query: "The Weeknd Blinding Lights".into(), display: "The Weeknd — Blinding Lights".into(), aliases: vec!["weeknd".into(), "blinding".into()] },
        MelodyItem { query: "Daft Punk Get Lucky".into(), display: "Daft Punk — Get Lucky".into(), aliases: vec!["daft".into(), "lucky".into()] },
        MelodyItem { query: "a-ha Take On Me".into(), display: "a-ha — Take On Me".into(), aliases: vec!["take on me".into(), "aha".into()] },
        MelodyItem { query: "Rick Astley Never Gonna Give You Up".into(), display: "Rick Astley — Never Gonna Give You Up".into(), aliases: vec!["rick".into(), "рик".into(), "never gonna".into()] },
        MelodyItem { query: "Imagine Dragons Believer".into(), display: "Imagine Dragons — Believer".into(), aliases: vec!["dragons".into(), "believer".into()] },
    ]
}

async fn generate_detective_case(engine: &Engine, theme: &str) -> DetectiveState {
    let s = engine.store.get();
    let p = s.active_profile();
    let sys = "Придумай компактное нуарное детективное дело с 3 подозреваемыми. Верни ТОЛЬКО валидный JSON: \
        {\"title\":\"...\",\"intro\":\"2 предложения завязки\",\"culprit_idx\":0,\"suspects\":[{\"name\":\"Имя\",\"role\":\"роль\",\"trait_desc\":\"манера речи\",\"alibi\":\"...\",\"secret\":\"...\"}],\"clues\":[\"улика1\",\"улика2\",\"улика3\"]}";
    let prompt = if theme.is_empty() {
        "Классический нуар: дождливый особняк или клуб 1940-х".into()
    } else {
        format!("Тема дела: {theme}")
    };
    if let Ok(raw) = crate::llm::ask_with_pool(
        &engine.host,
        &s,
        &p,
        sys,
        &prompt,
        &engine.tools,
        Vec::new(),
        Some(&engine.gemini_pool),
    )
    .await
        && let (Some(a), Some(b)) = (raw.find('{'), raw.rfind('}'))
        && let Ok(v) = serde_json::from_str::<Value>(&raw[a..=b])
        && let Ok(suspects) = serde_json::from_value::<Vec<Suspect>>(v["suspects"].clone())
        && suspects.len() >= 2
    {
        let clues = v["clues"]
            .as_array()
            .map(|c| c.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>())
            .unwrap_or_default();
        let idx = (v["culprit_idx"].as_u64().unwrap_or(0) as usize).min(suspects.len() - 1);
        return DetectiveState {
            active: true,
            title: v["title"].as_str().unwrap_or("Дело в сумерках").to_string(),
            intro: v["intro"].as_str().unwrap_or("Ночью прозвучал выстрел, и все двери оказались заперты изнутри.").to_string(),
            culprit_idx: idx,
            suspects,
            clues,
            clues_shown: 0,
            current_suspect: None,
            history: Vec::new(),
        };
    }

    DetectiveState {
        active: true,
        title: "Полуночный экспресс".into(),
        intro: "В частном особняке Блэквуд прямо во время приёма пропало редкое ожерелье и найден оглушённым хозяин дома.".into(),
        culprit_idx: 1,
        suspects: vec![
            Suspect {
                name: "Виктор".into(),
                role: "дворецкий".into(),
                trait_desc: "чопорный, говорит медленно".into(),
                alibi: "протирал бокалы в буфетной".into(),
                secret: "тайно продал фамильное вино коллекционеру, но кражу не совершал".into(),
            },
            Suspect {
                name: "Марина".into(),
                role: "пианистка".into(),
                trait_desc: "самоуверенная, с иронией".into(),
                alibi: "якобы репетировала в зимнем саду".into(),
                secret: "отключила камеру в коридоре заколкой и подсыпала снотворное хозяину, чтобы забрать ожерелье".into(),
            },
            Suspect {
                name: "Артур".into(),
                role: "деловой партнёр".into(),
                trait_desc: "нервный и торопливый".into(),
                alibi: "вышел на террасу позвонить".into(),
                secret: "крупно задолжал хозяину, но в библиотеку не заходил".into(),
            },
        ],
        clues: vec![
            "На пультe видеонаблюдения осталась царапина от тонкой дамской шпильки.".into(),
            "Рояль в зимнем саду был накрыт чехлом весь вечер — никто на нём не играл.".into(),
            "На бокале хозяина найдены следы снотворного и помады.".into(),
        ],
        clues_shown: 0,
        current_suspect: None,
        history: Vec::new(),
    }
}

fn deal_mafia_roles(players: &[PartyPlayer]) -> (String, Vec<DealtRole>) {
    let n = players.len();
    let maf_count = if n >= 9 { 3 } else if n >= 6 { 2 } else { 1 };
    let mut pool: Vec<(&str, &str)> = Vec::new();
    pool.push(("Дон мафии", "Возглавляешь мафию, ищешь Комиссара и просыпаешься с командой ночью."));
    for _ in 1..maf_count {
        pool.push(("Мафия", "Просыпаешься ночью вместе с Доном, устраняешь мирных и отводишь подозрения днём."));
    }
    pool.push(("Комиссар", "Каждую ночь проверяешь одного игрока: узнаёшь, мафия он или мирный."));
    if n >= 5 {
        pool.push(("Доктор", "Каждую ночь можешь вылечить одного игрока (в том числе себя)."));
    }
    while pool.len() < n {
        pool.push(("Мирный житель", "Вычисляй мафию днём по аргументам и интонациям."));
    }
    shuffle_slice(&mut pool);
    let dealt = players
        .iter()
        .zip(pool)
        .map(|(pl, (r, d))| DealtRole {
            name: pl.name.clone(),
            role: r.into(),
            detail: d.into(),
            dm_sent: false,
        })
        .collect();
    ("Классическая Мафия".into(), dealt)
}

fn deal_bunker_roles(players: &[PartyPlayer]) -> (String, Vec<DealtRole>) {
    let disasters = [
        "Солнечная вспышка и магнитный шторм (на 2 года под землёй)",
        "Глобальное оледенение (-55°C на поверхности)",
        "Биоугроза в атмосфере, бункер вмещает лишь половину группы",
    ];
    let mut profs = vec!["Хирург", "Инженер-энергетик", "Биолог", "Повар", "Программист", "Психолог", "Пожарный", "Механик", "Фермер", "Учитель", "Архитектор", "Спасатель"];
    let mut healths = vec!["Идеальное здоровье", "Астма", "Близорукость -3", "Спортивная подготовка", "Лёгкая клаустрофобия", "Крепкий иммунитет", "Больное колено", "Идеальный слух"];
    let mut hobbies = vec!["Гидропоника", "Радиодело", "Настольные игры", "Выживание в лесу", "Ремонт электроники", "Первая помощь", "Гитара", "Кулинария"];
    let mut bags = vec!["Набор инструментов", "Медицинская аптечка", "Семена овощей", "Рация и батареи", "Фильтр для воды", "Энциклопедия ремонта", "Тепловизор", "Запас специй"];

    shuffle_slice(&mut profs);
    shuffle_slice(&mut healths);
    shuffle_slice(&mut hobbies);
    shuffle_slice(&mut bags);

    let sc = disasters[rand_idx(disasters.len())];
    let dealt = players
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let prof = profs[i % profs.len()];
            let hl = healths[i % healths.len()];
            let hb = hobbies[i % hobbies.len()];
            let bg = bags[i % bags.len()];
            DealtRole {
                name: pl.name.clone(),
                role: prof.into(),
                detail: format!("Профессия: {prof}\nЗдоровье: {hl}\nХобби: {hb}\nБагаж: {bg}"),
                dm_sent: false,
            }
        })
        .collect();
    (sc.into(), dealt)
}

fn party_phase_label(game: &str, phase_idx: usize) -> &'static str {
    if game == "mafia" {
        match phase_idx {
            0 => "Знакомство",
            1 => "Ночь",
            2 => "День и обсуждение",
            _ => "Голосование",
        }
    } else if game == "bunker" {
        match phase_idx {
            0 => "Катастрофа",
            1 => "Раскрытие характеристик",
            _ => "Голосование",
        }
    } else {
        ""
    }
}

fn rand_idx(len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let mut buf = [0u8; 4];
    let _ = getrandom::fill(&mut buf);
    (u32::from_le_bytes(buf) as usize) % len
}

fn shuffle_slice<T>(slice: &mut [T]) {
    for i in (1..slice.len()).rev() {
        let j = rand_idx(i + 1);
        slice.swap(i, j);
    }
}

async fn send_tg_dm(host: &Host, proxy: &Proxy, chat_id: &str, text: &str) -> Result<(), String> {
    let token = host.plugin_setting_str("telegram-sender", "bot_token");
    if token.is_empty() || chat_id.is_empty() {
        return Err("Telegram-бот не настроен".into());
    }
    let client = match proxy.route("plugins") {
        Route::Direct => crate::net::direct().clone(),
        Route::Tunnel(t) => crate::net::via_socks(&t.socks, &t.user, &t.pass, Duration::from_secs(15))?,
        Route::Blocked(why) => return Err(why),
    };
    let resp = client
        .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
        .json(&json!({"chat_id": chat_id, "text": text}))
        .timeout(Duration::from_secs(12))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Telegram HTTP {}", resp.status()));
    }
    Ok(())
}

pub(crate) fn is_melody_skip(low: &str) -> bool {
    let low = low.trim();
    ["дальше", "пропустить", "пропуск", "не знаю", "сдаюсь", "пас", "следующая", "следующий"]
        .iter()
        .any(|p| low == *p || low.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deals_valid_mafia_and_bunker_roles() {
        let players: Vec<PartyPlayer> = ["Анна", "Борис", "Вера", "Глеб", "Дима", "Ева"]
            .into_iter()
            .map(|n| PartyPlayer { name: n.into(), chat_id: String::new() })
            .collect();
        let (_sc, mafia) = deal_mafia_roles(&players);
        assert_eq!(mafia.len(), 6);
        assert!(mafia.iter().any(|r| r.role.contains("Дон")));
        assert!(mafia.iter().any(|r| r.role.contains("Комиссар")));

        let (sc_b, bunker) = deal_bunker_roles(&players);
        assert!(!sc_b.is_empty());
        assert_eq!(bunker.len(), 6);
    }

    #[test]
    fn stop_all_noop_when_idle_keeps_ambient() {
        let dir = std::env::temp_dir().join(format!("jarvis-gm-{}", crate::settings::rand_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let store = crate::settings::Store::open(dir.join("settings.json")).unwrap();
        let host = Host::new(dir.clone(), crate::plugins::Hooks::default());
        let ambient = Ambient::new(host, store);
        let modes = ModeManager::new();
        let gm = GameManager::new(modes.clone(), ambient);
        modes.set_mode(crate::modes::SpecialMode::Translator("английский".into()), None);
        gm.stop_all(None);
        assert_eq!(modes.current(), crate::modes::SpecialMode::Translator("английский".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn melody_skip_phrases_detected() {
        assert!(is_melody_skip("не знаю, дальше"));
        assert!(is_melody_skip("пропустить"));
        assert!(is_melody_skip("сдаюсь"));
        assert!(!is_melody_skip("король и шут лесник"));
    }
}
