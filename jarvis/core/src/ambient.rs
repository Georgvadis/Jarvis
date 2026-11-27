//! Фоновые звуки (офлайн синтез прямо на колонке) и интернет-радио (через ffmpeg-поток jarvis-voice).

use crate::{
    plugins::Host,
    settings::{CustomRadio, Store},
};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{SyncSender, TrySendError, sync_channel},
    },
    time::{Duration, Instant},
};
use tokio::sync::Notify;

#[derive(Clone, Copy)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str, // "ambient" | "radio"
    pub url: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset { id: "rain", name: "Дождь", kind: "ambient", url: "" },
    Preset { id: "fireplace", name: "Камин", kind: "ambient", url: "" },
    Preset { id: "ocean", name: "Океан", kind: "ambient", url: "" },
    Preset { id: "white", name: "Белый шум", kind: "ambient", url: "" },
    Preset { id: "lofi", name: "Lo-Fi Radio", kind: "radio", url: "https://ice1.somafm.com/groovesalad-128-mp3" },
    Preset { id: "jazz", name: "Jazz Radio", kind: "radio", url: "https://ice1.somafm.com/sonicuniverse-128-mp3" },
    Preset { id: "chill", name: "Space Chill", kind: "radio", url: "https://ice1.somafm.com/dronezone-128-mp3" },
];

struct Active {
    id: String,
    name: String,
    stop: Arc<AtomicBool>,
    stop_notify: Arc<Notify>,
    tx: Mutex<Option<SyncSender<Vec<u8>>>>,
}

pub struct Ambient {
    plugins: Arc<Host>,
    store: Store,
    active: Arc<Mutex<Option<Active>>>,
    duck: Arc<AtomicBool>,
    sleep_until: Arc<Mutex<Option<Instant>>>,
}

