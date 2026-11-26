//! Инструменты для модели: умный дом (Яндекс, Home Assistant, Xiaomi, Smart Life — всё живёт в плагине
//! «Умный дом»), команды Алисе, инструменты плагинов. Простые команды колонка делает без модели (fastcmd).

use crate::{fastcmd, plugins::Host, settings::Store};
use serde_json::{Value, json};
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

static DEFS: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(include_str!("tooldefs.json")).unwrap());

fn def(name: &str) -> Value {
    DEFS.get(name).cloned().unwrap_or(Value::Null)
}

pub type MusicFn = Arc<dyn Fn(String) -> crate::plugins::BoxFut<String> + Send + Sync>;
pub type MusicFastFn=Arc<dyn Fn(String)->crate::plugins::BoxFut<Option<String>>+Send+Sync>;

pub fn local_voice_url(host: &Host) -> String {
    let url = host.plugin_setting_str("local-rvc-voice", "server_url");
    let u = if url.is_empty() { "http://192.168.1.3:8768" } else { url.as_str() };
    u.trim_end_matches('/').to_string()
}

pub struct Tools {
    pub host: Arc<Host>,
    pub store: Store,
    /// Слабая ссылка на движок для интерактивных модулей (книги, игры, AI-DJ, подкаст).
    pub engine: std::sync::OnceLock<std::sync::Weak<crate::engine::Engine>>,
    /// Включить с YouTube (None — нельзя).
    pub music: Option<MusicFn>,
    /// Управлять тем, что играет с YouTube.
    pub music_ctl: Option<MusicFn>,
    pub music_fast:Option<MusicFastFn>,
    /// Добавить в очередь YouTube.
    pub music_queue: Option<MusicFn>,
    /// Отменить незавершённый запуск музыки (например, по тайм-ауту инструмента).
    pub music_cancel: Option<Arc<dyn Fn() -> crate::plugins::BoxFut<()> + Send + Sync>>,
    /// Общая память: «вспомни», «забудь/запомни».
    pub memory: Option<Arc<crate::memory::Memory>>,
    /// Подсветка кольца (для мгновенного отклика при смене ассистента голосом).
    pub leds: Option<Arc<crate::led::Leds>>,
    /// Фоновые звуки и интернет-радио.
    pub ambient: Option<Arc<crate::ambient::Ambient>>,
    /// Фокус-таймер (Помодоро).
    pub focus: Option<Arc<crate::focus::Focus>>,
    /// Таймеры и ежедневные рутины.
    pub scheduler: Option<Arc<crate::scheduler::Scheduler>>,
    /// Запуск мини-подкаста / спора двух ассистентов.
    pub on_debate: Option<Arc<dyn Fn(String) + Send + Sync>>,
    /// Фокус-режим (Помодоро) голосом.
    pub on_focus: Option<Arc<dyn Fn(&str) -> Option<String> + Send + Sync>>,
    /// Интерактивные режимы (переводчик, RPG-квест, викторина, игры).
    pub modes: Option<Arc<crate::modes::ModeManager>>,
    /// Голосовые игры (Угадай мелодию, Нуарный детектив, Ведущий Мафии/Бункера).
    pub games: Option<Arc<crate::games::GameManager>>,
    /// Читалка книг и ранобэ.
    pub reader: Option<Arc<crate::reader::BookReader>>,
    /// Персональный AI-DJ.
    pub dj: Option<Arc<crate::dj::AiDj>>,
    /// Пользовательские сценарии.
    pub scenarios: Option<Arc<crate::scenarios::ScenarioStore>>,
    /// Определение присутствия по Wi-Fi.
    pub presence: Option<Arc<crate::presence::Presence>>,
}

const DAYS: [&str; 7] = ["воскресенье", "понедельник", "вторник", "среда", "четверг", "пятница", "суббота"];

fn room_line(room: &str) -> String {
    format!(
        "\nКолонка стоит в комнате «{room}». Если хозяин не назвал комнату («выключи свет», «включи кондиционер») — \
имеется в виду эта комната; «весь свет» — все лампы и выключатели света во всём доме."
    )
}

fn as_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

impl Tools {
    pub fn new(host: Arc<Host>, store: Store) -> Tools {
        Tools {
            host,
            store,
            engine: std::sync::OnceLock::new(),
            music: None,
            music_ctl: None,
            music_fast: None,
            music_queue: None,
            music_cancel: None,
            memory: None,
            leds: None,
            ambient: None,
            focus: None,
            scheduler: None,
            on_debate: None,
            on_focus: None,
            modes: None,
            games: None,
            reader: None,
            dj: None,
            scenarios: None,
            presence: None,
        }
    }

    pub fn engine(&self) -> Option<Arc<crate::engine::Engine>> {
        self.engine.get().and_then(|w| w.upgrade())
    }

    /// Арбитр аудио-источников: перед запуском нового режима останавливает конфликтующие.
    pub fn stop_for(&self, eng: &crate::engine::Engine, keep: &str) {
        if keep != "reader"
            && let Some(r) = &self.reader
        {
            r.stop();
        }
        if keep != "dj"
            && let Some(d) = &self.dj
        {
            d.stop(Some(eng));
        }
        if keep != "games"
            && let Some(g) = &self.games
        {
            g.stop_all(Some(eng));
        }
        if keep != "debate" {
            crate::debate::stop();
        }
        if keep != "modes"
            && keep != "games"
            && let Some(m) = &self.modes
        {
            m.reset(None);
        }
        if (keep == "reader" || keep == "debate") && eng.host.provider("youtube").is_ok() {
            let m = eng.music.clone();
            let h = eng.host.clone();
            tokio::spawn(async move {
                let _ = m.call(&h, "stop", Value::Null).await;
            });
        }
    }

    /// Централизованная остановка всех интерактивных активностей.
    pub fn stop_all_activities(
        &self,
        eng: Option<&crate::engine::Engine>,
        include_ambient_and_focus: bool,
    ) {
        if let Some(r) = &self.reader {
            r.stop();
        }
        if let Some(d) = &self.dj {
            d.stop(eng);
        }
        if let Some(g) = &self.games {
            g.stop_all(eng);
        }
        crate::debate::stop();
        if let Some(m) = &self.modes {
            m.reset(if include_ambient_and_focus {
                self.ambient.as_deref()
            } else {
                None
            });
        }
        if include_ambient_and_focus {
            if let Some(f) = &self.focus {
                f.stop();
            }
            if let Some(a) = &self.ambient {
                a.stop();
            }
        }
    }
    pub async fn cancel_loading_music(&self) {
        if let Some(cancel) = &self.music_cancel {
            cancel().await;
        }
    }
    /// Каталог из контекста определяет доступность инструментов уже в первой реплике.
    pub async fn prepare(&self)->(String,Vec<Value>) {
        let context=self.context().await;
        let definitions=self.definitions().await;
        (context,definitions)
    }

    /// Запрос к плагину, который даёт возможность cap.
    pub async fn service(&self, cap: &str, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let n = self.host.provider(cap)?;
        n.request(method, params, timeout).await
    }

    /// Галочка «через что управлять домом» и плагин на месте.
    pub fn use_ch(&self, ch: &str) -> bool {
        if ch == "yandex" && self.host.provider("yandexhome").is_err() {
            return false;
        }
        self.store.with(|s| s.use_home(ch))
    }

    fn ready(&self, ch: &str) -> bool {
        let cap = match ch {
            "homeassistant" | "xiaomi" | "smartlife" => ch,
            _ => return false,
        };
        self.store.with(|s| s.home_connected(ch)) && self.host.provider(cap).is_ok()
    }

    fn room(&self) -> String {
        self.store.with(|s| s.speaker_room.trim().to_string())
    }

    async fn yandex(&self, op: &str, args: Value, timeout: Duration) -> Result<Value, String> {
        let (token, alice) = (self.store.with(|s| s.yandex_token.clone()), self.use_ch("alice"));
        self.service("yandexhome", "yandexhome", json!({"op": op, "args": args, "token": token, "room": self.room(), "alice": alice}), timeout).await
    }

    async fn vendor(&self, ch: &str, op: &str, args: Value, timeout: Duration) -> Result<Value, String> {
        let mut p = json!({"op": op, "args": args});
        if ch == "homeassistant" {
            p["config"] = self.store.with(|s| s.home_assistant.clone());
        }
        self.service(ch, ch, p, timeout).await
    }

    fn has_token(&self) -> bool {
        self.store.with(|s| !s.yandex_token.is_empty())
    }

    async fn has_catalog(&self) -> bool {
        self.yandex("has-catalog", Value::Null, Duration::from_secs(1)).await.ok().and_then(|v| v.as_bool()).unwrap_or(false)
    }

