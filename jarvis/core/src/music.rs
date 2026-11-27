//! Общий интерфейс музыки. Поиск, аккаунт и декодирование принадлежат модулю-провайдеру.
use crate::{alice_music::AliceMusic, fastcmd, plugins::Host};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Remote {
    pub playing: bool,
    pub active: bool,
    pub paused: bool,
    pub proposal: bool,
    pub personal_search: bool,
    pub repeat: bool,
    pub cur: Value,
    pub pos: f64,
    pub status: String,
}

pub struct Music {
    state: Mutex<Remote>,
    pub alice: Mutex<AliceMusic>,
    button: Mutex<Button>,
    cancel: tokio::sync::watch::Sender<u64>,
    playing_tx: tokio::sync::watch::Sender<bool>,
}

#[derive(Default)]
struct Button {
    youtube_pause: bool,
    at: Option<Instant>,
}
impl Default for Music {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            alice: Mutex::default(),
            button: Mutex::default(),
            cancel: tokio::sync::watch::channel(0).0,
            playing_tx: tokio::sync::watch::channel(false).0,
        }
    }
}

impl Music {
    fn set_remote(&self, next: Remote) {
        let playing = next.playing;
        *self.state.lock().unwrap() = next;
        let _ = self.playing_tx.send_if_modified(|cur| {
            if *cur == playing {
                false
            } else {
                *cur = playing;
                true
            }
        });
    }

    pub fn state(&self) -> Remote {
        self.state.lock().unwrap().clone()
    }

    pub fn playing(&self) -> bool {
        self.state.lock().unwrap().playing || self.alice.lock().unwrap().playing()
    }

    fn use_alice(&self) -> bool {
        let youtube = self.state.lock().unwrap();
        let alice = self.alice.lock().unwrap();
        !youtube.playing && alice.active() && (alice.playing() || !youtube.active)
    }

    pub async fn alice_control(&self, op: &str, args: &Value) -> Result<(), String> {
        let cmd = self.alice.lock().unwrap().command(op, args)?;
        if cmd.is_empty() { return Ok(()); }
        tokio::task::spawn_blocking(move || crate::alice::text(&cmd, true)).await.map_err(|e| e.to_string())?
    }

    pub async fn transport(&self, host: &Host, op: &str, args: Value) -> Result<(), String> {
        if self.use_alice() && matches!(op, "pause" | "resume" | "next" | "prev" | "stop" | "seek" | "related") {
            self.alice_control(op, &args).await?;
            tokio::time::sleep(Duration::from_millis(300)).await;
            return Ok(());
        }
        self.call(host, op, args).await.map(|_| ())
    }

    pub async fn stop_all(&self, host: &Host) {
        let stop_alice = self.alice.lock().unwrap().active();
        let _ = tokio::join!(self.call(host, "stop", Value::Null), async {
            if stop_alice { let _ = self.alice_control("stop", &Value::Null).await; }
        });
    }

    pub async fn cancel_loading(&self, host: &Host) {
        self.cancel.send_modify(|n| *n = n.wrapping_add(1));
        let should_stop = {
            let s = self.state.lock().unwrap();
            !s.playing && !s.paused
        };
        if should_stop {
            let _ = self.call(host, "stop", Value::Null).await;
        }
    }

    pub async fn alice_started(&self, host: &Host) {
        let from_button = {
            let mut button = self.button.lock().unwrap();
            let from_button = button.at.is_some_and(|at| at.elapsed() < Duration::from_secs(4));
            if !from_button { button.youtube_pause = false; }
            from_button
        };
        if from_button {
            let playing = self.alice.lock().unwrap().playing();
            if playing { let _ = self.alice_control("pause", &Value::Null).await; }
        } else if self.state().playing {
            let _ = self.call(host, "pause", Value::Null).await;
        }
    }

    pub async fn pause_button(&self, host: &Host) {
        let next = {
            let youtube = self.state.lock().unwrap();
            let alice = self.alice.lock().unwrap();
            let mut button = self.button.lock().unwrap();
            if youtube.playing {
                button.at = Some(Instant::now());
                button.youtube_pause = true;
                "pause"
            } else if youtube.paused && !alice.playing() && (button.youtube_pause || alice.paused_since().is_none()) {
                button.at = Some(Instant::now());
                button.youtube_pause = false;
                "resume"
            } else { return; }
        };
        let _ = self.call(host, next, Value::Null).await;
    }

    pub fn event(&self, name: &str, data: Value) {
        match name {
            "music.state" => {
                if let Ok(st) = serde_json::from_value(data) {
                    self.set_remote(st);
                }
            }
            "exit" => {
                self.cancel.send_modify(|n| *n = n.wrapping_add(1));
                self.set_remote(Remote::default());
            }
            _ => {}
        }
    }

    pub async fn call(&self, host: &Host, op: &str, args: Value) -> Result<Value, String> {
        if op == "stop" {
            self.cancel.send_modify(|n| *n = n.wrapping_add(1));
        }
        let mut cancel = self.cancel.subscribe();
        let provider = host.provider("youtube").map_err(|_| {
            "Установите и включите модуль музыки YouTube на странице «Плагины».".to_string()
        })?;
        let timeout = if matches!(
            op,
            "play"
                | "play_music"
                | "play_id"
                | "enqueue"
                | "propose"
                | "related"
                | "resume_saved"
                | "respond"
                | "next"
                | "prev"
        ) {
            24
        } else if matches!(op, "play_liked" | "play_playlist") {
            28
        } else if matches!(
            op,
            "cookies_set" | "account_begin" | "library" | "playlist_items" | "search" | "like"
        ) {
            20
        } else {
            10
        };
        let request = provider.request(
            "music",
            json!({"op": op, "args": args}),
            Duration::from_secs(timeout),
        );
        let cancellable = matches!(
            op,
            "play"
                | "play_music"
                | "play_id"
                | "play_liked"
                | "play_playlist"
                | "enqueue"
                | "propose"
                | "related"
                | "resume_saved"
                | "respond"
                | "next"
                | "prev"
        );
        let out = tokio::select! {
            biased;
            _ = cancel.changed(), if cancellable => return Err("операция отменена".into()),
            result = request => result?,
        };
        if let Some(st) = out.get("state").filter(|v| !v.is_null()) {
            let st =
                serde_json::from_value(st.clone()).map_err(|e| format!("состояние музыки: {e}"))?;
            self.set_remote(st);
        }
        if let Some(err) = out
            .get("error")
            .and_then(Value::as_str)
            .filter(|e| !e.is_empty())
        {
            return Err(err.into());
        }
        Ok(out.get("value").cloned().unwrap_or(Value::Null))
    }