fn stop_aplay(ch: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(ch.id() as i32, libc::SIGINT);
    }
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(600) {
        if matches!(ch.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = ch.kill();
    let _ = ch.wait();
}

fn slugify_station(name: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for ch in name.trim().to_lowercase().chars() {
        let tr = match ch {
            'а' => "a", 'б' => "b", 'в' => "v", 'г' => "g", 'д' => "d", 'е' | 'ё' => "e",
            'ж' => "zh", 'з' => "z", 'и' => "i", 'й' => "y", 'к' => "k", 'л' => "l",
            'м' => "m", 'н' => "n", 'о' => "o", 'п' => "p", 'р' => "r", 'с' => "s",
            'т' => "t", 'у' => "u", 'ф' => "f", 'х' => "h", 'ц' => "ts", 'ч' => "ch",
            'ш' => "sh", 'щ' => "sch", 'ъ' | 'ь' => "", 'ы' => "y", 'э' => "e",
            'ю' => "yu", 'я' => "ya",
            c if c.is_ascii_alphanumeric() => {
                out.push(c);
                prev_sep = false;
                continue;
            }
            _ => {
                if !prev_sep && !out.is_empty() {
                    out.push('_');
                    prev_sep = true;
                }
                continue;
            }
        };
        if !tr.is_empty() {
            out.push_str(tr);
            prev_sep = false;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        format!(
            "st_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        )
    } else {
        trimmed.chars().take(32).collect()
    }
}

impl Ambient {
    pub fn new(plugins: Arc<Host>, store: Store) -> Arc<Self> {
        Arc::new(Self {
            plugins,
            store,
            active: Arc::new(Mutex::new(None)),
            duck: Arc::new(AtomicBool::new(false)),
            sleep_until: Arc::new(Mutex::new(None)),
        })
    }

    pub fn set_duck(&self, on: bool) {
        self.duck.store(on, Ordering::Relaxed);
    }

    pub fn volume_f32(&self) -> f32 {
        (self.store.with(|s| s.ambient_volume).clamp(5, 100) as f32) / 100.0
    }

    pub fn set_volume(&self, vol: i64) -> Value {
        let clamped = vol.clamp(5, 100);
        let _ = self.store.update(|s| {
            s.ambient_volume = clamped;
            Ok::<(), std::io::Error>(())
        });
        self.status()
    }

    pub fn add_station(&self, name: &str, url: &str) -> Result<Value, String> {
        let name = name.trim();
        let url = url.trim();
        if name.is_empty() || name.chars().count() > 40 {
            return Err("Название станции — от 1 до 40 символов".into());
        }
        if !(url.starts_with("http://") || url.starts_with("https://"))
            || url.len() > 300
            || url.chars().any(char::is_whitespace)
        {
            return Err("Укажите корректный http(s) URL потока (до 300 символов)".into());
        }
        let base_slug = slugify_station(name);
        self.store
            .update(|s| {
                if s.custom_radios.len() >= 12 {
                    return Err(std::io::Error::other("Максимум 12 своих станций"));
                }
                let mut id = format!("custom_{base_slug}");
                let mut idx = 2usize;
                while s.custom_radios.iter().any(|r| r.id == id) {
                    id = format!("custom_{base_slug}_{idx}");
                    idx += 1;
                }
                s.custom_radios.push(CustomRadio {
                    id,
                    name: name.to_string(),
                    url: url.to_string(),
                });
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(self.status())
    }

    pub fn remove_station(&self, id: &str) -> Result<Value, String> {
        let key = id.trim();
        if self.current_id().as_deref() == Some(key) {
            self.stop();
        }
        let mut removed = false;
        self.store
            .update(|s| {
                let before = s.custom_radios.len();
                s.custom_radios.retain(|r| r.id != key);
                removed = s.custom_radios.len() != before;
                Ok::<(), std::io::Error>(())
            })
            .map_err(|e| e.to_string())?;
        if !removed {
            return Err("Станция не найдена".into());
        }
        Ok(self.status())
    }

    pub fn set_sleep(&self, minutes: u64) -> String {
        let mut slot = self.sleep_until.lock().unwrap();
        if minutes == 0 {
            *slot = None;
            return "Таймер сна отменён.".into();
        }
        let mins = minutes.min(480);
        *slot = Some(Instant::now() + Duration::from_secs(mins * 60));
        format!("Выключу через {mins} мин.")
    }

    pub fn sleep_remaining_s(&self) -> u64 {
        let slot = self.sleep_until.lock().unwrap();
        match *slot {
            Some(deadline) => deadline.saturating_duration_since(Instant::now()).as_secs(),
            None => 0,
        }
    }

    pub fn current_id(&self) -> Option<String> {
        self.active.lock().unwrap().as_ref().map(|a| a.id.clone())
    }

    pub fn current_label(&self) -> Option<String> {
        self.active.lock().unwrap().as_ref().map(|a| a.name.clone())
    }

    pub fn status(&self) -> Value {
        let cur = self.current_id().unwrap_or_default();
        let sleep_s = if cur.is_empty() { 0 } else { self.sleep_remaining_s() };
        let (vol, customs) = self.store.with(|s| (s.ambient_volume.clamp(5, 100), s.custom_radios.clone()));
        let mut items: Vec<Value> = PRESETS
            .iter()
            .map(|p| json!({"id": p.id, "name": p.name, "kind": p.kind, "custom": false}))
            .collect();
        for r in customs {
            items.push(json!({"id": r.id, "name": r.name, "kind": "radio", "custom": true, "url": r.url}));
        }
        json!({"active": cur, "sleep_s": sleep_s, "volume": vol, "presets": items})
    }

    pub fn stop(&self) {
        *self.sleep_until.lock().unwrap() = None;
        let prev = self.active.lock().unwrap().take();
        if let Some(a) = prev {
            a.stop.store(true, Ordering::SeqCst);
            a.stop_notify.notify_waiters();
            a.tx.lock().unwrap().take();
            log::info!("фон/радио: остановлено ({})", a.name);
        }
    }

    pub fn start(&self, id: &str) -> Result<String, String> {
        let key = id.trim().to_lowercase();
        let target: (String, String, &'static str, String) =
            if let Some(p) = PRESETS.iter().find(|p| p.id == key) {
                (p.id.into(), p.name.into(), p.kind, p.url.into())
            } else if let Some(c) = self
                .store
                .with(|s| s.custom_radios.iter().find(|r| r.id.to_lowercase() == key).cloned())
            {
                (c.id, c.name, "radio", c.url)
            } else {
                return Err(format!("Неизвестный пресет: {id}"));
            };
        self.stop();

        let stop = Arc::new(AtomicBool::new(false));
        let stop_notify = Arc::new(Notify::new());
        let aplay = if std::path::Path::new("/usr/bin/aplay").exists() {
            "/usr/bin/aplay"
        } else {
            "aplay"
        };
        let mut child = Command::new(aplay)
            .args(["-q", "-D", "default", "-t", "raw", "-f", "S16_LE", "-r", "48000", "-c", "1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Не удалось запустить aplay: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("aplay stdin недоступен")?;
        let (tx, rx) = sync_channel::<Vec<u8>>(16);

        let stop_out = stop.clone();
        std::thread::Builder::new()
            .name(format!("ambient-out-{}", target.0))
            .spawn(move || {
                while !stop_out.load(Ordering::Relaxed) {
                    match rx.recv_timeout(Duration::from_millis(80)) {
                        Ok(chunk) => {
                            if stdin.write_all(&chunk).is_err() {
                                break;
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                drop(stdin);
                stop_aplay(&mut child);
            })
            .map_err(|e| e.to_string())?;

        *self.active.lock().unwrap() = Some(Active {
            id: target.0.clone(),
            name: target.1.clone(),
            stop: stop.clone(),
            stop_notify: stop_notify.clone(),
            tx: Mutex::new(Some(tx.clone())),
        });

        let stop_bg = stop.clone();
        let notify_bg = stop_notify.clone();
        let duck_bg = self.duck.clone();
        let sleep_bg = self.sleep_until.clone();
        let active_bg = self.active.clone();
        let store_bg = self.store.clone();
        let server_url = crate::tools::local_voice_url(&self.plugins);
        let (preset_id, preset_name, preset_kind, preset_url) = target;
        let ret_name = preset_name.clone();

        let cleanup = move |active_slot: &Mutex<Option<Active>>,
                            sleep_slot: &Mutex<Option<Instant>>,
                            stop_ref: &Arc<AtomicBool>| {
            let mut g = active_slot.lock().unwrap();
            if g.as_ref().is_some_and(|a| Arc::ptr_eq(&a.stop, stop_ref)) {
                g.take();
                *sleep_slot.lock().unwrap() = None;
            }
        };

        if preset_kind == "ambient" {
            tokio::spawn(async move {
                log::info!("фон/радио: запущено «{preset_name}»");
                let hi_fi_url = format!("{server_url}/ambient/{preset_id}");
                let ok = run_pcm48_stream_async(
                    &hi_fi_url,
                    tx.clone(),
                    &stop_bg,
                    &notify_bg,
                    &duck_bg,
                    &sleep_bg,
                    &store_bg,
                    false,
                )
                .await;
                if !ok && !stop_bg.load(Ordering::Relaxed) && !sleep_expired(&sleep_bg) {
                    log::info!("фон: сервер недоступен для «{preset_name}», включаю офлайн-синтез");
                    let stop_f = stop_bg.clone();
                    let duck_f = duck_bg.clone();
                    let sleep_f = sleep_bg.clone();
                    let store_f = store_bg.clone();
                    let pid = preset_id.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        run_procedural(&pid, tx, &stop_f, &duck_f, &sleep_f, &store_f);
                    })
                    .await;
                }
                stop_bg.store(true, Ordering::SeqCst);
                cleanup(&active_bg, &sleep_bg, &stop_bg);
            });
        } else {
            tokio::spawn(async move {
                log::info!("фон/радио: запущено «{preset_name}»");
                let encoded: String = preset_url
                    .bytes()
                    .map(|b| match b {
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                        _ => format!("%{:02X}", b),
                    })
                    .collect();
                let url = format!("{server_url}/stream?url={encoded}&rate=48000");
                let _ = run_pcm48_stream_async(&url, tx, &stop_bg, &notify_bg, &duck_bg, &sleep_bg, &store_bg, true).await;
                stop_bg.store(true, Ordering::SeqCst);
                cleanup(&active_bg, &sleep_bg, &stop_bg);
            });
        }

        Ok(ret_name)
    }

    /// Быстрые голосовые команды без вызова LLM.
    pub fn fast_command(&self, text: &str) -> Option<String> {
        let t = text.trim().to_lowercase();
        if t.contains("песн")
            || t.contains("трек")
            || t.contains("клип")
            || t.contains("альбом")
            || t.contains("групп")
            || t.contains("музык")
            || t.contains("свет")
            || t.contains("ламп")
            || t.contains("люстр")
            || t.contains("погод")
            || t.contains("зонт")
        {
            return None;
        }
        let is_ambient_topic = t.contains("дожд")
            || t.contains("камин")
            || t.contains("костёр")
            || t.contains("костер")
            || t.contains("океан")
            || t.contains("прибо")
            || t.contains("белый шум")
            || t.contains("шум для сна")
            || t.contains("фон")
            || t.contains("радио")
            || t.contains("лоуфай")
            || t.contains("lo-fi")
            || t.contains("lofi")
            || t.contains("лофай")
            || t.contains("джаз")
            || t.contains("jazz")
            || t.contains("чилл")
            || t.contains("chill")
            || t.contains("таймер сна");
        // Таймер сна: «выключи дождь через 30 минут» / «таймер сна на 20 минут»
        if (is_ambient_topic || self.current_id().is_some())
            && (t.contains("через ") || t.contains("таймер сна"))
            && let Some(mins) = parse_minutes(&t)
        {
            return Some(self.set_sleep(mins));
        }
        let wants_off = t.contains("выключи") || t.contains("останови") || t.contains("хватит") || t.contains("убери");
        if wants_off && is_ambient_topic {
            if self.current_id().is_some() {
                self.stop();
                return Some("Выключил.".into());
            }
            return None;
        }
        let wants_on = t.contains("включи") || t.contains("поставь") || t.contains("запусти");
        if !wants_on {
            return None;
        }
        let customs = self.store.with(|s| s.custom_radios.clone());
        for c in customs {
            if t.contains(&c.name.to_lowercase()) {
                return match self.start(&c.id) {
                    Ok(name) => Some(format!("Включил {name}.")),
                    Err(e) => Some(format!("Не получилось включить: {e}")),
                };
            }
        }
        let id = if t.contains("дожд") {
            "rain"
        } else if t.contains("камин") || t.contains("костёр") || t.contains("костер") {
            "fireplace"
        } else if t.contains("океан") || t.contains("прибо") {
            "ocean"
        } else if t.contains("белый шум") || t.contains("шум для сна") {
            "white"
        } else if t.contains("лоуфай") || t.contains("lo-fi") || t.contains("lofi") || t.contains("лофай") {
            "lofi"
        } else if t.contains("джаз") || t.contains("jazz") {
            "jazz"
        } else if t.contains("чилл") || t.contains("chill") || t.contains("фоновое радио") || t.contains("радио для работы") {
            "chill"
        } else {
            return None;
        };
        match self.start(id) {
            Ok(name) => Some(format!("Включил {name}.")),
            Err(e) => Some(format!("Не получилось включить: {e}")),
        }
    }
}

/// Простой быстрый генератор псевдослучайных чисел (xorshift64) в [-1.0, 1.0].
struct Rng(u64);
impl Rng {
    fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        ((x >> 40) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0
    }
}

fn parse_minutes(text: &str) -> Option<u64> {
    let mut num: Option<u64> = None;
    for tok in text.split_whitespace() {
        let digits: String = tok.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<u64>() && n > 0 {
            num = Some(n);
            break;
        }
    }
    let n = num.or_else(|| if text.contains("полчаса") { Some(30) } else if text.contains("час") { Some(1) } else { None })?;
    if text.contains("час") && !text.contains("полчаса") {
        Some(n * 60)
    } else {
        Some(n)
    }
}

fn sleep_expired(sleep_until: &Mutex<Option<Instant>>) -> bool {
    let slot = sleep_until.lock().unwrap();
    slot.is_some_and(|d| Instant::now() >= d)
}

fn run_procedural(
    kind: &str,
    tx: SyncSender<Vec<u8>>,
    stop: &AtomicBool,
    duck: &AtomicBool,
    sleep_until: &Mutex<Option<Instant>>,
    store: &Store,
) {
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let mut b0 = 0.0_f32;
    let mut b1 = 0.0_f32;
    let mut b2 = 0.0_f32;
    let mut lp1 = 0.0_f32;
    let mut lp2 = 0.0_f32;
    let mut hp_prev = 0.0_f32;
    let mut crackle_env = 0.0_f32;
    let mut drop_env = 0.0_f32;
    let mut drop_freq = 1200.0_f32;
    let mut drop_phase = 0.0_f32;
    let mut phase = 0.0_f32;
    let mut gain = (store.with(|s| s.ambient_volume).clamp(5, 100) as f32) / 100.0;

    while !stop.load(Ordering::Relaxed) && !sleep_expired(sleep_until) {
        let vol = (store.with(|s| s.ambient_volume).clamp(5, 100) as f32) / 100.0;
        let target_gain = if duck.load(Ordering::Relaxed) { (vol * 0.18).min(0.14) } else { vol };
        let mut buf = vec![0u8; 4800 * 2]; // 100ms chunks at 48kHz (ALSA dmix rate)
        for i in 0..4800 {
            gain += (target_gain - gain) * 0.0012;
            let w = rng.next_f32();
            b0 = 0.9985 * b0 + w * 0.022;
            b1 = 0.992 * b1 + w * 0.026;
            b2 = 0.975 * b2 + w * 0.038;
            let pink = b0 + b1 + b2 + w * 0.010;

            let raw = match kind {
                "rain" => {
                    lp1 += 0.22 * (pink - lp1);
                    let hp = lp1 - hp_prev;
                    hp_prev += 0.025 * (lp1 - hp_prev);
                    if rng.next_f32() > 0.99965 {
                        drop_env = 0.28;
                        drop_freq = 900.0 + (rng.next_f32() + 1.0) * 650.0;
                    }
                    drop_env *= 0.992;
                    drop_phase += std::f32::consts::TAU * drop_freq / 48000.0;
                    if drop_phase > std::f32::consts::TAU { drop_phase -= std::f32::consts::TAU; }
                    let drop = drop_phase.sin() * drop_env;
                    hp * 1.6 + drop * 0.45
                }
                "fireplace" => {
                    lp1 += 0.018 * (pink - lp1);
                    lp2 += 0.018 * (lp1 - lp2);
                    if rng.next_f32() > 0.99975 {
                        crackle_env = 0.55 + (rng.next_f32() + 1.0) * 0.25;
                    }
                    crackle_env *= 0.965;
                    let crackle = (w - hp_prev) * crackle_env * 0.7;
                    hp_prev = w * 0.6 + hp_prev * 0.4;
                    lp2 * 4.2 + crackle
                }
                "ocean" => {
                    phase += 1.0 / (48000.0 * 10.0);
                    if phase > 1.0 { phase -= 1.0; }
                    let wave = 0.12 + 0.88 * (0.5 - 0.5 * (phase * std::f32::consts::TAU).cos()).powf(1.6);
                    let alpha = 0.015 + 0.14 * wave;
                    lp1 += alpha * (pink - lp1);
                    lp2 += alpha * (lp1 - lp2);
                    lp2 * (0.6 + 2.4 * wave)
                }
                _ => {
                    lp1 += 0.10 * (pink - lp1);
                    lp2 += 0.10 * (lp1 - lp2);
                    lp2 * 2.2
                }
            };

            let soft = (raw * gain).tanh();
            let s = (soft * 16500.0) as i16;
            let bytes = s.to_le_bytes();
            buf[i * 2] = bytes[0];
            buf[i * 2 + 1] = bytes[1];
        }
        let mut pending = buf;
        loop {
            if stop.load(Ordering::Relaxed) || sleep_expired(sleep_until) {
                return;
            }
            match tx.try_send(pending) {
                Ok(()) => break,
                Err(TrySendError::Full(b)) => {
                    pending = b;
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(TrySendError::Disconnected(_)) => return,
            }
        }
    }
}

async fn wait_or_stop(dur: Duration, stop: &AtomicBool, notify: &Notify, sleep_until: &Mutex<Option<Instant>>) -> bool {
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        if stop.load(Ordering::Relaxed) || sleep_expired(sleep_until) {
            return false;
        }
        let rem = deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(200));
        tokio::select! {
            _ = notify.notified() => return false,
            _ = tokio::time::sleep(rem) => {}
        }
    }
    !stop.load(Ordering::Relaxed) && !sleep_expired(sleep_until)
}

/// Принимает нативный 48 кГц mono s16le поток (студийный эмбиент или радио) и плавно регулирует громкость/ducking.
/// Возвращает true, если удалось подключиться и проиграть поток (false — сервер недоступен, можно перейти на фолбэк).
#[allow(clippy::too_many_arguments)]
async fn run_pcm48_stream_async(
    url: &str,
    tx: SyncSender<Vec<u8>>,
    stop: &AtomicBool,
    notify: &Notify,
    duck: &AtomicBool,
    sleep_until: &Mutex<Option<Instant>>,
    store: &Store,
    retry_initial: bool,
) -> bool {
    let client = match reqwest::Client::builder().connect_timeout(Duration::from_secs(6)).build() {
        Ok(c) => c,
        Err(_) => return false,
    };

    let mut gain = (store.with(|s| s.ambient_volume).clamp(5, 100) as f32) / 100.0;
    let mut backoff_s = 2u64;
    let mut ever_connected = false;

    while !stop.load(Ordering::Relaxed) && !sleep_expired(sleep_until) {
        let resp_res = tokio::select! {
            _ = notify.notified() => return ever_connected,
            r = client.get(url).send() => r,
        };
        let mut resp = match resp_res {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                log::warn!("фон/радио: сервер вернул {}", r.status());
                if !ever_connected && !retry_initial {
                    return false;
                }
                if !wait_or_stop(Duration::from_secs(backoff_s), stop, notify, sleep_until).await {
                    return ever_connected;
                }
                backoff_s = (backoff_s * 2).min(8);
                continue;
            }
            Err(e) => {
                log::warn!("фон/радио: ошибка подключения: {e}");
                if !ever_connected && !retry_initial {
                    return false;
                }
                if !wait_or_stop(Duration::from_secs(backoff_s), stop, notify, sleep_until).await {
                    return ever_connected;
                }
                backoff_s = (backoff_s * 2).min(8);
                continue;
            }
        };

        ever_connected = true;
        let connected_at = Instant::now();
        let mut carry: Option<u8> = None;

        while !stop.load(Ordering::Relaxed) && !sleep_expired(sleep_until) {
            let vol = (store.with(|s| s.ambient_volume).clamp(5, 100) as f32) / 100.0;
            let target_gain = if duck.load(Ordering::Relaxed) { (vol * 0.18).min(0.14) } else { vol };
            let chunk_res = tokio::select! {
                _ = notify.notified() => return true,
                c = tokio::time::timeout(Duration::from_secs(15), resp.chunk()) => match c {
                    Ok(res) => res,
                    Err(_) => {
                        log::warn!("фон/радио: таймаут чтения потока — переподключаюсь");
                        break;
                    }
                },
            };
            match chunk_res {
                Ok(Some(chunk)) => {
                    if connected_at.elapsed() > Duration::from_secs(10) {
                        backoff_s = 2;
                    }
                    let mut bytes = Vec::with_capacity(chunk.len() + 1);
                    if let Some(b) = carry.take() {
                        bytes.push(b);
                    }
                    bytes.extend_from_slice(&chunk);
                    if bytes.len() % 2 == 1 {
                        carry = bytes.pop();
                    }
                    let mut scaled = Vec::with_capacity(bytes.len());
                    for pair in bytes.chunks_exact(2) {
                        gain += (target_gain - gain) * 0.001;
                        let s = i16::from_le_bytes([pair[0], pair[1]]) as f32 * gain;
                        scaled.extend_from_slice(&(s.clamp(-32000.0, 32000.0) as i16).to_le_bytes());
                    }
                    if !scaled.is_empty() {
                        let mut pending = scaled;
                        loop {
                            if stop.load(Ordering::Relaxed) || sleep_expired(sleep_until) {
                                return true;
                            }
                            match tx.try_send(pending) {
                                Ok(()) => break,
                                Err(TrySendError::Full(b)) => {
                                    pending = b;
                                    tokio::select! {
                                        _ = notify.notified() => return true,
                                        _ = tokio::time::sleep(Duration::from_millis(15)) => {}
                                    }
                                }
                                Err(TrySendError::Disconnected(_)) => return true,
                            }
                        }
                    }
                }
                _ => break,
            }
        }
        if stop.load(Ordering::Relaxed) || sleep_expired(sleep_until) {
            return true;
        }
        log::info!("фон/радио: поток оборвался, переподключение через {backoff_s}с…");
        if !wait_or_stop(Duration::from_secs(backoff_s), stop, notify, sleep_until).await {
            return true;
        }
        backoff_s = (backoff_s * 2).min(8);
    }
    ever_connected
}