    pub async fn prewarm(&self) {
        if self.has_token() && self.use_ch("yandex") {
            let _ = self.yandex("prewarm", Value::Null, Duration::from_secs(6)).await;
        }
    }

    /// Мгновенные встроенные команды YaJarvis (Фокус, Фон/Радио, Читалка, Игры, AI-DJ, Спор, Сценарии) без обращения к Алисе и облаку.
    pub fn fast_local(&self, text: &str) -> Option<String> {
        if let Some(scenario_id) = parse_scenario_command(text)
            && let Some(eng) = self.engine()
            && let Ok(ans) = run_scenario(eng, scenario_id)
        {
            return Some(ans);
        }
        if let Some(sc_store) = &self.scenarios
            && let Some(sc) = sc_store.match_voice(text)
            && let Some(eng) = self.engine()
        {
            let name = sc.name.clone();
            crate::scenarios::ScenarioStore::execute(eng, sc);
            return Some(format!("Запускаю сценарий «{name}»."));
        }
        if let Some(on_focus) = &self.on_focus
            && let Some(ans) = on_focus(text)
        {
            return Some(ans);
        }
        if let Some(sched) = &self.scheduler
            && let Some(ans) = sched.fast_command(text)
        {
            return Some(ans);
        }
        if let Some(amb) = &self.ambient
            && let Some(ans) = amb.fast_command(text)
        {
            return Some(ans);
        }
        if let Some(eng) = self.engine() {
            if let Some(reader) = &self.reader
                && let Some(ans) = reader.fast_command(&eng, text)
            {
                return Some(ans);
            }
            if let Some(games) = &self.games
                && let Some(ans) = crate::games::fast_game_command(games, &eng, text)
            {
                return Some(ans);
            }
            if let Some(dj) = &self.dj {
                let low = text.trim().to_lowercase();
                if dj.is_active() && (low.contains("диджей стоп") || low.contains("выключи дидже") || low.contains("останови дидже")) {
                    dj.stop(Some(&eng));
                    return Some("Выключаю AI-DJ.".into());
                }
                if dj.is_active() && (low == "дальше" || low.contains("следующий трек")) {
                    dj.next();
                    return Some("Переключаю на следующий трек подборки.".into());
                }
                for pat in ["поставь вайб ", "включи вайб ", "диджей поставь ", "диджей включи ", "режим диджея "] {
                    if let Some(pos) = low.find(pat) {
                        let trimmed = text.trim();
                        let tail = if trimmed.len() == low.len() {
                            trimmed[pos + pat.len()..].trim()
                        } else {
                            low[pos + pat.len()..].trim()
                        };
                        if !tail.is_empty() {
                            let dj_c = dj.clone();
                            let eng_c = eng.clone();
                            let mood = tail.to_string();
                            tokio::spawn(async move {
                                let _ = dj_c.start(eng_c, mood);
                            });
                            return Some(format!("Запускаю AI-DJ с вайбом «{tail}»!"));
                        }
                    }
                }
            }
        }
        if let Some(on_debate) = &self.on_debate {
            let trimmed = text.trim();
            let low = trimmed.to_lowercase();
            for pat in ["поспорьте про ", "поспорьте о ", "поспорьте об ", "подкаст про ", "подкаст о ", "устройте спор про ", "устройте спор о "] {
                if let Some(pos) = low.find(pat) {
                    let tail = if trimmed.len() == low.len() {
                        trimmed[pos + pat.len()..].trim()
                    } else {
                        low[pos + pat.len()..].trim()
                    };
                    if !tail.is_empty() {
                        on_debate(tail.to_string());
                        return Some(format!("Запускаю спор на тему «{tail}»!"));
                    }
                }
            }
        }
        None
    }

    /// Быстрая команда Яндекс Умного дома без модели.
    pub async fn fast(&self, text: &str) -> Option<String> {
        if let Some(ans) = self.fast_local(text) {
            return Some(ans);
        }
        if is_book_recap_command(text)
            && let Some(reader) = &self.reader
            && let Some(eng) = self.engine()
        {
            return Some(reader.recap(eng).await.unwrap_or_else(|e| e));
        }
        if is_book_quote_command(text)
            && let Some(reader) = &self.reader
            && let Some(eng) = self.engine()
        {
            return Some(reader.save_quote(&eng).await.unwrap_or_else(|e| e));
        }
        if crate::briefing::is_briefing_command(text)
            && let Some(eng) = self.engine()
        {
            return Some(
                crate::briefing::generate_briefing_text(&eng, "")
                    .await
                    .unwrap_or_else(|e| e),
            );
        }
        if !self.has_token() || !self.use_ch("yandex") {
            return None;
        }
        let v = self.yandex("fast", text.into(), Duration::from_secs(6)).await.ok()?;
        v.get("handled").and_then(Value::as_bool).unwrap_or(false).then(|| as_str(&v["answer"]))
    }

    /// Smart Life без модели.
    pub async fn smartlife_fast(&self, text: &str) -> Option<String> {
        if !self.ready("smartlife") || !self.use_ch("smartlife") {
            return None;
        }
        match self.vendor("smartlife", "fast", text.into(), Duration::from_secs(6)).await {
            Err(e) => Some(e),
            Ok(v) => v.get("handled").and_then(Value::as_bool).unwrap_or(false).then(|| as_str(&v["answer"])),
        }
    }

    pub async fn ha_alive(&self) -> bool {
        self.ready("homeassistant")
            && self.vendor("homeassistant", "alive", Value::Null, Duration::from_secs(2)).await.ok().and_then(|v| v.as_bool()).unwrap_or(false)
    }

    /// Пылесос в Home Assistant — быстрым путём, без модели.
    pub async fn ha_vacuum(&self, text: &str) -> Option<String> {
        if !self.ready("homeassistant") || !self.use_ch("homeassistant") {
            return None;
        }
        let intent = self.vendor("homeassistant", "intent", text.into(), Duration::from_secs(1)).await.ok().and_then(|v| v.as_bool()).unwrap_or(false);
        if !intent || !self.ha_alive().await {
            return None;
        }
        Some(match self.vendor("homeassistant", "command", text.into(), Duration::from_secs(20)).await {
            Ok(v) => as_str(&v),
            Err(e) => e,
        })
    }

    /// Короткая команда умного дома или фраза сценария — её можно отдать Алисе целиком.
    pub async fn home_command(&self, text: &str) -> Option<String> {
        if fastcmd::youtube_request(text) || fastcmd::clarification(text).is_some() {
            return None;
        }
        if self.direct_home_owns(text).await {
            return None;
        }
        if let Some(b) = fastcmd::home_command_local(text) {
            return Some(b);
        }
        let bare = fastcmd::strip_lead(&fastcmd::norm_text(text));
        if bare.is_empty() || bare.split_whitespace().count() > 9 || !self.has_token() {
            return None;
        }
        let m = self.yandex("scenario-match", bare.clone().into(), Duration::from_secs(6)).await.ok()?;
        (m.as_bool() == Some(true)).then_some(bare)
    }

    async fn direct_home_owns(&self, text: &str) -> bool {
        if self.ready("xiaomi") && self.use_ch("xiaomi") {
            if let Ok(Value::Array(devices)) =
                self.vendor("xiaomi", "devices", Value::Null, Duration::from_millis(600)).await
                && devices.iter().any(|d| device_matches_command(text, d))
            {
                return true;
            }
        }
        if self.use_ch("homeassistant") && self.ha_alive().await {
            if let Ok(Value::Array(entities)) =
                self.vendor("homeassistant", "catalog", json!(false), Duration::from_millis(600)).await
                && entities.iter().any(|d| device_matches_command(text, d))
            {
                return true;
            }
        }
        false
    }