    pub async fn snapshot(&self, host: &Host) -> Value {
        let youtube = match self.call(host, "snapshot", Value::Null).await {
            Ok(v) => v,
            Err(_) => {
                let s = self.state();
                json!({"source":"youtube", "playing":s.playing,"paused":s.paused,"repeat":s.repeat,"track":s.cur,"pos":s.pos})
            }
        };
        if self.use_alice() {
            let mut alice = self.alice.lock().unwrap().snapshot();
            alice["history"] = youtube["history"].clone();
            return alice;
        }
        youtube
    }

    async fn pause_alice_if_playing(&self) {
        if self.alice.lock().unwrap().playing() {
            let _ = self.alice_control("pause", &Value::Null).await;
        }
    }

    pub async fn play(&self, host: &Host, query: &str, queue: bool) -> String {
        if let Some(intent) = parse_query_intent(query) {
            return match intent {
                PlayIntent::Liked { shuffle } => self.play_liked(host, shuffle, queue).await,
                PlayIntent::Playlist { query, shuffle } => {
                    self.play_playlist(host, &query, shuffle, queue).await
                }
            };
        }
        let propose = recommendation(query);
        if !queue && !propose {
            self.pause_alice_if_playing().await;
        }
        // Для музыкальных запросов (не подкастов/интервью/видео) используем встроенный в плагин
        // YouTube Music поиск (с его пулом/VPN, фолбэк-кандидатами и радио без повторных поисков).
        if !propose && !is_spoken_video_query(query) {
            let generic = is_generic_music_query(query);
            if queue {
                match self
                    .call(host, "enqueue", json!({"query": query, "music": true}))
                    .await
                {
                    Ok(v) => {
                        let shown = track_display(&v, query);
                        return format!("Добавил в очередь: {shown}.");
                    }
                    Err(e) => return format!("Не получилось: {e}"),
                }
            } else {
                match self
                    .call(
                        host,
                        "play_music",
                        json!({"query": query, "generic": generic}),
                    )
                    .await
                {
                    Ok(v) => {
                        let shown = track_display(&v, query);
                        return format!("Включил в YouTube Music: {shown}.");
                    }
                    Err(e) if e == "Неизвестная команда плеера" => {
                        // Совместимость со старым бинарником плагина YouTube (до v1.1.5)
                        let songs = search_ytmusic_songs(query).await;
                        if !songs.is_empty() {
                            let pick_idx = if generic {
                                let pool = songs.len().min(8);
                                let seed = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.subsec_nanos() as usize + d.as_secs() as usize)
                                    .unwrap_or(0);
                                seed % pool
                            } else {
                                0
                            };
                            let picked = &songs[pick_idx];
                            let display = if picked.author.is_empty() {
                                picked.title.clone()
                            } else {
                                format!("{} — {}", picked.author, picked.title)
                            };
                            if let Ok(v) = self
                                .call(
                                    host,
                                    "play_id",
                                    json!({
                                        "id": picked.id,
                                        "title": picked.title,
                                        "author": picked.author
                                    }),
                                )
                                .await
                            {
                                let shown = track_display(&v, &display);
                                return format!("Включил в YouTube Music: {shown}.");
                            }
                        }
                    }
                    Err(e) => return format!("Не получилось: {e}"),
                }
            }
        }
        let result = self
            .call(
                host,
                if queue {
                    "enqueue"
                } else if propose {
                    "propose"
                } else {
                    "play"
                },
                json!({"query":query}),
            )
            .await;
        match result {
            Ok(v) if queue => format!("В очереди: {}.", title(&v)),
            Ok(Value::String(s)) if propose => format!(
                "{s} Ролик ещё не включён. Дождись ответа человека и передай его в music_control."
            ),
            Ok(v) => format!("Включил с YouTube: {}.", title(&v)),
            Err(e) => format!("Не получилось: {e}"),
        }
    }

    pub async fn play_liked(&self, host: &Host, shuffle: bool, queue: bool) -> String {
        if !queue {
            self.pause_alice_if_playing().await;
        }
        let args = json!({"shuffle": shuffle, "queue": queue});
        match self.call(host, "play_liked", args).await {
            Ok(v) => {
                let track = v.get("track").unwrap_or(&v);
                if queue {
                    format!("Добавил понравившиеся в очередь: {}.", title(track))
                } else {
                    format!("Включил понравившиеся с YouTube: {}.", title(track))
                }
            }
            Err(e) => format!("Не получилось: {e}"),
        }
    }

    pub async fn play_playlist(
        &self,
        host: &Host,
        query: &str,
        shuffle: bool,
        queue: bool,
    ) -> String {
        if !queue {
            self.pause_alice_if_playing().await;
        }
        let args = json!({"query": query, "shuffle": shuffle, "queue": queue});
        match self.call(host, "play_playlist", args).await {
            Ok(v) => {
                let track = v.get("track").unwrap_or(&v);
                let pl = v
                    .get("playlist")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if queue {
                    if pl.is_empty() {
                        format!("Добавил плейлист в очередь: {}.", title(track))
                    } else {
                        format!("Добавил плейлист «{pl}» в очередь: {}.", title(track))
                    }
                } else if pl.is_empty() {
                    format!("Включил плейлист с YouTube: {}.", title(track))
                } else {
                    format!("Включил плейлист «{pl}»: {}.", title(track))
                }
            }
            Err(e) => format!("Не получилось: {e}"),
        }
    }

    pub async fn control(&self, host: &Host, text: &str) -> Option<String> {
        let command = command(text);
        if self.use_alice() && let Some((op, args, answer)) = &command {
            if *op == "status" { return Some(self.alice.lock().unwrap().description()); }
            return Some(match self.alice_control(op, args).await {
                Ok(()) if *op == "related" => "Переключаю трек.".into(),
                Ok(()) => (*answer).into(),
                Err(e) => format!("Не получилось: {e}"),
            });
        }
        if self.state().proposal {
            match self.call(host, "respond", json!({"text":text})).await {
                Ok(v) if v["ok"].as_bool() == Some(true) => {
                    return Some(v["answer"].as_str().unwrap_or("").into());
                }
                Err(e) => return Some(format!("Не получилось: {e}")),
                _ => {}
            }
        }
        if !self.state().active {
            return None;
        }
        let (op, args, answer) = command?;
        if op == "status" {
            return Some(self.state().status);
        }
        let cur_title = title(&self.state().cur).to_string();
        Some(match self.call(host, op, args.clone()).await {
            Err(e) => format!("Не получилось: {e}"),
            Ok(Value::String(s)) if op == "related" => s,
            Ok(v) if matches!(op, "next" | "prev") => {
                let shown = track_display(&v, "");
                if shown.is_empty() {
                    answer.into()
                } else {
                    format!("Включил «{shown}».")
                }
            }
            Ok(_) if op == "like" && args["liked"].as_bool() == Some(true) && !cur_title.is_empty() => {
                format!("Добавил «{cur_title}» в избранное.")
            }
            Ok(_) if op == "like" && args["liked"].as_bool() == Some(false) && !cur_title.is_empty() => {
                format!("Убрал «{cur_title}» из избранного.")
            }
            Ok(_) => answer.into(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayIntent {
    Liked { shuffle: bool },
    Playlist { query: String, shuffle: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayDirect {
    Liked { shuffle: bool },
    Playlist { query: String, shuffle: bool },
    Yandex { query: String },
    Queue { query: String },
    Track { query: String },
}

const PLAY_VERBS: &[&str] = &[
    "включи ",
    "поставь ",
    "запусти ",
    "давай ",
    "найди ",
    "сыграй ",
    "воспроизведи ",
];

const SHUFFLE_PHRASES: &[&str] = &[
    "в случайном порядке",
    "случайном порядке",
    "вперемешку",
    "перемешай",
    "перемешать",
    "шафл",
    "рандомно",
    "рандом",
];

const YOUTUBE_PHRASES: &[&str] = &[
    "youtube music",
    "ютуб музыку",
    "ютуб музыка",
    "ютуб музыке",
    "на youtube",
    "в youtube",
    "с youtube",
    "из youtube",
    "по youtube",
    "youtube",
    "на ютубе",
    "в ютубе",
    "с ютуба",
    "из ютуба",
    "по ютубу",
    "на ютуб",
    "в ютуб",
    "с ютуб",
    "ютубе",
    "ютуба",
    "ютубу",
    "ютуб",
    "на ютьюбе",
    "с ютьюба",
    "ютьюбе",
    "ютьюба",
    "ютьюб",
];

const LIKED_TARGETS: &[&str] = &[
    "мои лайки",
    "лайки",
    "лайкнутые",
    "лайкнутые треки",
    "лайкнутые песни",
    "лайкнутые видео",
    "понравившиеся",
    "понравившиеся треки",
    "понравившиеся песни",
    "понравившиеся видео",
    "понравившуюся музыку",
    "понравившиеся ролики",
    "мои понравившиеся",
    "мои понравившиеся треки",
    "мои понравившиеся песни",
    "любимые треки",
    "любимые песни",
    "любимую музыку",
    "треки из лайков",
    "песни из лайков",
    "музыку из лайков",
];

const PLAYLIST_PREFIXES: &[&str] = &[
    "мой плейлист ",
    "свой плейлист ",
    "сохраненный плейлист ",
    "личный плейлист ",
    "плейлист ",
];

fn remove_whole_phrase(padded: &str, phrase: &str) -> (String, bool) {
    let pat = format!(" {phrase} ");
    if !padded.contains(&pat) {
        return (padded.to_string(), false);
    }
    let replaced = padded.replace(&pat, " ");
    let collapsed = replaced.split_whitespace().collect::<Vec<_>>().join(" ");
    (format!(" {collapsed} "), true)
}

fn clean_play_tokens(normalized: &str) -> (String, bool) {
    let mut work = format!(
        " {} ",
        normalized.split_whitespace().collect::<Vec<_>>().join(" ")
    );
    let mut shuffle = false;
    for phrase in SHUFFLE_PHRASES {
        let (next, hit) = remove_whole_phrase(&work, phrase);
        work = next;
        shuffle |= hit;
    }
    for phrase in YOUTUBE_PHRASES {
        let (next, _) = remove_whole_phrase(&work, phrase);
        work = next;
    }
    let trimmed = work.trim();
    let trimmed = trimmed
        .strip_prefix("пожалуйста ")
        .unwrap_or(trimmed)
        .strip_suffix(" пожалуйста")
        .unwrap_or(trimmed)
        .trim();
    (trimmed.to_string(), shuffle)
}

fn classify_target(target: &str, shuffle: bool) -> Option<PlayIntent> {
    let target = target.trim();
    if LIKED_TARGETS.contains(&target) {
        return Some(PlayIntent::Liked { shuffle });
    }
    for prefix in PLAYLIST_PREFIXES {
        if let Some(rest) = target.strip_prefix(prefix) {
            let name = rest.trim();
            if name.is_empty() {
                return None;
            }
            if LIKED_TARGETS.contains(&name) || matches!(name, "из лайков" | "из моих лайков") {
                return Some(PlayIntent::Liked { shuffle });
            }
            return Some(PlayIntent::Playlist {
                query: name.to_string(),
                shuffle,
            });
        }
    }
    None
}

pub fn parse_play_intent(text: &str) -> Option<PlayIntent> {
    if wants_keep_talking(text) {
        return None;
    }
    let lower = text.to_lowercase().replace('ё', "е");
    if lower.contains("яндекс") || lower.contains("алис") {
        return None;
    }
    let stripped = fastcmd::strip_lead(&fastcmd::norm_text(text));
    let (cleaned, shuffle) = clean_play_tokens(&stripped);
    let verb_rest = PLAY_VERBS.iter().find_map(|v| cleaned.strip_prefix(v))?;
    classify_target(verb_rest, shuffle)
}

pub fn parse_query_intent(query: &str) -> Option<PlayIntent> {
    let lower = query.to_lowercase().replace('ё', "е");
    if lower.contains("яндекс") || lower.contains("алис") {
        return None;
    }
    let norm = fastcmd::norm_text(query);
    let (cleaned, shuffle) = clean_play_tokens(&norm);
    let target = PLAY_VERBS
        .iter()
        .find_map(|v| cleaned.strip_prefix(v))
        .unwrap_or(&cleaned);
    classify_target(target, shuffle)
}

const DIRECT_MUSIC_BLOCKLIST: &[&str] = &[
    "свет", "ламп", "люстр", "лент", "торшер", "пылесос", "пылик", "кондиц", "кондей",
    "розетк", "увлажнит", "чайник", "телевизор", "сценари", "штор", "обогрев", "вентилятор",
    "камин", "дожд", "океан", "прибой", "белый шум", "шум для сна", "таймер", "будильник",
    "напоминан", "громкост", "ночной режим", "диджей", "вайб", "книг", "сказк", "урок",
    "курс", "игр", "квест", "викторин", "переводчик", "профил", "ассистент", "повтор",
    "репит", "пауз", "подкаст", "спор", "радио", "блютуз", "bluetooth",
];

const CONTEXT_DEPENDENT_WORDS: &[&str] = &[
    "котор", "ту саму", "эту саму", "помнишь", "вчера", "недавно", "тоже", "как в прошлый",
];

const QUEUE_PREFIXES: &[&str] = &[
    "поставь следующим треком ",
    "поставь следующей песней ",
    "поставь следующим ",
    "поставь следующей ",
    "включи следующим треком ",
    "включи следующей песней ",
    "включи следующим ",
    "включи следующей ",
    "добавь в очередь трек ",
    "добавь в очередь песню ",
    "добавь в очередь ",
    "поставь в очередь трек ",
    "поставь в очередь песню ",
    "поставь в очередь ",
    "в очередь ",
];

const DIRECT_TRACK_VERBS: &[&str] = &[
    "включи ",
    "поставь ",
    "сыграй ",
    "запусти ",
    "воспроизведи ",
];

/// Разбирает однозначные музыкальные команды, которые можно выполнить напрямую без задержки LLM:
/// лайки, плейлисты, «Мою волну» / Яндекс Музыку, добавление трека в очередь и конкретные треки.
pub fn parse_direct_music_request(text: &str) -> Option<PlayDirect> {
    if wants_keep_talking(text)
        || fastcmd::clarification(text).is_some()
        || recommendation(text)
        || fastcmd::profile_switch_target(text).is_some()
        || fastcmd::alice_schedule(text) != fastcmd::AliceSchedule::None
    {
        return None;
    }
    if let Some(intent) = parse_play_intent(text) {
        return Some(match intent {
            PlayIntent::Liked { shuffle } => PlayDirect::Liked { shuffle },
            PlayIntent::Playlist { query, shuffle } => PlayDirect::Playlist { query, shuffle },
        });
    }
    let norm = fastcmd::norm_text(text);
    let stripped = fastcmd::strip_lead(&norm);
    let stripped = stripped
        .strip_prefix("пожалуйста ")
        .unwrap_or(&stripped)
        .strip_suffix(" пожалуйста")
        .unwrap_or(&stripped)
        .trim();
    if stripped.is_empty() || CONTEXT_DEPENDENT_WORDS.iter().any(|w| stripped.contains(w)) {
        return None;
    }
    if let Some(rest) = QUEUE_PREFIXES.iter().find_map(|p| stripped.strip_prefix(p)) {
        let (cleaned, _) = clean_play_tokens(rest);
        let q = cleaned
            .strip_prefix("трек ")
            .or_else(|| cleaned.strip_prefix("песню "))
            .or_else(|| cleaned.strip_prefix("песня "))
            .or_else(|| cleaned.strip_prefix("композицию "))
            .unwrap_or(&cleaned)
            .trim();
        if !q.is_empty()
            && !matches!(q, "трек" | "песню" | "песня" | "музыку" | "композицию")
            && q.split_whitespace().count() <= 10
        {
            return Some(PlayDirect::Queue { query: q.to_string() });
        }
        return None;
    }
    if DIRECT_TRACK_VERBS.iter().any(|v| stripped.starts_with(v))
        && (stripped.contains("моя волн")
            || stripped.contains("мою волн")
            || stripped.contains("яндекс музык")
            || stripped.contains("на яндекс")
            || stripped.contains("в яндекс"))
        && !stripped.contains("алис")
    {
        return Some(PlayDirect::Yandex {
            query: stripped.to_string(),
        });
    }
    if fastcmd::yandex_music_request(stripped)
        || DIRECT_MUSIC_BLOCKLIST.iter().any(|w| stripped.contains(w))
    {
        return None;
    }
    let after_verb = DIRECT_TRACK_VERBS
        .iter()
        .find_map(|v| stripped.strip_prefix(v))?;
    let explicit_yt = fastcmd::youtube_request(text);
    let (cleaned_after, _) = clean_play_tokens(after_verb);
    let target = if let Some(rest) = [
        "песню ",
        "песня ",
        "трек ",
        "композицию ",
        "музыку ",
    ]
    .iter()
    .find_map(|n| cleaned_after.strip_prefix(n))
    {
        rest.trim()
    } else if explicit_yt {
        cleaned_after
            .strip_prefix("ролик ")
            .or_else(|| cleaned_after.strip_prefix("видео "))
            .or_else(|| cleaned_after.strip_prefix("клип "))
            .unwrap_or(&cleaned_after)
            .trim()
    } else {
        return None;
    };
    if target.is_empty()
        || target.split_whitespace().count() > 10
        || is_generic_music_query(target)
        || is_spoken_video_query(target)
        || target.starts_with("для ")
        || target.starts_with("про ")
        || target.starts_with("под ")
        || target.starts_with("на ")
        || target.starts_with("как")
        || target.starts_with("что")
        || target.starts_with("где")
        || target.starts_with("похож")
        || target.starts_with("следующ")
        || target.starts_with("предыдущ")
        || target.starts_with("друг")
        || matches!(
            target,
            "трек" | "песню" | "песня" | "музыку" | "композицию" | "плейлист" | "лайки"
        )
    {
        return None;
    }
    Some(PlayDirect::Track {
        query: target.to_string(),
    })
}

fn track_display(v: &Value, fallback: &str) -> String {
    let t = v
        .get("Title")
        .or_else(|| v.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let a = v
        .get("Author")
        .or_else(|| v.get("author"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if t.is_empty() {
        return fallback.trim().to_string();
    }
    if !a.is_empty() && !t.to_lowercase().contains(&a.to_lowercase()) {
        format!("{a} — {t}")
    } else {
        t.to_string()
    }
}

fn title(v: &Value) -> &str {
    v.get("Title")
        .or_else(|| v.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("трек")
}

fn recommendation(text: &str) -> bool {
    let t = fastcmd::strip_lead(&fastcmd::norm_text(text));
    if [
        "рекомендац",
        "рекомендован",
        "что нибудь ",
        "подбери ",
        "выбери ",
        "предложи ",
    ]
    .iter()
    .any(|p| t.starts_with(p))
    {
        return true;
    }
    if ["мои рекомендации", "что то интересное"].contains(&t.as_str())
    {
        return true;
    }
    ["включи ", "поставь ", "запусти ", "давай ", "найди "]
        .iter()
        .any(|p| t.starts_with(p))
        && [
            "мои рекомендации",
            "моих рекомендац",
            "из рекомендац",
            "что нибудь",
            "что то интересное",
            "послушать",
            "чтобы болтало на фоне",
            "сам ",
            "самому",
            "на свой вкус",
        ]
        .iter()
        .any(|p| t.contains(p))
}

fn is_next_track_cmd(t: &str) -> bool {
    if matches!(
        t,
        "дальше"
            | "скип"
            | "скипни"
            | "пропусти"
            | "пропусти трек"
            | "пропусти песню"
            | "переключи"
            | "переключи трек"
            | "переключи песню"
            | "переключи музыку"
            | "другой трек"
            | "другую песню"
            | "другую музыку"
            | "включи другую песню"
            | "включи другой трек"
            | "включи другую музыку"
            | "поставь другую песню"
            | "поставь другой трек"
            | "поставь другую музыку"
    ) {
        return true;
    }
    if t.starts_with("включи похож") || t.starts_with("поставь похож") {
        return true;
    }
    let stripped = t
        .strip_prefix("включи ")
        .or_else(|| t.strip_prefix("поставь "))
        .or_else(|| t.strip_prefix("переключи на "))
        .or_else(|| t.strip_prefix("переключи "))
        .unwrap_or(t)
        .trim();
    if let Some(rest) = [
        "следующий",
        "следующая",
        "следующее",
        "следующую",
        "следующие",
        "следующим",
    ]
    .iter()
    .find_map(|p| stripped.strip_prefix(p))
    {
        let rest = rest.trim();
        return rest.is_empty()
            || matches!(
                rest,
                "трек"
                    | "песня"
                    | "песню"
                    | "музыка"
                    | "музыку"
                    | "композиция"
                    | "композицию"
                    | "ролик"
                    | "видео"
            );
    }
    false
}

fn is_prev_track_cmd(t: &str) -> bool {
    if matches!(
        t,
        "верни трек" | "верни песню" | "назад трек" | "прошлый трек" | "прошлую песню"
    ) {
        return true;
    }
    let stripped = t
        .strip_prefix("включи ")
        .or_else(|| t.strip_prefix("поставь "))
        .or_else(|| t.strip_prefix("переключи на "))
        .or_else(|| t.strip_prefix("переключи "))
        .or_else(|| t.strip_prefix("верни "))
        .unwrap_or(t)
        .trim();
    if let Some(rest) = [
        "предыдущий",
        "предыдущая",
        "предыдущее",
        "предыдущую",
        "прошлый",
        "прошлую",
    ]
    .iter()
    .find_map(|p| stripped.strip_prefix(p))
    {
        let rest = rest.trim();
        return rest.is_empty()
            || matches!(
                rest,
                "трек"
                    | "песня"
                    | "песню"
                    | "музыка"
                    | "музыку"
                    | "композиция"
                    | "композицию"
                    | "ролик"
                    | "видео"
            );
    }
    false
}

pub fn command(text: &str) -> Option<(&'static str, Value, &'static str)> {
    let lowered = text
        .to_lowercase()
        .replace('ё', "е")
        .replace([',', '.', '!', '?'], " ");
    let collapsed = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = fastcmd::strip_lead(&collapsed)
        .trim()
        .trim_start_matches("пожалуйста ")
        .trim_end_matches(" пожалуйста")
        .trim()
        .to_string();
    let starts = |s: &[&str]| s.iter().any(|p| t.starts_with(p));
    let simple = |op, answer| Some((op, Value::Null, answer));
    if starts(&[
        "что играет",
        "что сейчас играет",
        "что за пес",
        "что за трек",
        "кто поет",
        "как называется пес",
    ]) {
        return simple("status", "");
    }
    if starts(&["сначала", "с начала", "в начало", "включи сначала"])
    {
        return Some(("seek", json!({"delta":0,"abs":true}), "С начала."));
    }
    if starts(&[
        "перемот",
        "промот",
        "мотни",
        "вперед на",
        "назад на",
        "отмотай",
    ]) {
        let (delta, abs) = seek(&t);
        return Some(("seek", json!({"delta":delta,"abs":abs}), "Перемотал."));
    }
    if starts(&["продолж", "сними с пауз", "сними пауз", "играй дальше"])
    {
        return simple("resume", "Продолжаю.");
    }
    if starts(&[
        "пауза",
        "на паузу",
        "поставь на паузу",
        "останови музыку",
        "приостанов",
    ]) {
        return simple("pause", "Пауза.");
    }
    if starts(&["выключи музыку", "выключи песню", "выруби музыку", "стоп"])
    {
        return simple("stop", "Выключил.");
    }
    if starts(&[
        "на повтор",
        "на репит",
        "включи повтор",
        "поставь на повтор",
        "поставь на репит",
        "включи репит",
        "повтор трека",
        "повторяй",
    ]) {
        return Some(("repeat", json!({"on": true}), "Повтор включён."));
    }
    if starts(&[
        "выключи повтор",
        "убери повтор",
        "выключи репит",
        "убери репит",
        "без повтора",
        "отключи повтор",
        "не повторяй",
    ]) {
        return Some(("repeat", json!({"on": false}), "Повтор выключен."));
    }
    if is_next_track_cmd(&t) {
        return simple("next", "Переключаю.");
    }
    if is_prev_track_cmd(&t) {
        return simple("prev", "Предыдущий трек.");
    }
    if starts(&[
        "убери лайк",
        "сними лайк",
        "удали лайк",
        "убери из лайков",
        "убери из понравившихся",
        "убери из избранного",
        "убери из любимых",
        "удали из лайков",
        "удали из понравившихся",
        "удали из избранного",
        "удали из любимых",
        "не нравится этот трек",
        "не нравится эта песня",
    ]) {
        return Some(("like", json!({"liked": false}), "Убрал лайк."));
    }
    if starts(&[
        "поставь лайк",
        "лайкни",
        "нравится этот трек",
        "нравится эта песня",
        "мне нравится",
        "мне понравил",
        "понравил",
        "классный трек",
        "классная песня",
        "крутой трек",
        "крутая песня",
        "отличная песня",
        "отличный трек",
        "добавь в лайки",
        "добавь в понравившиеся",
        "добавь в избранное",
        "добавь в любимые",
        "в избранное",
        "в любимые",
        "сохрани трек",
        "сохрани песню",
        "запомни трек",
        "запомни песню",
    ]) || t == "лайк"
        || (t.contains("понравил") && (t.contains("песн") || t.contains("трек") || t.contains("музык")))
        || (t.contains("избранн") && (t.contains("добав") || t.contains("сохран") || t.contains("песн") || t.contains("трек")))
    {
        return Some(("like", json!({"liked": true}), "Поставил лайк."));
    }
    None
}

fn seek(t: &str) -> (f64, bool) {
    for word in t.split_whitespace() {
        if let Some((m, s)) = word.split_once(':') {
            if let (Ok(m), Ok(s)) = (m.parse::<u32>(), s.parse::<u32>()) {
                if s < 60 {
                    return ((m as f64) * 60.0 + s as f64, true);
                }
            }
        }
    }
    let words: Vec<_> = t.split_whitespace().collect();
    let mut total = 0.0;
    for (i, w) in words.iter().enumerate() {
        let n = match *w {
            "полминуты" => {
                total += 30.0;
                continue;
            }
            "минуту" if i == 0 || amount(words[i - 1]).is_none() => {
                total += 60.0;
                continue;
            }
            _ => amount(w),
        };
        if let Some(n) = n {
            let unit = words.get(i + 1).copied().unwrap_or("");
            total += n * if unit.starts_with("мин")
                || *w == "полторы"
                || (*w == "пару" && unit.is_empty())
            {
                60.0
            } else {
                1.0
            };
        }
    }
    if total == 0.0 {
        total = 30.0;
    }
    if t.contains("назад") || t.contains("отмотай") {
        total = -total;
    }
    (total, t.contains("на отметку") || t.contains("на позицию"))
}
fn amount(w: &str) -> Option<f64> {
    Some(match w {
        "одну" | "один" => 1.0,
        "два" | "две" | "пару" => 2.0,
        "три" => 3.0,
        "четыре" => 4.0,
        "пять" => 5.0,
        "десять" => 10.0,
        "пятнадцать" => 15.0,
        "двадцать" => 20.0,
        "тридцать" => 30.0,
        "сорок" => 40.0,
        "пятьдесят" => 50.0,
        "полторы" => 1.5,
        _ => w.parse().ok()?,
    })
}

pub fn volume_loop(music: Arc<Music>, host: Arc<Host>, duck: impl Fn() -> bool + Send + 'static) {
    tokio::spawn(async move {
        let mut rx = music.playing_tx.subscribe();
        let mut last_yt = -1.0_f64;
        let mut alice_saved_vol: Option<i64> = None;
        let mut alice_duck_checked = false;
        loop {
            let is_duck = duck();
            let yt_playing = *rx.borrow_and_update();
            let alice_playing = !yt_playing && music.alice.lock().unwrap().playing();

            if yt_playing {
                let v = if is_duck { 0.25 } else { 0.95 };
                if v != last_yt
                    && let Ok(n) = host.provider("youtube")
                    && n.notify("volume", json!({"v": v})).await.is_ok()
                {
                    last_yt = v;
                }
            } else {
                last_yt = -1.0;
            }

            if alice_playing && is_duck {
                if !alice_duck_checked && alice_saved_vol.is_none() {
                    alice_duck_checked = true;
                    if let Ok(Some(cur)) = tokio::task::spawn_blocking(crate::speakervol::speaker_volume).await
                        && cur > 2
                    {
                        alice_saved_vol = Some(cur);
                        let _ = tokio::task::spawn_blocking(|| crate::speakervol::set_alice_volume(2)).await;
                    }
                }
            } else {
                alice_duck_checked = false;
                if !is_duck
                    && let Some(saved) = alice_saved_vol.take()
                    && let Ok(Some(cur)) = tokio::task::spawn_blocking(crate::speakervol::speaker_volume).await
                    && cur <= 2
                {
                    let _ = tokio::task::spawn_blocking(move || crate::speakervol::set_alice_volume(saved)).await;
                }
            }

            if yt_playing || alice_playing || is_duck || alice_saved_vol.is_some() {
                tokio::time::sleep(Duration::from_millis(30)).await;
            } else {
                tokio::select! {
                    res = rx.changed() => {
                        if res.is_err() {
                            return;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(80)) => {}
                }
            }
        }
    });
}

pub fn played_music_title(tool_name: &str, result: &str) -> Option<String> {
    match tool_name {
        "play_music" | "play_yandex_music" => {
            if let Some(rest) = result
                .strip_prefix("Включил с YouTube:")
                .or_else(|| result.strip_prefix("Включил в YouTube Music:"))
                .or_else(|| result.strip_prefix("Включил в Яндекс Музыке:"))
                .or_else(|| result.strip_prefix("В YouTube Music не нашлось, поэтому включил в Яндекс Музыке:"))
                .or_else(|| result.strip_prefix("Включил понравившиеся с YouTube:"))
                .or_else(|| result.strip_prefix("Включил плейлист с YouTube:"))
            {
                return Some(rest.trim().trim_end_matches('.').to_string());
            }
            result
                .strip_prefix("Включил плейлист «")
                .and_then(|s| s.split_once("»:"))
                .map(|(_, title)| title.trim().trim_end_matches('.').to_string())
        }
        "music_control" => result
            .strip_prefix("Включил «")
            .and_then(|s| s.strip_suffix("».").or_else(|| s.strip_suffix('»')))
            .map(|s| s.trim().to_string()),
        _ => None,
    }
}

pub fn wants_keep_talking(text: &str) -> bool {
    let lower = text.to_lowercase().replace('ё', "е");
    lower.contains('?')
        || [
            "не отключай",
            "не выключай",
            "не закрывай",
            "останься",
            "поболта",
            "поговори",
            "расскажи",
            "объясни",
            "подскажи",
            "какая погода",
            "сколько врем",
            "что такое",
            "кто такой",
            "кто такая",
            "почему ",
            "зачем ",
            "как сдела",
        ]
        .iter()
        .any(|p| lower.contains(p))
}

pub fn should_auto_close_after_music(
    played_title: Option<&str>,
    other_tools: bool,
    keep_talking: bool,
    answer: &str,
) -> bool {
    let Some(title) = played_title else {
        return false;
    };
    !other_tools
        && !keep_talking
        && !answer.contains('?')
        && answer.chars().count() <= title.chars().count() + 100
}

#[derive(Default)]
pub struct MusicTurnTracker {
    played_title: Option<String>,
    other_tools: bool,
    keep_talking: bool,
}

impl MusicTurnTracker {
    pub fn note_other(&mut self) {
        self.other_tools = true;
    }

    pub fn observe_tool(&mut self, name: &str, args: &str, result: &str) {
        if name == "play_music" || name == "play_yandex_music" || name == "music_control" {
            if let Ok(v) = serde_json::from_str::<Value>(args)
                && v.get("keep_talking").and_then(Value::as_bool) == Some(true)
            {
                self.keep_talking = true;
            }
            if let Some(title) = played_music_title(name, result) {
                if !self.other_tools {
                    self.played_title = Some(title);
                }
            } else {
                self.other_tools = true;
            }
        } else {
            self.other_tools = true;
        }
    }

    pub fn should_close(&self, answer: &str) -> bool {
        should_auto_close_after_music(
            self.played_title.as_deref(),
            self.other_tools,
            self.keep_talking,
            answer,
        )
    }

    pub fn can_instant_close(&self) -> Option<&str> {
        if !self.other_tools && !self.keep_talking {
            self.played_title.as_deref()
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
struct YtMusicTrack {
    id: String,
    title: String,
    author: String,
    duration_s: u32,
}

fn is_spoken_video_query(query: &str) -> bool {
    let q = query.to_lowercase().replace('ё', "е");
    [
        "подкаст",
        "интервью",
        "лекци",
        "аудиокниг",
        "стендап",
        "standup",
        "новости",
        "обзор",
        "ролик",
        "видео",
        "стрим",
        "трейлер",
        "документал",
        "сказк",
        "выпуск",
        "шоу ",
        "podcast",
        "interview",
        "lecture",
    ]
    .iter()
    .any(|w| q.contains(w))
}

fn is_generic_music_query(query: &str) -> bool {
    let q = query.trim().to_lowercase().replace('ё', "е");
    if q.is_empty() {
        return true;
    }
    [
        "популярн",
        "новинк",
        "хит",
        "чарт",
        "топ ",
        "динамичн",
        "бодр",
        "весел",
        "грустн",
        "спокойн",
        "расслаб",
        "танцевальн",
        "клубн",
        "фонов",
        "классн",
        "хорош",
        "крут",
        "современн",
        "трендов",
        "что-нибудь",
        "какую-нибудь",
        "любую музыку",
        "просто музыку",
        "музыка",
        "музыку",
    ]
    .iter()
    .any(|w| q.contains(w))
}

fn is_spam_music_track(title: &str, author: &str, duration_s: u32) -> bool {
    if duration_s > 540 || (duration_s > 0 && duration_s < 45) {
        return true;
    }
    let meta = format!("{} {}", title.to_lowercase(), author.to_lowercase()).replace('ё', "е");
    [
        "подборка",
        "в машину",
        "сборник",
        "мегамикс",
        "megamix",
        "nonstop",
        "non-stop",
        "1 час",
        "2 часа",
        "3 часа",
        "randomchik",
        "музыка для души",
        "best hits",
        "top 100",
        "топ 100",
        "миксы",
        "дискотека 202",
        "клубная музыка 202",
        "новинки 202",
        "хиты 202",
    ]
    .iter()
    .any(|w| meta.contains(w))
}

fn parse_duration_secs(s: &str) -> u32 {
    let parts: Vec<u32> = s
        .trim()
        .split(':')
        .filter_map(|p| p.trim().parse::<u32>().ok())
        .collect();
    match parts.as_slice() {
        [m, sec] => m * 60 + sec,
        [h, m, sec] => h * 3600 + m * 60 + sec,
        _ => 0,
    }
}

fn collect_runs_text(v: &Value) -> String {
    v.get("runs")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r.get("text").and_then(Value::as_str))
                .collect::<String>()
        })
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn collect_ytmusic_tracks(v: &Value, out: &mut Vec<YtMusicTrack>) {
    match v {
        Value::Object(map) => {
            if let Some(it) = map.get("musicResponsiveListItemRenderer") {
                let vid = it["playlistItemData"]["videoId"]
                    .as_str()
                    .or_else(|| it["overlay"]["musicItemThumbnailOverlayRenderer"]["content"]["musicPlayButtonRenderer"]["playNavigationEndpoint"]["watchEndpoint"]["videoId"].as_str())
                    .unwrap_or("")
                    .trim();
                if !vid.is_empty() && !out.iter().any(|t| t.id == vid) {
                    let cols = it["flexColumns"].as_array();
                    let title = cols
                        .and_then(|c| c.first())
                        .map(|c| collect_runs_text(&c["musicResponsiveListItemFlexColumnRenderer"]["text"]))
                        .unwrap_or_default();
                    let sub_runs = cols
                        .and_then(|c| c.get(1))
                        .and_then(|c| c["musicResponsiveListItemFlexColumnRenderer"]["text"]["runs"].as_array());
                    let mut author = String::new();
                    let mut duration_s = 0u32;
                    if let Some(runs) = sub_runs {
                        let texts: Vec<&str> = runs
                            .iter()
                            .filter_map(|r| r.get("text").and_then(Value::as_str))
                            .map(str::trim)
                            .filter(|s| !s.is_empty() && *s != "•" && *s != "Песня" && *s != "Song" && *s != "Видео" && *s != "Video")
                            .collect();
                        if let Some(first) = texts.first() {
                            author = (*first).to_string();
                        }
                        if let Some(last) = texts.last() {
                            duration_s = parse_duration_secs(last);
                        }
                    }
                    if !title.is_empty() && !is_spam_music_track(&title, &author, duration_s) {
                        out.push(YtMusicTrack {
                            id: vid.to_string(),
                            title,
                            author,
                            duration_s,
                        });
                    }
                }
            }
            for val in map.values() {
                if out.len() >= 15 {
                    break;
                }
                collect_ytmusic_tracks(val, out);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                if out.len() >= 15 {
                    break;
                }
                collect_ytmusic_tracks(val, out);
            }
        }
        _ => {}
    }
}

async fn search_ytmusic_songs(query: &str) -> Vec<YtMusicTrack> {
    let clean = query.trim();
    if clean.is_empty() {
        return Vec::new();
    }
    let body = json!({
        "context": {
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": "1.20240501.01.00",
                "hl": "ru",
                "gl": "RU"
            }
        },
        "query": clean,
        "params": "EgWKAQIIAWoKEAMQBBAJEAoQBQ%3D%3D"
    });
    let Ok(resp) = crate::net::direct()
        .post("https://music.youtube.com/youtubei/v1/search?prettyPrint=false")
        .header("Origin", "https://music.youtube.com")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .json(&body)
        .timeout(Duration::from_secs(5))
        .send()
        .await
    else {
        return Vec::new();
    };
    if !resp.status().is_success() {
        return Vec::new();
    }
    let Ok(data) = resp.json::<Value>().await else {
        return Vec::new();
    };
    let mut tracks = Vec::new();
    collect_ytmusic_tracks(&data, &mut tracks);
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_controls_without_catching_questions() {
        assert!(command("расскажи как перемотать фильм").is_none());
        assert_eq!(
            command("Джарвис, перемотай назад на 2 минуты 15 секунд")
                .unwrap()
                .1,
            json!({"delta":-135.0,"abs":false})
        );
        assert_eq!(
            command("перемотай на 2:05").unwrap().1,
            json!({"delta":125.0,"abs":true})
        );
        assert_eq!(command("перемотай на полминуты").unwrap().1["delta"], 30.0);
        assert_eq!(command("пауза").unwrap().0, "pause");
    }
    #[test]
    fn resets_state_on_exit() {
        let m = Music::default();
        m.event("music.state", json!({"active":true,"playing":true}));
        assert!(m.state().playing);
        m.event("exit", Value::Null);
        assert!(!m.state().active);
    }

    #[test]
    fn foreground_player_owns_controls() {
        let m = Music::default();
        m.alice.lock().unwrap().main_log(r#"event=track_start, payload={"id":"42"}"#);
        assert!(m.use_alice());
        m.event("music.state", json!({"playing":true,"active":true}));
        assert!(!m.use_alice());
        m.event("music.state", json!({"paused":true,"active":true}));
        assert!(m.use_alice());
        m.alice.lock().unwrap().media_log("[audio stream=42] pause (position 2000)");
        assert!(!m.use_alice());
    }

    #[test]
    fn repeat_voice_commands_toggle_on_and_off() {
        assert_eq!(command("поставь на повтор").unwrap().1, json!({"on": true}));
        assert_eq!(command("на репит").unwrap().1, json!({"on": true}));
        assert_eq!(command("включи повтор").unwrap().0, "repeat");
        assert_eq!(command("выключи повтор").unwrap().1, json!({"on": false}));
        assert_eq!(command("не повторяй").unwrap().1, json!({"on": false}));
        assert!(command("повтори что ты сказал").is_none());
    }

    #[test]
    fn like_voice_commands_toggle_like() {
        assert_eq!(command("поставь лайк").unwrap(), ("like", json!({"liked": true}), "Поставил лайк."));
        assert_eq!(command("Джарвис, лайкни этот трек").unwrap().1, json!({"liked": true}));
        assert_eq!(command("убери лайк").unwrap(), ("like", json!({"liked": false}), "Убрал лайк."));
        assert_eq!(command("удали из понравившихся").unwrap().1, json!({"liked": false}));
    }

    #[test]
    fn auto_close_after_music_only_triggers_for_single_intent_play() {
        assert_eq!(played_music_title("play_music", "Включил с YouTube: Сало.").as_deref(), Some("Сало"));
        assert_eq!(
            played_music_title("play_music", "Включил понравившиеся с YouTube: Сало.").as_deref(),
            Some("Сало")
        );
        assert_eq!(
            played_music_title("play_music", "Включил плейлист «Lo-Fi»: Сало.").as_deref(),
            Some("Сало")
        );
        assert_eq!(
            played_music_title("play_music", "Включил плейлист с YouTube: Сало.").as_deref(),
            Some("Сало")
        );
        assert_eq!(
            played_music_title("music_control", "Включил «Сало».").as_deref(),
            Some("Сало")
        );
        assert!(played_music_title("play_music", "В очереди: Сало.").is_none());
        assert!(
            played_music_title(
                "play_music",
                "Нашёл «Сало». Послушаем? Ролик ещё не включён."
            )
            .is_none()
        );
        assert!(played_music_title("play_music", "Не получилось: ошибка").is_none());
        assert!(played_music_title("music_control", "Пауза.").is_none());

        assert!(wants_keep_talking("Включи Сало и расскажи новости"));
        assert!(wants_keep_talking("Включи музыку?"));
        assert!(!wants_keep_talking("Включи на ютубе Сало"));

        let mut tr = MusicTurnTracker::default();
        tr.observe_tool("play_music", r#"{"query":"Сало"}"#, "Включил с YouTube: Сало.");
        assert!(tr.should_close("Включил трек «Сало» с YouTube."));
        assert!(!tr.should_close("Включил трек «Сало». Что ещё сделать?"));

        let mut tr_keep = MusicTurnTracker::default();
        tr_keep.observe_tool(
            "play_music",
            r#"{"query":"Сало","keep_talking":true}"#,
            "Включил с YouTube: Сало.",
        );
        assert!(!tr_keep.should_close("Включил Сало."));

        let mut tr_multi = MusicTurnTracker::default();
        tr_multi.observe_tool("play_music", r#"{"query":"Сало"}"#, "Включил с YouTube: Сало.");
        tr_multi.observe_tool("smart_home", r#"{"on":false}"#, "Готово.");
        assert!(!tr_multi.should_close("Включил Сало и выключил свет."));
    }

    #[test]
    fn parses_liked_and_playlist_voice_intents() {
        assert_eq!(
            parse_play_intent("Джарвис, включи мои лайки"),
            Some(PlayIntent::Liked { shuffle: false })
        );
        assert_eq!(
            parse_play_intent("включи понравившиеся треки вперемешку"),
            Some(PlayIntent::Liked { shuffle: true })
        );
        assert_eq!(
            parse_play_intent("Джарвис, включи мои лайки на ютубе"),
            Some(PlayIntent::Liked { shuffle: false })
        );
        assert_eq!(
            parse_play_intent("Джарвис, поставь плейлист lo-fi"),
            Some(PlayIntent::Playlist {
                query: "lo fi".into(),
                shuffle: false
            })
        );
        assert_eq!(
            parse_play_intent("поставь мой плейлист в машину"),
            Some(PlayIntent::Playlist {
                query: "в машину".into(),
                shuffle: false
            })
        );
        assert_eq!(
            parse_play_intent("включи плейлист lo-fi на ютубе вперемешку"),
            Some(PlayIntent::Playlist {
                query: "lo fi".into(),
                shuffle: true
            })
        );

        assert!(parse_play_intent("поставь плейлист дня на яндекс музыке").is_none());
        assert!(parse_play_intent("включи мои лайки и расскажи новости").is_none());
        assert!(parse_play_intent("включи плейлист").is_none());
        assert!(parse_play_intent("включи плейлист на ютубе").is_none());

        assert_eq!(
            parse_query_intent("мои лайки"),
            Some(PlayIntent::Liked { shuffle: false })
        );
        assert_eq!(
            parse_query_intent("плейлист Synthwave"),
            Some(PlayIntent::Playlist {
                query: "synthwave".into(),
                shuffle: false
            })
        );
    }

    #[test]
    fn next_prev_and_direct_music_fast_path() {
        assert_eq!(command("следующий").unwrap().0, "next");
        assert_eq!(command("Следующий трек.").unwrap().0, "next");
        assert_eq!(command("дальше").unwrap().0, "next");
        assert_eq!(command("переключи песню").unwrap().0, "next");
        assert_eq!(command("включи следующую песню").unwrap().0, "next");
        assert_eq!(command("скип").unwrap().0, "next");
        assert_eq!(command("предыдущий трек").unwrap().0, "prev");
        assert_eq!(command("верни песню").unwrap().0, "prev");
        assert!(command("поставь следующим треком Daft Punk").is_none());
        assert!(command("переключи профиль на Рюка").is_none());

        assert_eq!(
            parse_direct_music_request("включи песню M83 Midnight City"),
            Some(PlayDirect::Track { query: "m83 midnight city".into() })
        );
        assert_eq!(
            parse_direct_music_request("поставь на ютубе The Weeknd Blinding Lights"),
            Some(PlayDirect::Track { query: "the weeknd blinding lights".into() })
        );
        assert_eq!(
            parse_direct_music_request("поставь следующим треком Daft Punk Instant Crush"),
            Some(PlayDirect::Queue { query: "daft punk instant crush".into() })
        );
        assert_eq!(
            parse_direct_music_request("включи мою волну"),
            Some(PlayDirect::Yandex { query: "включи мою волну".into() })
        );
        assert_eq!(
            parse_direct_music_request("включи мои лайки вперемешку"),
            Some(PlayDirect::Liked { shuffle: true })
        );
        // Generic / conversational / smart-home requests must stay on LLM / other paths
        assert!(parse_direct_music_request("включи музыку").is_none());
        assert!(parse_direct_music_request("включи бодрую музыку").is_none());
        assert!(parse_direct_music_request("включи песню для работы").is_none());
        assert!(parse_direct_music_request("включи песню которую мы слушали вчера").is_none());
        assert!(parse_direct_music_request("включи песню Кукушка и расскажи погоду").is_none());
        assert!(parse_direct_music_request("включи свет на кухне").is_none());
        assert!(parse_direct_music_request("включи камин").is_none());
        assert!(parse_direct_music_request("включи песню Белый свет").is_none());

        let mut tr = MusicTurnTracker::default();
        tr.observe_tool("play_music", r#"{"query":"M83 Midnight City"}"#, "Включил в YouTube Music: M83 — Midnight City.");
        assert_eq!(tr.can_instant_close(), Some("M83 — Midnight City"));
    }
}