    /// Текст для инструкций ассистента: время, живое состояние колонки и подключённые контексты.
    pub async fn context(&self) -> String {
        let (now, city, night_now) = self.store.with(|s| (s.now(), s.city.trim().to_string(), s.is_night()));
        let mut b = format!("\n\nСейчас {}, {}.", now.strftime("%d.%m.%Y %H:%M"), DAYS[now.weekday().to_sunday_zero_offset() as usize]);
        if !city.is_empty() {
            b += &format!(" Город хозяина: {city}.");
        }
        if night_now {
            b += " Сейчас действует ночной режим — говори чуть спокойнее и мягче.";
        }
        let mut live_parts: Vec<String> = Vec::new();
        if let Some(amb) = &self.ambient
            && let Some(lbl) = amb.current_label()
        {
            live_parts.push(format!("фоновый звук «{lbl}»"));
        }
        if let Some(dj) = &self.dj
            && dj.is_active()
        {
            let st = dj.status();
            let title = st["current_title"].as_str().unwrap_or("");
            live_parts.push(if title.is_empty() { "AI-DJ в эфире".into() } else { format!("AI-DJ (трек: {title})") });
        }
        if let Some(r) = &self.reader
            && r.is_running()
        {
            let st = r.status();
            let title = st["title"].as_str().unwrap_or("книга");
            live_parts.push(format!("читается книга «{title}»"));
        }
        if let Some(sch) = &self.scheduler {
            let timers = sch.active_timers_json();
            if let Some(arr) = timers.as_array()
                && !arr.is_empty()
            {
                let items: Vec<String> = arr
                    .iter()
                    .take(4)
                    .map(|t| {
                        let lbl = t["label"].as_str().unwrap_or("таймер");
                        let rem = crate::scheduler::format_duration_ru(t["remaining_s"].as_u64().unwrap_or(0));
                        format!("{lbl} ({rem})")
                    })
                    .collect();
                live_parts.push(format!("активные таймеры: {}", items.join(", ")));
            }
        }
        if !live_parts.is_empty() {
            b += &format!(" Сейчас на колонке: {}.", live_parts.join("; "));
        }
        // Independent providers must not add their cold-start delays together.
        // Definitions are still prepared after context, so their first catalog is ready.
        let ha=async{if self.use_ch("homeassistant") && self.ha_alive().await{as_str(&self.vendor("homeassistant","context",Value::Null,Duration::from_secs(4)).await.unwrap_or_default())}else{String::new()}};
        let vendor=|ch|async move{if self.use_ch(ch) && self.ready(ch){as_str(&self.vendor(ch,"context",Value::Null,Duration::from_secs(5)).await.unwrap_or_default())}else{String::new()}};
        let yandex=async{if self.use_ch("yandex"){self.yandex("context",Value::Null,Duration::from_secs(5)).await.map(|v|as_str(&v)).unwrap_or_default()}else{String::new()}};
        let (ha,xiaomi,smartlife,yandex)=tokio::join!(ha,vendor("xiaomi"),vendor("smartlife"),yandex);
        b+=&ha;b+=&xiaomi;b+=&smartlife;
        if !self.use_ch("yandex") {
            let room = self.room();
            if !room.is_empty() {
                b += &room_line(&room);
            }
            return b;
        }
        b+=&yandex;
        b
    }

    pub async fn definitions(&self) -> Vec<Value> {
        let mut defs = vec![];
        let available=|ch|async move {self.ready(ch) && self.use_ch(ch) && self.vendor(ch,"devices",Value::Null,Duration::from_secs(5)).await.ok().and_then(|v|v.as_array().map(Vec::len)).unwrap_or(0)>0};
        let (ha,xiaomi,smartlife)=tokio::join!(async{self.use_ch("homeassistant") && self.ha_alive().await},available("xiaomi"),available("smartlife"));
        if ha {
            defs.push(def("home_assistant"));
        }
        for (available,name) in [(xiaomi,"xiaomi_home"),(smartlife,"smart_life")] {
            if available{defs.push(def(name));}
        }
        if self.music.is_some() && self.host.provider("youtube").is_ok() {
            let who = if self.use_ch("yandexmusic") {
                "YouTube / YouTube Music — ОСНОВНОЙ музыкальный сервис по умолчанию. Любую просьбу «включи музыку», трек, песню, исполнителя, лайки, плейлист или «на YouTube / YouTube Music» ВСЕГДА включай через play_music (если просят просто «включи музыку» — подбери музыку сам или включи понравившиеся). В Яндекс Музыку (alice_command) отправляй ТОЛЬКО если хозяин явно упомянул «Яндекс», «Яндекс Музыка», «Алиса» или «Моя волна»."
            } else {
                "Яндекс Музыка выключена в настройках: любую музыку, песню или радио включай только этим инструментом, сразу, запрос придумай сам, если не назвали."
            };
            defs.push(json!({"type": "function", "name": "play_music",
                "description": format!("Найти по всему YouTube / YouTube Music и включить звук видео, музыки, плейлиста или лайков аккаунта на колонке: любой ролик, трек, кавер, ремикс, концерт, подкаст, песню, сохранённый/публичный плейлист или понравившиеся треки. {who} Если сказали только «включи на YouTube трек», сначала спроси название или исполнителя. Подбирай сам, когда об этом попросили. Не обещай «сейчас поищу» без вызова."),
                "parameters": {"type": "object", "properties": {
                    "query": {"type": "string", "description": "что искать или включить: исполнитель и название, «мои лайки» / «понравившиеся», или «плейлист <название>»"},
                    "queue": {"type": "boolean", "description": "true — не включать сразу, а добавить в очередь после того, что играет («добавь в очередь», «потом включи»)"},
                    "keep_talking": {"type": "boolean", "description": "true — если кроме включения музыки хозяин попросил ещё о чём-то (ответить на вопрос, рассказать, не отключаться)"}},
                    "required": ["query"]}}));
        }
        if self.music_ctl.is_some() && self.host.provider("youtube").is_ok() {
            defs.push(json!({"type": "function", "name": "music_control",
                "description": "Управлять тем, что сейчас играет с YouTube: пауза, продолжить, перемотка вперёд/назад, с начала, следующее видео, похожее (радио), повтор (на репит), поставь лайк, убери лайк, выключить, что играет.",
                "parameters": {"type": "object", "properties": {
                    "command": {"type": "string", "description": "команда по-русски, например «пауза», «продолжи», «перемотай вперёд на 2 минуты», «назад на 30 секунд», «сначала», «следующее», «включи похожее», «включи повтор», «выключи повтор», «поставь лайк», «убери лайк», «выключи музыку», «что играет»"}},
                    "required": ["command"]}}));
        }
        if self.use_ch("alice") {
            let mut alice_tool = def("alice_command");
            if self.music.is_some()
                && self.host.provider("youtube").is_ok()
                && let Some(desc) = alice_tool.get("description").and_then(Value::as_str)
            {
                alice_tool["description"] = Value::String(format!(
                    "{desc} ВАЖНО: для музыки используй alice_command ТОЛЬКО если хозяин ЯВНО попросил Яндекс Музыку, Алису или Мою волну. Обычную музыку («включи музыку», треки, YouTube, YouTube Music, лайки, плейлисты) ВСЕГДА включай через play_music, НЕ через alice_command."
                ));
            }
            defs.push(alice_tool);
        }
        if self.memory.as_ref().is_some_and(|m| m.enabled()) {
            defs.push(def("recall_conversation"));
            defs.push(def("edit_memory"));
        }
        let assistant_names: Vec<String> = self.store.with(|s| {
            s.profiles.iter().map(|p| {
                if p.id == s.active { format!("{} (активен)", p.name) } else { p.name.clone() }
            }).collect()
        });
        if !assistant_names.is_empty() {
            defs.push(json!({
                "type": "function",
                "name": "switch_assistant",
                "description": format!("Переключить активного ассистента/голос на колонке по просьбе хозяина («переключи ассистента на ...», «позови ...», «включи голос ...», «какие есть ассистенты»). Доступные ассистенты: {}.", assistant_names.join(", ")),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {"type": "string", "enum": ["switch", "list"], "description": "switch — переключить на ассистента по имени; list — перечислить всех ассистентов"},
                        "name": {"type": "string", "description": "имя (или часть имени) нужного ассистента из списка"}
                    },
                    "required": ["action"]
                }
            }));
        }
        defs.extend(self.host.tools());
        if self.has_token() && self.use_ch("yandex") && self.has_catalog().await {
            defs.push(def("smart_home"));
        }
        let s_snap = self.store.get();
        if !s_snap.pc_agent_url.trim().is_empty() {
            defs.push(json!({
                "type": "function",
                "name": "look_at_screen",
                "description": "Посмотреть на экран компьютера хозяина (через PC-компаньон и Vision-модель) и ответить на его вопрос по изображению («посмотри на экран», «что тут за ошибка», «кто этот персонаж на экране»).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "question": {"type": "string", "description": "что именно нужно увидеть или объяснить на экране"}
                    },
                    "required": ["question"]
                }
            }));
            defs.push(json!({
                "type": "function",
                "name": "pc_control",
                "description": "Выполнить безопасную команду на компьютере хозяина (через PC-компаньон): медиа пауза/далее/назад, громкость ПК, скриншот, блокировка экрана, таймер выключения, узнать активное окно, прочитать/записать буфер обмена, открыть ссылку http(s), запустить макрос или узнать загрузку ПК.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["media_play_pause", "media_next", "media_prev", "volume_up", "volume_down", "volume_mute", "lock_screen", "sleep_timer", "cancel_sleep", "screenshot_save", "active_window", "clipboard_read", "clipboard_write", "open_url", "run_macro", "system_info"]
                        },
                        "minutes": {"type": "integer", "description": "минуты до выключения (только для sleep_timer)"},
                        "text": {"type": "string", "description": "текст для записи в буфер обмена (для clipboard_write)"},
                        "url": {"type": "string", "description": "ссылка http:// или https:// для открытия в браузере ПК (для open_url)"},
                        "macro_id": {"type": "string", "description": "идентификатор макроса из ~/.jarvis-pc-macros.json (для run_macro)"}
                    },
                    "required": ["action"]
                }
            }));
        }
        if s_snap.cybernotes_enabled {
            defs.push(json!({
                "type": "function",
                "name": "save_note",
                "description": "Сохранить мысль, идею, задачу или заметку в личную базу знаний CyberNotes («запиши идею», «сохрани заметку про...»).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string", "description": "короткий чёткий заголовок заметки"},
                        "content": {"type": "string", "description": "подробный текст заметки"},
                        "tags": {"type": "array", "items": {"type": "string"}, "description": "1–3 коротких тега без #"}
                    },
                    "required": ["title", "content"]
                }
            }));
            defs.push(json!({
                "type": "function",
                "name": "search_notes",
                "description": "Найти информацию по смыслу или ключевым словам в заметках CyberNotes и долгой памяти Джарвиса («что я записывал про...», «найди в моих заметках», «какая идея была насчёт...»).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "поисковый запрос, тема или ключевые слова"}
                    },
                    "required": ["query"]
                }
            }));
        }
        if self.dj.is_some() && self.music.is_some() && self.host.provider("youtube").is_ok() {
            defs.push(json!({
                "type": "function",
                "name": "start_ai_dj",
                "description": "Запустить персонального AI-DJ: колонка подберёт тематическую волну на YouTube под нужное настроение/вайб и будет делать короткие радио-подводки между треками.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "mood": {"type": "string", "description": "описание вайба/настроения, например «ночной Токио синтвейв» или «чилл аниме OST»"}
                    },
                    "required": ["mood"]
                }
            }));
        }
        defs.push(json!({
            "type": "function",
            "name": "start_notebook_podcast",
            "description": "Запустить подкаст-обсуждение двумя голосами (Локальный NotebookLM) по ссылке на статью/YouTube или тексту/теме.",
            "parameters": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "ссылка на статью или YouTube (если указана)"},
                    "text": {"type": "string", "description": "тема или текст материала для обсуждения"},
                    "title": {"type": "string", "description": "краткое название выпуска"}
                }
            }
        }));
        if self.ambient.is_some() {
            defs.push(json!({
                "type": "function",
                "name": "control_ambient",
                "description": "Управлять студийными фоновыми звуками 48 кГц и фоновым радио прямо на колонке: зажечь/включить камин (fireplace), дождь (rain), океан (ocean), мягкий шум для сна (white), Lo-Fi радио (lofi), джаз (jazz), чилл (chill), остановить фон, поменять громкость фона или поставить таймер сна для фона.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {"type": "string", "enum": ["start", "stop", "volume", "sleep"], "description": "start — включить звук/радио; stop — выключить фон; volume — громкость фона; sleep — таймер отключения фона"},
                        "preset": {"type": "string", "description": "fireplace (камин), rain (дождь), ocean (океан), white (мягкий шум), lofi, jazz, chill"},
                        "volume": {"type": "integer", "description": "громкость фона от 5 до 100 (для action=volume)"},
                        "sleep_minutes": {"type": "integer", "description": "через сколько минут выключить фон, 0 — отменить таймер (для action=sleep)"}
                    },
                    "required": ["action"]
                }
            }));
        }
        if self.scheduler.is_some() {
            defs.push(json!({
                "type": "function",
                "name": "manage_timer",
                "description": "Нативные таймеры и голосовые напоминания на колонке (без Алисы): поставить таймер или напоминание через N минут/секунд, проверить активные таймеры или отменить их.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {"type": "string", "enum": ["set", "list", "cancel_all"], "description": "set — поставить таймер/напоминание; list — какие таймеры идут; cancel_all — снять все таймеры"},
                        "minutes": {"type": "number", "description": "длительность в минутах (для set)"},
                        "seconds": {"type": "integer", "description": "длительность в секундах (если указаны секунды, для set)"},
                        "label": {"type": "string", "description": "короткая подпись таймера или текст напоминания, например «Чай» или «Проверить духовку»"}
                    },
                    "required": ["action"]
                }
            }));
        }
        defs.push(json!({
            "type": "function",
            "name": "set_speaker_volume",
            "description": "Изменить громкость самой колонки тихо и напрямую (без Алисы): сделать громче, тише, выключить звук или выставить точный уровень от 0 до 10.",
            "parameters": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["set", "up", "down", "mute"], "description": "set — точный уровень 0..10; up — громче; down — тише; mute — без звука"},
                    "level": {"type": "integer", "description": "уровень громкости от 0 до 10 (для set)"}
                },
                "required": ["action"]
            }
        }));
        defs.push(json!({
            "type": "function",
            "name": "run_scenario",
            "description": "Запустить домашний сценарий колонки: morning (Утро — брифинг и бодрый фон), work (Работа — фокус и Lo-Fi), evening (Вечер — камин и уют), sleep (Сон — океан и таймер сна), либо пользовательский сценарий по названию.",
            "parameters": {
                "type": "object",
                "properties": {
                    "scenario": {"type": "string", "description": "morning, work, evening, sleep или название пользовательского сценария"}
                },
                "required": ["scenario"]
            }
        }));
        if self.reader.is_some() {
            defs.push(json!({
                "type": "function",
                "name": "control_book",
                "description": "Управление аудиокнигой на колонке: пересказать суть («на чём остановились»), поставить на паузу, сохранить цитату в CyberNotes или поставить таймер сна для книги.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {"type": "string", "enum": ["recap", "pause", "save_quote", "sleep"]},
                        "sleep_minutes": {"type": "integer", "description": "минуты до остановки книги (для sleep)"}
                    },
                    "required": ["action"]
                }
            }));
        }
        defs.push(json!({
            "type": "function",
            "name": "morning_briefing",
            "description": "Собрать живую персональную сводку дня для хозяина: погода, активные таймеры и рутины на сегодня, прогресс книги и свежие заметки CyberNotes.",
            "parameters": {"type": "object", "properties": {}}
        }));
        if self.modes.is_some() || self.games.is_some() {
            defs.push(json!({
                "type": "function",
                "name": "control_mode_or_game",
                "description": "Запустить или остановить интерактивные режимы и голосовые игры колонки: свободный диалог (chat — длинная живая беседа без повторных окликов), RPG-квест (rpg), викторина (quiz), угадай мелодию (melody), нуарный детектив (detective), синхронный переводчик (translator) или выйти из режима (stop).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {"type": "string", "enum": ["chat", "rpg", "quiz", "melody", "detective", "translator", "stop"]},
                        "theme": {"type": "string", "description": "тема игры (например «аниме опенинги» для melody или сюжет для detective)"},
                        "language": {"type": "string", "description": "язык перевода для translator (например «английский», «японский»)"}
                    },
                    "required": ["action"]
                }
            }));
        }
        defs
    }

    pub async fn call(&self, name: &str, args: &str) -> String {
        let a: Value = serde_json::from_str(args).unwrap_or(Value::Null);
        let field = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
        let ch = match name {
            "home_assistant" => Some("homeassistant"),
            "xiaomi_home" => Some("xiaomi"),
            "smart_life" => Some("smartlife"),
            _ => None,
        };
        if let Some(ch) = ch
            && self.ready(ch)
            && (ch != "homeassistant" || self.use_ch(ch))
        {
            return match self.vendor(ch, "control", args.into(), Duration::from_secs(15)).await {
                Ok(v) => as_str(&v),
                Err(e) => e,
            };
        }
        if name == "play_music"
            && let Some(m) = &self.music
        {
            let q = field("query");
            if q.is_empty() {
                return "Не понял, что включить.".into();
            }
            if fastcmd::yandex_music_request(&q) && self.use_ch("alice") {
                return "Хозяин явно попросил Яндекс Музыку / Алису — вызови alice_command вместо play_music.".into();
            }
            if a.get("queue").and_then(Value::as_bool) == Some(true)
                && let Some(mq) = &self.music_queue
            {
                return mq(q).await;
            }
            return m(q).await;
        }
        if name == "switch_assistant" {
            let action = field("action");
            let target = field("name").to_lowercase();
            if action == "list" || target.is_empty() {
                let list = self.store.with(|s| {
                    s.profiles.iter().map(|p| {
                        if p.id == s.active { format!("{} (сейчас активен)", p.name) } else { p.name.clone() }
                    }).collect::<Vec<_>>().join(", ")
                });
                return format!("Доступные ассистенты: {list}.");
            }
            let found = self.store.with(|s| {
                s.profiles.iter().find(|p| {
                    let pn = p.name.to_lowercase();
                    pn == target || pn.contains(&target) || p.id.to_lowercase() == target
                }).map(|p| (p.id.clone(), p.name.clone()))
            });
            return match found {
                Some((id, pname)) => {
                    let res = self.store.update(|s| {
                        s.active = id.clone();
                        Ok::<(), std::io::Error>(())
                    });
                    match res {
                        Ok(()) => {
                            if let Some(leds) = &self.leds {
                                let l = self.store.led_for();
                                leds.configure(&l);
                                leds.show(crate::led::Mode::Listening, &l.listening, l.brightness, Duration::from_secs(2));
                            }
                            format!("Переключил ассистента на «{pname}». Со следующего обращения будет отвечать он.")
                        }
                        Err(e) => format!("Не удалось переключить ассистента: {e}"),
                    }
                }
                None => {
                    let list = self.store.with(|s| s.profiles.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", "));
                    format!("Ассистент «{target}» не найден. Доступны: {list}.")
                }
            };
        }
        if name == "music_control"
            && let Some(m) = &self.music_ctl
        {
            let c = field("command");
            if c.is_empty() {
                return "Не понял команду для музыки.".into();
            }
            return m(c).await;
        }
        if let Some(m) = &self.memory {
            if name == "recall_conversation" {
                let (q, d) = (field("query"), field("date"));
                if q.is_empty() && d.is_empty() {
                    return "Скажи, что вспомнить: ключевые слова или день.".into();
                }
                return m.search(&q, &d);
            }
            if name == "edit_memory" {
                let words: Vec<String> = a.get("words").and_then(Value::as_array).map(|w| w.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
                return m.edit(&field("forget"), &field("remember"), &words);
            }
        }
        if name == "alice_command" && self.use_ch("alice") {
            let cmd = field("command");
            if cmd.is_empty() {
                return "Не понял команду для Алисы.".into();
            }
            if let Some(question) = fastcmd::clarification(&cmd) {
                return question.into();
            }
            if fastcmd::youtube_request(&cmd) {
                return if self.host.provider("youtube").is_ok() {
                    "Для просьбы включить с YouTube / YouTube Music вызови play_music.".into()
                } else {
                    "Для этой команды нужен установленный и включённый плагин YouTube.".into()
                };
            }
            if self.host.provider("youtube").is_ok() {
                let bare = fastcmd::strip_lead(&fastcmd::norm_text(&cmd));
                if fastcmd::alice_music_request(&bare) && !fastcmd::yandex_music_request(&bare) {
                    return "Для обычной музыки и YouTube Music используй инструмент play_music, а не alice_command. Вызывай alice_command для музыки только если хозяин явно попросил Яндекс Музыку или Алису.".into();
                }
            }
            let schedule = fastcmd::alice_schedule(&cmd);
            let c2 = cmd.clone();
            let is_schedule = schedule == fastcmd::AliceSchedule::Ready;
            return match tokio::task::spawn_blocking(move || crate::alice::text(&c2, !is_schedule)).await.unwrap_or_else(|e| Err(e.to_string())) {
                Err(e) => format!("Алиса недоступна: {e}"),
                Ok(()) => {
                    log::info!("инструмент: «{cmd}» → Алисе");
                    if is_schedule {
                        "Команда передана. Алиса ответит своим голосом. Не утверждай, что команда выполнена.".into()
                    } else {
                        format!("Передал Алисе: «{cmd}». Она выполняет — скажи коротко, что сделано.")
                    }
                }
            };
        }
        if name == "look_at_screen" {
            let question = field("question");
            let q = if question.is_empty() { "Что сейчас на экране?" } else { question.as_str() };
            return match capture_and_describe_screen(&self.host, &self.store, q).await {
                Ok(ans) => ans,
                Err(e) => format!("Не получилось посмотреть экран: {e}"),
            };
        }
        if name == "pc_control" {
            let action = field("action");
            match action.as_str() {
                "clipboard_read" => {
                    return match pc_clipboard_read(&self.store).await {
                        Ok(txt) if txt.is_empty() => "Буфер обмена на ПК пуст.".into(),
                        Ok(txt) => format!("В буфере обмена ПК: {txt}"),
                        Err(e) => format!("Не удалось прочитать буфер ПК: {e}"),
                    };
                }
                "clipboard_write" => {
                    let text = field("text");
                    return match pc_clipboard_write(&self.store, &text).await {
                        Ok(msg) => msg,
                        Err(e) => format!("Не удалось записать в буфер ПК: {e}"),
                    };
                }
                "open_url" => {
                    let url = field("url");
                    return match pc_open_url(&self.store, &url).await {
                        Ok(msg) => msg,
                        Err(e) => format!("Не удалось открыть ссылку на ПК: {e}"),
                    };
                }
                "run_macro" => {
                    let mid = field("macro_id");
                    return match pc_macro_run(&self.store, &mid).await {
                        Ok(msg) => msg,
                        Err(e) => format!("Не удалось запустить макрос на ПК: {e}"),
                    };
                }
                "system_info" => {
                    return match pc_system_info(&self.store).await {
                        Ok(info) => format_pc_system_summary(&info),
                        Err(e) => format!("Не удалось получить статус ПК: {e}"),
                    };
                }
                _ => {}
            }
            let mins = a.get("minutes").and_then(Value::as_i64).unwrap_or(30);
            return match call_pc_action(&self.store, &action, mins).await {
                Ok(msg) => msg,
                Err(e) => format!("Не удалось выполнить команду на ПК: {e}"),
            };
        }
        if name == "save_note" {
            let title = field("title");
            let content = field("content");
            let tags: Vec<String> = a
                .get("tags")
                .and_then(Value::as_array)
                .map(|xs| xs.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            return match save_cybernote(&self.host, &title, &content, &tags, "Голосовые заметки").await {
                Ok(path) => format!("Записал заметку «{path}» в CyberNotes."),
                Err(e) => format!("Не удалось сохранить заметку: {e}"),
            };
        }
        if name == "search_notes" {
            let q = field("query");
            return match semantic_search_all(&self.host, self.memory.as_deref(), &q, 6).await {
                Ok(v) => {
                    let hits = v["hits"].as_array().cloned().unwrap_or_default();
                    if hits.is_empty() {
                        match search_cybernotes(&self.host, &q).await {
                            Ok(s) => s,
                            Err(e) => format!("Не удалось поискать в заметках: {e}"),
                        }
                    } else {
                        let lines: Vec<String> = hits
                            .iter()
                            .map(|h| {
                                let kind = if h["source"].as_str() == Some("note") { "Заметка" } else { "Память" };
                                let title = h["title"].as_str().unwrap_or("");
                                let folder = h["folder"].as_str().unwrap_or("");
                                let snippet = h["snippet"].as_str().unwrap_or("");
                                if folder.is_empty() {
                                    format!("• [{kind}] «{title}»: {snippet}")
                                } else {
                                    format!("• [{kind}] «{title}» ({folder}): {snippet}")
                                }
                            })
                            .collect();
                        format!("Найдено по смыслу:\n{}", lines.join("\n"))
                    }
                }
                Err(_) => match search_cybernotes(&self.host, &q).await {
                    Ok(summary) => summary,
                    Err(e) => format!("Не удалось поискать в заметках: {e}"),
                },
            };
        }
        if name == "start_ai_dj"
            && let Some(dj) = &self.dj
            && let Some(eng) = self.engine()
        {
            let mood = field("mood");
            return match dj.start(eng, mood.clone()) {
                Ok(st) => format!("Запускаю волну «{}»!", st["station"].as_str().unwrap_or(&mood)),
                Err(e) => e,
            };
        }
        if name == "start_notebook_podcast"
            && let Some(eng) = self.engine()
        {
            let url = field("url");
            let text = field("text");
            let title = field("title");
            crate::debate::spawn_notebook(
                eng,
                crate::debate::NotebookParams {
                    url,
                    text,
                    title,
                    p1_id: None,
                    p2_id: None,
                    turns: 6,
                },
            );
            return "Запускаю подкаст-разбор двумя голосами!".into();
        }
        if name == "control_ambient"
            && let Some(amb) = &self.ambient
        {
            let action = field("action");
            return match action.as_str() {
                "stop" => {
                    amb.stop();
                    "Фоновый звук выключен.".into()
                }
                "volume" => {
                    let vol = a.get("volume").and_then(Value::as_i64).unwrap_or(50).clamp(5, 100);
                    amb.set_volume(vol);
                    format!("Громкость фона выставлена на {vol}%.")
                }
                "sleep" => {
                    let mins = a.get("sleep_minutes").and_then(Value::as_u64).unwrap_or(30);
                    amb.set_sleep(mins)
                }
                _ => {
                    let raw = field("preset").to_lowercase();
                    let id = match raw.as_str() {
                        "камин" | "костёр" | "костер" | "fire" | "fireplace" => "fireplace",
                        "дождь" | "rain" => "rain",
                        "океан" | "прибой" | "море" | "ocean" => "ocean",
                        "белый шум" | "шум" | "white" => "white",
                        "лоуфай" | "лофай" | "lo-fi" | "lofi" => "lofi",
                        "джаз" | "jazz" => "jazz",
                        "чилл" | "chill" => "chill",
                        other => other,
                    };
                    match amb.start(if id.is_empty() { "fireplace" } else { id }) {
                        Ok(lbl) => format!("Включил фон: {lbl}."),
                        Err(e) => format!("Не получилось включить фон: {e}"),
                    }
                }
            };
        }
        if name == "manage_timer"
            && let Some(sch) = &self.scheduler
        {
            let action = field("action");
            return match action.as_str() {
                "list" => {
                    let timers = sch.active_timers_json();
                    let Some(arr) = timers.as_array() else { return "Активных таймеров сейчас нет.".into(); };
                    if arr.is_empty() {
                        "Активных таймеров сейчас нет.".into()
                    } else {
                        let list: Vec<String> = arr
                            .iter()
                            .map(|t| {
                                let lbl = t["label"].as_str().unwrap_or("Таймер");
                                let rem = crate::scheduler::format_duration_ru(t["remaining_s"].as_u64().unwrap_or(0));
                                format!("{lbl} — осталось {rem}")
                            })
                            .collect();
                        format!("Активные таймеры: {}.", list.join("; "))
                    }
                }
                "cancel_all" => {
                    let n = sch.cancel_all_timers();
                    if n == 0 {
                        "Активных таймеров и так не было.".into()
                    } else {
                        format!("Отменил таймеров: {n}.")
                    }
                }
                _ => {
                    let mins_s = a.get("minutes").and_then(Value::as_f64).map(|m| (m * 60.0).round() as u64).unwrap_or(0);
                    let secs_s = a.get("seconds").and_then(Value::as_u64).unwrap_or(0);
                    let total = mins_s.saturating_add(secs_s).clamp(5, 86400);
                    if mins_s == 0 && secs_s == 0 {
                        return "Уточните, на сколько минут или секунд поставить таймер.".into();
                    }
                    let label = {
                        let l = field("label");
                        if l.is_empty() { "Таймер".into() } else { l }
                    };
                    match sch.add_timer(label.clone(), total) {
                        Ok(_) => format!("Поставил таймер «{label}» на {}.", crate::scheduler::format_duration_ru(total)),
                        Err(e) => e,
                    }
                }
            };
        }
        if name == "set_speaker_volume" {
            let action = field("action");
            let cur = tokio::task::spawn_blocking(crate::speakervol::speaker_volume)
                .await
                .ok()
                .flatten()
                .unwrap_or(5);
            let target = match action.as_str() {
                "up" => (cur + 2).min(10),
                "down" => (cur - 2).max(1),
                "mute" => 0,
                _ => a.get("level").and_then(Value::as_i64).unwrap_or(cur).clamp(0, 10),
            };
            return match tokio::task::spawn_blocking(move || crate::speakervol::set_alice_volume(target))
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            {
                Ok(()) => format!("Громкость колонки теперь {target} из 10."),
                Err(e) => format!("Не удалось изменить громкость: {e}"),
            };
        }
        if name == "run_scenario"
            && let Some(eng) = self.engine()
        {
            let sc = field("scenario");
            if let Some(custom) = self.scenarios.as_ref().and_then(|st| st.find_by_query(&sc)) {
                let cname = custom.name.clone();
                crate::scenarios::ScenarioStore::execute(eng, custom);
                return format!("Запустил сценарий «{cname}».");
            }
            let sc_low = sc.to_lowercase();
            let mapped = match sc_low.as_str() {
                "утро" | "доброе утро" | "morning" => "morning",
                "работа" | "фокус" | "work" => "work",
                "вечер" | "уют" | "отдых" | "relax" | "evening" => "evening",
                "сон" | "спать" | "ночь" | "sleep" => "sleep",
                other => other,
            };
            return match run_scenario(eng, mapped) {
                Ok(msg) => msg,
                Err(e) => e,
            };
        }
        if name == "control_book"
            && let Some(reader) = &self.reader
            && let Some(eng) = self.engine()
        {
            let action = field("action");
            return match action.as_str() {
                "pause" => {
                    reader.stop();
                    "Чтение книги поставлено на паузу.".into()
                }
                "save_quote" => reader.save_quote(&eng).await.unwrap_or_else(|e| e),
                "sleep" => {
                    let mins = a.get("sleep_minutes").and_then(Value::as_u64).unwrap_or(20);
                    reader.set_sleep(mins);
                    if mins == 0 {
                        "Таймер сна книги отменён.".into()
                    } else {
                        format!("Поставил таймер сна книги на {mins} мин.")
                    }
                }
                _ => Box::pin(reader.recap(eng)).await.unwrap_or_else(|e| e),
            };
        }
        if name == "morning_briefing"
            && let Some(eng) = self.engine()
        {
            return Box::pin(crate::briefing::generate_briefing_text(&eng, ""))
                .await
                .unwrap_or_else(|e| format!("Не удалось собрать сводку: {e}"));
        }
        if name == "control_mode_or_game" {
            let action = field("action");
            return match action.as_str() {
                "stop" => {
                    if let Some(g) = &self.games {
                        g.stop_all(self.engine().as_deref());
                    }
                    if let Some(m) = &self.modes {
                        m.reset(self.ambient.as_deref());
                    }
                    "Режим остановлен, возвращаюсь к обычному общению.".into()
                }
                "chat" => self
                    .modes
                    .as_ref()
                    .map(|m| m.activate(crate::modes::SpecialMode::Chat, self.ambient.as_deref()))
                    .unwrap_or_else(|| "Режим диалога недоступен.".into()),
                "rpg" => self
                    .modes
                    .as_ref()
                    .map(|m| m.activate(crate::modes::SpecialMode::Rpg, self.ambient.as_deref()))
                    .unwrap_or_else(|| "Режим RPG недоступен.".into()),
                "quiz" => self
                    .modes
                    .as_ref()
                    .map(|m| m.activate(crate::modes::SpecialMode::Quiz, self.ambient.as_deref()))
                    .unwrap_or_else(|| "Викторина недоступна.".into()),
                "translator" => {
                    let lang = {
                        let l = field("language");
                        if l.is_empty() { "английский".into() } else { l }
                    };
                    self.modes
                        .as_ref()
                        .map(|m| m.activate(crate::modes::SpecialMode::Translator(lang), self.ambient.as_deref()))
                        .unwrap_or_else(|| "Переводчик недоступен.".into())
                }
                "melody" => {
                    if let (Some(g), Some(eng)) = (&self.games, self.engine()) {
                        let theme = field("theme");
                        match Box::pin(g.start_melody(eng, if theme.is_empty() { "популярные хиты".into() } else { theme })).await {
                            Ok(_) => "Запустил игру «Угадай мелодию»!".into(),
                            Err(e) => e,
                        }
                    } else {
                        "Игры недоступны.".into()
                    }
                }
                "detective" => {
                    if let (Some(g), Some(eng)) = (&self.games, self.engine()) {
                        let theme = field("theme");
                        match Box::pin(g.start_detective(eng, theme)).await {
                            Ok(_) => "Запустил «Нуарный детектив»!".into(),
                            Err(e) => e,
                        }
                    } else {
                        "Игры недоступны.".into()
                    }
                }
                _ => "Неизвестный режим.".into(),
            };
        }
        if name != "smart_home" {
            if let Some(res) = self.host.call_tool(name, args).await {
                log::info!("инструмент плагина {name} → {}", crate::plugins::clip(&res, 200));
                return res;
            }
            return "Нет такого инструмента.".into();
        }
        if !self.use_ch("yandex") {
            return "Управление через Яндекс Умный дом выключено.".into();
        }
        match self.yandex("control", args.into(), Duration::from_secs(15)).await {
            Ok(v) => as_str(&v),
            Err(e) => e,
        }
    }
}

const CMD_VERB_STOPWORDS: &[&str] = &[
    "джарвис",
    "пожалуйста",
    "конкретно",
    "включи",
    "выключи",
    "погаси",
    "отключи",
    "зажги",
    "запусти",
    "останови",
    "сделай",
    "поставь",
    "убавь",
    "прибавь",
    "увеличь",
    "уменьши",
    "открой",
    "закрой",
    "отправь",
    "верни",
];

fn device_matches_command(text: &str, device: &Value) -> bool {
    let norm_cmd = fastcmd::norm_text(text);
    if norm_cmd.is_empty() {
        return false;
    }
    let name_raw = device["name"]
        .as_str()
        .or_else(|| device["attributes"]["friendly_name"].as_str())
        .unwrap_or("");
    let name = fastcmd::norm_text(name_raw);
    let kind = device["type"].as_str().unwrap_or("").to_lowercase();
    let model = device["model"].as_str().unwrap_or("").to_lowercase();
    let domain = device["domain"]
        .as_str()
        .or_else(|| device["entity_id"].as_str().and_then(|id| id.split('.').next()))
        .unwrap_or("")
        .to_lowercase();
    let meta = format!("{} {} {} {}", name, kind, model, domain);

    const PAIRS: &[(&[&str], &[&str])] = &[
        (&["пылесос", "пылик", "робот"], &["vacuum"]),
        (&["свет", "ламп", "люстр", "лент", "торшер", "подсвет"], &["light", "lamp", "bulb", "ceiling", "strip"]),
        (&["розетк", "выключат"], &["outlet", "plug", "switch", "relay"]),
        (&["кондиц", "кондей", "кондер"], &["air-conditioner", "air_conditioner", "climate"]),
        (&["увлажнит"], &["humidifier"]),
        (&["очистит"], &["air-purifier", "purifier"]),
        (&["чайник"], &["kettle"]),
        (&["штор", "занавес"], &["curtain", "cover", "blind"]),
        (&["обогрев"], &["heater"]),
        (&["вентилятор"], &["fan"]),
        (&["телевизор"], &["television", "media_player"]),
    ];

    for (cmd_stems, dev_tokens) in PAIRS {
        if cmd_stems.iter().any(|stem| norm_cmd.contains(stem))
            && dev_tokens.iter().any(|tok| meta.contains(tok))
        {
            return true;
        }
    }

    if !name.is_empty() {
        let name_words: Vec<&str> = name.split_whitespace().collect();
        for cw in norm_cmd.split_whitespace() {
            if cw.chars().count() < 4 || CMD_VERB_STOPWORDS.contains(&cw) {
                continue;
            }
            if name_words.iter().any(|nw| {
                nw.chars().count() >= 4
                    && (*nw == cw || nw.starts_with(cw) || cw.starts_with(nw))
            }) {
                return true;
            }
        }
    }
    false
}

pub async fn capture_and_describe_screen(host: &Host, store: &Store, question: &str) -> Result<String, String> {
    let s = store.get();
    let url = s.pc_agent_url.trim().trim_end_matches('/').to_string();
    let tok = s.pc_agent_token.trim().to_string();
    if url.is_empty() {
        return Err("URL ПК-компаньона не указан в разделе «Колонка»".into());
    }
    let resp = crate::net::direct()
        .get(format!("{url}/screenshot?max_width=1280&quality=75"))
        .bearer_auth(&tok)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("ПК-компаньон не отвечает: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("ПК-компаньон вернул HTTP {}", resp.status()));
    }
    let jpg = resp.bytes().await.map_err(|e| e.to_string())?;
    if jpg.len() < 100 {
        return Err("ПК-компаньон прислал пустой скриншот".into());
    }
    let p = s.active_profile();
    let voice_srv = local_voice_url(host);
    crate::llm::ask_vision(host, &s, &p, &voice_srv, question, &jpg).await
}

fn pc_config(store: &Store) -> Result<(String, String), String> {
    let s = store.get();
    let url = s.pc_agent_url.trim().trim_end_matches('/').to_string();
    let tok = s.pc_agent_token.trim().to_string();
    if url.is_empty() {
        return Err("ПК-компаньон не настроен".into());
    }
    Ok((url, tok))
}

pub(crate) fn valid_http_url(raw: &str) -> Option<String> {
    let url = raw.trim();
    if url.is_empty() || url.len() > 2000 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        return None;
    }
    Some(url.to_string())
}

pub fn format_pc_system_summary(v: &Value) -> String {
    let host = v["host"].as_str().unwrap_or("ПК");
    let os = v["os"].as_str().unwrap_or("");
    let ram = match (v["ram_used_mb"].as_u64(), v["ram_total_mb"].as_u64()) {
        (Some(u), Some(t)) if t > 0 => format!(", RAM {u}/{t} МБ"),
        _ => String::new(),
    };
    let win = v["active_window"].as_str().unwrap_or("").trim();
    let win_part = if win.is_empty() { String::new() } else { format!(", {win}") };
    format!("{host} ({os}){ram}{win_part}")
}

pub async fn call_pc_action(store: &Store, action: &str, minutes: i64) -> Result<String, String> {
    let (url, tok) = pc_config(store)?;
    let resp = crate::net::direct()
        .post(format!("{url}/action"))
        .bearer_auth(&tok)
        .json(&json!({"action": action, "minutes": minutes}))
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| format!("ПК-компаньон не отвечает: {e}"))?;
    let status = resp.status();
    let data: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(data["error"].as_str().unwrap_or("Ошибка команды ПК").to_string());
    }
    Ok(data["message"].as_str().unwrap_or("Готово.").to_string())
}

pub async fn pc_clipboard_read(store: &Store) -> Result<String, String> {
    let (url, tok) = pc_config(store)?;
    let resp = crate::net::direct()
        .get(format!("{url}/clipboard"))
        .bearer_auth(&tok)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("ПК-компаньон не отвечает: {e}"))?;
    let status = resp.status();
    let data: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(data["error"].as_str().unwrap_or("Не удалось прочитать буфер ПК").to_string());
    }
    Ok(data["text"].as_str().unwrap_or("").to_string())
}

pub async fn pc_clipboard_write(store: &Store, text: &str) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 16000 {
        return Err("Текст буфера должен быть от 1 до 16000 байт".into());
    }
    let (url, tok) = pc_config(store)?;
    let resp = crate::net::direct()
        .post(format!("{url}/clipboard"))
        .bearer_auth(&tok)
        .json(&json!({"text": trimmed}))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("ПК-компаньон не отвечает: {e}"))?;
    let status = resp.status();
    let data: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(data["error"].as_str().unwrap_or("Не удалось записать в буфер ПК").to_string());
    }
    Ok(data["message"].as_str().unwrap_or("Скопировано в буфер ПК.").to_string())
}

pub async fn pc_open_url(store: &Store, raw_url: &str) -> Result<String, String> {
    let clean = valid_http_url(raw_url).ok_or_else(|| "Укажите корректную ссылку http:// или https://".to_string())?;
    let (url, tok) = pc_config(store)?;
    let resp = crate::net::direct()
        .post(format!("{url}/open-url"))
        .bearer_auth(&tok)
        .json(&json!({"url": clean}))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("ПК-компаньон не отвечает: {e}"))?;
    let status = resp.status();
    let data: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(data["error"].as_str().unwrap_or("Не удалось открыть ссылку на ПК").to_string());
    }
    Ok(data["message"].as_str().unwrap_or("Ссылка открыта на ПК.").to_string())
}

pub async fn pc_macros_list(store: &Store) -> Result<Value, String> {
    let (url, tok) = pc_config(store)?;
    let resp = crate::net::direct()
        .get(format!("{url}/macros"))
        .bearer_auth(&tok)
        .timeout(Duration::from_secs(4))
        .send()
        .await
        .map_err(|e| format!("ПК-компаньон не отвечает: {e}"))?;
    let status = resp.status();
    let data: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(data["error"].as_str().unwrap_or("Не удалось получить список макросов").to_string());
    }
    Ok(data)
}

pub async fn pc_macro_run(store: &Store, id: &str) -> Result<String, String> {
    let mid = id.trim();
    if mid.is_empty() || mid.len() > 40 {
        return Err("Неверный ID макроса".into());
    }
    let (url, tok) = pc_config(store)?;
    let resp = crate::net::direct()
        .post(format!("{url}/macro"))
        .bearer_auth(&tok)
        .json(&json!({"id": mid}))
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .map_err(|e| format!("ПК-компаньон не отвечает: {e}"))?;
    let status = resp.status();
    let data: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(data["error"].as_str().unwrap_or("Не удалось запустить макрос").to_string());
    }
    Ok(data["message"].as_str().unwrap_or("Макрос запущен.").to_string())
}

pub async fn pc_system_info(store: &Store) -> Result<Value, String> {
    let (url, tok) = pc_config(store)?;
    let resp = crate::net::direct()
        .get(format!("{url}/system"))
        .bearer_auth(&tok)
        .timeout(Duration::from_secs(4))
        .send()
        .await
        .map_err(|e| format!("ПК-компаньон не отвечает: {e}"))?;
    let status = resp.status();
    let data: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(data["error"].as_str().unwrap_or("Не удалось получить системную сводку ПК").to_string());
    }
    Ok(data)
}

pub async fn save_cybernote(host: &Host, title: &str, content: &str, tags: &[String], folder: &str) -> Result<String, String> {
    let srv = local_voice_url(host);
    let resp = crate::net::direct()
        .post(format!("{srv}/notes/save"))
        .json(&json!({"title": title, "content": content, "tags": tags, "folder": folder}))
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| format!("Сервер заметок недоступен: {e}"))?;
    let status = resp.status();
    let v: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(v["detail"].as_str().unwrap_or("Ошибка записи заметки").to_string());
    }
    Ok(v["path"].as_str().unwrap_or(title).to_string())
}

pub async fn semantic_search_all(
    host: &Host,
    memory: Option<&crate::memory::Memory>,
    query: &str,
    limit: usize,
) -> Result<Value, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Пустой запрос".into());
    }
    let extra_docs = memory.map(|m| m.semantic_docs(45)).unwrap_or_default();
    let srv = local_voice_url(host);
    let resp = crate::net::direct()
        .post(format!("{srv}/notes/semantic"))
        .json(&json!({
            "query": q,
            "limit": limit.clamp(1, 24),
            "extra_docs": extra_docs,
        }))
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| format!("Сервер поиска недоступен: {e}"))?;
    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(v["detail"].as_str().unwrap_or("Ошибка семантического поиска").to_string());
    }
    Ok(v)
}

pub async fn search_cybernotes(host: &Host, query: &str) -> Result<String, String> {
    let srv = local_voice_url(host);
    let resp = crate::net::direct()
        .get(format!("{srv}/notes/search"))
        .query(&[("q", query), ("limit", "5")])
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| format!("Сервер заметок недоступен: {e}"))?;
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    let hits = v["hits"].as_array().cloned().unwrap_or_default();
    if hits.is_empty() {
        return Ok("По этому запросу в заметках CyberNotes ничего не нашлось.".into());
    }
    let lines: Vec<String> = hits
        .iter()
        .map(|h| {
            format!(
                "• «{}» ({}): {}",
                h["title"].as_str().unwrap_or(""),
                h["path"].as_str().unwrap_or(""),
                h["snippet"].as_str().unwrap_or("")
            )
        })
        .collect();
    Ok(format!("Найдено в CyberNotes:\n{}", lines.join("\n")))
}

pub fn parse_scenario_command(text: &str) -> Option<&'static str> {
    let low = text.trim().to_lowercase();
    if ["режим работы", "сценарий работы", "рабочий режим"].iter().any(|p| low.contains(p)) {
        return Some("work");
    }
    if ["вечерний режим", "режим вечера", "сценарий вечера"].iter().any(|p| low.contains(p)) {
        return Some("evening");
    }
    if ["режим сна", "сценарий сна", "ночной сценарий"].iter().any(|p| low.contains(p)) {
        return Some("sleep");
    }
    if ["утренний режим", "режим утра", "сценарий утра"].iter().any(|p| low.contains(p)) {
        return Some("morning");
    }
    None
}

pub fn is_book_recap_command(text: &str) -> bool {
    let low = text.trim().to_lowercase();
    [
        "на чём остановились",
        "на чем остановились",
        "перескажи книгу",
        "напомни книгу",
        "что было в книге",
        "суть книги",
    ]
    .iter()
    .any(|p| low.contains(p))
}

pub fn is_book_quote_command(text: &str) -> bool {
    let low = text.trim().to_lowercase();
    ["сохрани цитату", "запиши цитату", "цитату в заметки"]
        .iter()
        .any(|p| low.contains(p))
}

pub fn run_scenario(engine: Arc<crate::engine::Engine>, id: &str) -> Result<String, String> {
    match id.trim().to_lowercase().as_str() {
        "work" => {
            engine.tools.stop_for(&engine, "focus");
            std::thread::spawn(|| {
                let _ = crate::speakervol::set_alice_volume(4);
            });
            if let Some(amb) = &engine.tools.ambient {
                let _ = amb.set_volume(35);
                if amb.start("lofi").is_err() {
                    let _ = amb.start("rain");
                }
            }
            if let Some(focus) = &engine.tools.focus {
                focus.start(engine.clone(), 25);
            }
            engine.journal.note("сценарий", "Режим работы", "Фокус 25м + Lo-Fi");
            Ok("Режим работы: фокус 25 минут и Lo-Fi фон.".into())
        }
        "evening" | "relax" => {
            engine.tools.stop_for(&engine, "dj");
            std::thread::spawn(|| {
                let _ = crate::speakervol::set_alice_volume(5);
            });
            if engine.host.provider("youtube").is_ok()
                && let Some(dj) = &engine.tools.dj
            {
                let _ = dj.start(engine.clone(), "ночной Токио чилл синтвейв".into());
            } else if let Some(amb) = &engine.tools.ambient
                && amb.start("jazz").is_err()
            {
                let _ = amb.start("fireplace");
            }
            engine.journal.note("сценарий", "Вечерний режим", "Атмосферный вайб");
            Ok("Вечерний режим запущен.".into())
        }
        "sleep" => {
            engine.tools.stop_all_activities(Some(&engine), true);
            std::thread::spawn(|| {
                let _ = crate::speakervol::set_alice_volume(2);
            });
            if let Some(amb) = &engine.tools.ambient {
                let _ = amb.set_volume(35);
                let _ = amb.start("ocean");
                amb.set_sleep(20);
            }
            engine.journal.note("сценарий", "Режим сна", "Океан 20 мин");
            Ok("Режим сна: тихий океан и таймер на 20 минут.".into())
        }
        "morning" => {
            engine.tools.stop_all_activities(Some(&engine), true);
            std::thread::spawn(|| {
                let _ = crate::speakervol::set_alice_volume(5);
            });
            if let Some(amb) = &engine.tools.ambient {
                let _ = amb.set_volume(40);
                let _ = amb.start("jazz");
            }
            engine.journal.note("сценарий", "Утренний режим", "Джаз + громкость 5");
            Ok("Доброе утро! Утренний режим включён.".into())
        }
        _ => {
            if let Some(sc_store) = &engine.tools.scenarios
                && let Some(sc) = sc_store.find_by_query(id)
            {
                let name = sc.name.clone();
                crate::scenarios::ScenarioStore::execute(engine.clone(), sc);
                return Ok(format!("Сценарий «{name}» запущен."));
            }
            Err("Неизвестный сценарий".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defs_load() {
        for n in ["home_assistant", "xiaomi_home", "smart_life", "alice_command", "smart_home", "recall_conversation", "edit_memory"] {
            assert_eq!(super::def(n)["name"], n, "{n}");
        }
    }

    #[test]
    fn device_matches_command_routes_xiaomi_and_ha_away_from_alice() {
        let vacuum = json!({
            "name": "Mijia Robot Vacuum 5",
            "room": "",
            "type": "urn:miot-spec-v2:device:vacuum:0000A006:xiaomi-ov31:1",
            "model": "xiaomi.vacuum.ov31"
        });
        assert!(device_matches_command("включи пылесос в спальне конкретно в спальне", &vacuum));
        assert!(!device_matches_command("выключи свет на кухне", &vacuum));

        let ha_light = json!({
            "entity_id": "light.kitchen_ceiling",
            "attributes": {"friendly_name": "Люстра кухня"}
        });
        assert!(device_matches_command("выключи свет на кухне", &ha_light));
        assert!(!device_matches_command("включи кондиционер", &ha_light));
    }

    #[test]
    fn parse_scenario_command_matches() {
        assert_eq!(parse_scenario_command("Джарвис, включи режим работы"), Some("work"));
        assert_eq!(parse_scenario_command("вечерний режим пожалуйста"), Some("evening"));
        assert_eq!(parse_scenario_command("запусти сценарий сна"), Some("sleep"));
        assert_eq!(parse_scenario_command("утренний режим"), Some("morning"));
        assert_eq!(parse_scenario_command("какая погода"), None);
    }

    #[test]
    fn book_recap_command_matches() {
        assert!(is_book_recap_command("на чём остановились в книге?"));
        assert!(is_book_recap_command("перескажи книгу пожалуйста"));
        assert!(is_book_recap_command("напомни книгу"));
        assert!(!is_book_recap_command("включи музыку"));
        assert!(is_book_quote_command("сохрани цитату в заметки"));
        assert!(!is_book_quote_command("включи трек"));
    }

    #[test]
    fn validates_pc_http_urls() {
        assert_eq!(valid_http_url("https://youtu.be/dQw4w9WgXcQ"), Some("https://youtu.be/dQw4w9WgXcQ".into()));
        assert_eq!(valid_http_url("  http://example.com/a?b=1  "), Some("http://example.com/a?b=1".into()));
        assert_eq!(valid_http_url("javascript:alert(1)"), None);
        assert_eq!(valid_http_url("file:///etc/passwd"), None);
        assert_eq!(valid_http_url("https://"), None);
        assert_eq!(valid_http_url("https://bad host.com"), None);
    }
}
