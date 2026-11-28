//! Звук колонки: микрофон — обработанный канал vqe_0 (эхо- и шумоподавление Яндекса) через наш
//! vqe_tap.so в maind; динамик — aplay в тот же ALSA-микшер, что и у Алисы.

use jarvis_dsp::audio::{Limiter, OUT_RATE, Upsampler, gain_pcm};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const MIC_RATE: u32 = 16000;

pub type Frame = Arc<[i16]>;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Микрофон: датаграммы PCM s16le 16 кГц от vqe_tap.so, раздаются подписчикам.
pub struct Mic {
    path: PathBuf,
    subs: Mutex<HashMap<u64, SyncSender<Frame>>>,
    next: AtomicU64,
    seen: AtomicU64,
    /// Общее усиление (дБ) для всех подписчиков: слово, ИИ, записи для обучения.
    pub gain_db: Box<dyn Fn() -> i64 + Send + Sync>,
}

impl Mic {
    pub fn new(path: PathBuf, gain_db: Box<dyn Fn() -> i64 + Send + Sync>) -> Arc<Mic> {
        Arc::new(Mic { path, subs: Mutex::default(), next: AtomicU64::new(0), seen: AtomicU64::new(0), gain_db })
    }

    /// Подписаться: кадры по ~10 мс; медленный подписчик теряет кадры, а не тормозит остальных.
    pub fn subscribe(&self, buf: usize) -> (u64, Receiver<Frame>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = sync_channel(buf);
        self.subs.lock().unwrap().insert(id, tx);
        (id, rx)
    }

    pub fn unsubscribe(&self, id: u64) {
        self.subs.lock().unwrap().remove(&id);
    }

    /// Микрофон на связи: кадр был за последние 3 с.
    pub fn online(&self) -> bool {
        now_ms().saturating_sub(self.seen.load(Ordering::Relaxed)) < 3000
    }

    /// Короткое объяснение, почему звук с микрофона ещё не поступает.
    pub fn why(&self) -> String {
        if self.online() {
            return String::new();
        }
        #[cfg(target_os = "linux")]
        {
            if std::path::Path::new("/data/opt/jarvisd/no-mic").exists() {
                return "отключён файлом no-mic".into();
            }
            let maps = crate::alice::platformd_maps();
            if maps.iter().any(|m| classify_platformd_maps(m) == "deleted") {
                return "библиотека микрофона обновилась, перезагрузите колонку".into();
            }
            if maps.iter().any(|m| classify_platformd_maps(m) == "missing") {
                return "перехват микрофона ещё не загружен в систему, перезагрузите колонку".into();
            }
            if !maps.is_empty() {
                return "перехват загружен, но звук не поступает, попробуйте перезагрузить колонку".into();
            }
        }
        String::new()
    }

    /// Раздать кадр (из сокета или для тестов).
    pub fn publish(&self, mut pcm: Vec<i16>) {
        gain_pcm(&mut pcm, (self.gain_db)());
        let f: Frame = pcm.into();
        self.seen.store(now_ms(), Ordering::Relaxed);
        let mut subs = self.subs.lock().unwrap();
        subs.retain(|_, tx| !matches!(tx.try_send(f.clone()), Err(TrySendError::Disconnected(_))));
    }

    /// Слушать сокет (в своём потоке): упал — повтор через 3 с.
    pub fn run(self: Arc<Self>) {
        thread::Builder::new()
            .name("mic".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                loop {
                    if let Err(e) = self.serve() {
                        log::info!("микрофон: {e} — повтор через 3 с");
                    }
                    thread::sleep(Duration::from_secs(3));
                }
            })
            .expect("поток микрофона");
    }

    #[cfg(unix)]
    fn serve(&self) -> std::io::Result<()> {
        use std::os::unix::{fs::PermissionsExt, net::UnixDatagram};
        if let Some(d) = self.path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::remove_file(&self.path);
        let sock = UnixDatagram::bind(&self.path)?;
        // maind пишет от root, но на других прошивках может быть другой пользователь
        let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o666));
        let mut buf = [0u8; 4096];
        loop {
            let n = sock.recv(&mut buf)?;
            if n < 2 {
                continue;
            }
            let pcm = buf[..n / 2 * 2].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            self.publish(pcm);
        }
    }

    #[cfg(not(unix))]
    fn serve(&self) -> std::io::Result<()> {
        Err(std::io::Error::other("микрофон есть только на колонке"))
    }
}

/// Усиление голоса Джарвиса при громкости 100% (голос моделей приходит тихим, около −22 дБ):
/// ×6,5 с ограничителем даёт речь около −8 дБ — чуть громче Алисы.
const PLAY_GAIN: f64 = 6.5;

/// Поиск 32-битного armhf-помощника `alsasink` (прямой вывод в `libasound.so.2` без форка `aplay`).
pub(crate) fn alsasink_path() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let p = dir.join("alsasink");
        if p.is_file() {
            return Some(p);
        }
    }
    let p = PathBuf::from("/data/opt/jarvisd/alsasink");
    p.is_file().then_some(p)
}

/// Постоянный дочерний процесс `alsasink` (1 процесс на всё время работы — ноль fork+exec на фразы).
struct AlsaSink {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl AlsaSink {
    fn spawn(path: &std::path::Path) -> std::io::Result<Self> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| std::io::Error::other("alsasink stdin"))?;
        let stdout = BufReader::new(child.stdout.take().ok_or_else(|| std::io::Error::other("alsasink stdout"))?);
        Ok(Self { child, stdin, stdout })
    }

    fn cmd_reply(&mut self, op: u8, arg: u32) -> std::io::Result<String> {
        let mut hdr = [0u8; 5];
        hdr[0] = op;
        hdr[1..5].copy_from_slice(&arg.to_le_bytes());
        self.stdin.write_all(&hdr)?;
        self.stdin.flush()?;
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line)?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "alsasink eof"));
        }
        Ok(line.trim().to_string())
    }

    fn open(&mut self, rate: usize) -> std::io::Result<()> {
        let reply = self.cmd_reply(b'O', rate as u32)?;
        if reply == "OK" {
            Ok(())
        } else {
            Err(std::io::Error::other(format!("alsasink open: {reply}")))
        }
    }

    fn write_chunk(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let mut hdr = [0u8; 5];
        hdr[0] = b'W';
        hdr[1..5].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        self.stdin.write_all(&hdr)?;
        self.stdin.write_all(bytes)?;
        self.stdin.flush()
    }

    fn close_pcm(&mut self, drain: bool) -> std::io::Result<()> {
        let op = if drain { b'D' } else { b'X' };
        let reply = self.cmd_reply(op, 0)?;
        if reply == "OK" {
            Ok(())
        } else {
            Err(std::io::Error::other(format!("alsasink close: {reply}")))
        }
    }
}

impl Drop for AlsaSink {
    fn drop(&mut self) {
        let _ = self.stdin.write_all(&[b'Q', 0, 0, 0, 0]);
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

enum PlaybackBackend {
    Aplay {
        child: Child,
        pump: Option<thread::JoinHandle<()>>,
    },
    Alsa {
        stop_now: Arc<std::sync::atomic::AtomicBool>,
        pump: Option<thread::JoinHandle<Option<AlsaSink>>>,
    },
}

struct Playing {
    backend: PlaybackBackend,
    queue: Option<SyncSender<Vec<u8>>>,
    queued: Arc<AtomicUsize>,
    lim: Limiter,
    up: Option<Upsampler>,
    boost: f64,
    shaped: Vec<i16>,
    up_buf: Vec<i16>,
}

/// Плеер: прямой вывод через постоянный `alsasink` (`libasound.so.2`, без форка `aplay`)
/// с автоматическим откатом на `aplay`, если помощник недоступен.
pub struct Player {
    cur: Mutex<Option<Playing>>,
    idle_sink: Mutex<Option<AlsaSink>>,
    aplay: String,
    /// 0..100, читается на каждом куске.
    pub volume: Box<dyn Fn() -> i64 + Send + Sync>,
    /// Поправка к громкости голоса от громкости самой колонки (1 — без поправки).
    pub trim: Box<dyn Fn() -> f64 + Send + Sync>,
}

impl Player {
    pub fn new(volume: Box<dyn Fn() -> i64 + Send + Sync>, trim: Box<dyn Fn() -> f64 + Send + Sync>) -> Arc<Player> {
        let aplay = if std::path::Path::new("/usr/bin/aplay").exists() { "/usr/bin/aplay" } else { "aplay" };
        Arc::new(Player { cur: Mutex::default(), idle_sink: Mutex::default(), aplay: aplay.into(), volume, trim })
    }

    pub fn playing(&self) -> bool {
        self.cur.lock().unwrap().is_some()
    }

    /// Есть ли ещё недоигранные куски в очереди на динамик.
    pub fn has_queued(&self) -> bool {
        self.cur
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|p| p.queued.load(Ordering::Relaxed) > 0)
    }

    fn take_or_spawn_sink(&self) -> Option<AlsaSink> {
        if let Some(sink) = self.idle_sink.lock().unwrap().take() {
            return Some(sink);
        }
        let path = alsasink_path()?;
        match AlsaSink::spawn(&path) {
            Ok(sink) => {
                log::info!("динамик: запущен прямой ALSA-вывод (alsasink)");
                Some(sink)
            }
            Err(e) => {
                log::warn!("динамик: alsasink не запустился ({e}), использую aplay");
                None
            }
        }
    }

    /// Открыть плеер; rate — частота PCM (моно s16le).
    pub fn start(&self, rate: usize) -> std::io::Result<()> {
        self.stop(true);
        let (up, out) = if rate > 0 && rate < OUT_RATE && OUT_RATE % rate == 0 {
            (Some(Upsampler::new(OUT_RATE / rate)), OUT_RATE)
        } else {
            (None, rate)
        };
        if let Some(mut sink) = self.take_or_spawn_sink() {
            let open_res = sink.open(out).or_else(|e| {
                log::warn!("динамик (alsasink): переоткрытие после сбоя ({e})");
                let path = alsasink_path().ok_or_else(|| std::io::Error::other("no alsasink"))?;
                sink = AlsaSink::spawn(&path)?;
                sink.open(out)
            });
            if open_res.is_ok() {
                let (tx, rx) = sync_channel::<Vec<u8>>(2048);
                let queued = Arc::new(AtomicUsize::new(0));
                let q_pump = queued.clone();
                let stop_now = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let stop_flag = stop_now.clone();
                let pump = thread::Builder::new()
                    .name("alsa-pump".into())
                    .stack_size(128 * 1024)
                    .spawn(move || pump_alsasink(sink, rx, q_pump, out, stop_flag))?;
                *self.cur.lock().unwrap() = Some(Playing {
                    backend: PlaybackBackend::Alsa { stop_now, pump: Some(pump) },
                    queue: Some(tx),
                    queued,
                    lim: Limiter::new(rate),
                    up,
                    boost: 1.0,
                    shaped: Vec::new(),
                    up_buf: Vec::new(),
                });
                return Ok(());
            }
            log::warn!("динамик: alsasink недоступен, откат на aplay");
        }

        let mut child = Command::new(&self.aplay)
            .args(["-q", "-D", "default", "-t", "raw", "-f", "S16_LE", "-r", &out.to_string(), "-c", "1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().unwrap();
        let (tx, rx) = sync_channel::<Vec<u8>>(2048);
        let queued = Arc::new(AtomicUsize::new(0));
        let q_pump = queued.clone();
        let pump = thread::Builder::new()
            .name("aplay-pump".into())
            .stack_size(128 * 1024)
            .spawn(move || pump(stdin, rx, q_pump, out))?;
        *self.cur.lock().unwrap() = Some(Playing {
            backend: PlaybackBackend::Aplay { child, pump: Some(pump) },
            queue: Some(tx),
            queued,
            lim: Limiter::new(rate),
            up,
            boost: 1.0,
            shaped: Vec::new(),
            up_buf: Vec::new(),
        });
        Ok(())
    }

    /// Доп. усиление голоса конкретного движка (ChatGPT по подписке приходит тише).
    pub fn set_boost(&self, b: f64) {
        if let Some(p) = self.cur.lock().unwrap().as_mut() {
            p.boost = b;
        }
    }

    fn prepare_bytes(&self, pcm: &[i16]) -> Result<(Vec<u8>, SyncSender<Vec<u8>>, Arc<AtomicUsize>), &'static str> {
        let mut g = (self.volume)() as f64 / 50.0 * PLAY_GAIN; // 50% — прежний максимум, 100% — вдвое громче
        g *= (self.trim)();
        let mut cur = self.cur.lock().unwrap();
        let p = cur.as_mut().ok_or("плеер не запущен")?;
        if p.boost > 0.0 {
            g *= p.boost;
        }
        let g = g as f32;
        p.shaped.clear();
        p.shaped.reserve(pcm.len());
        for &s in pcm {
            p.shaped.push(p.lim.next_f32(s as f32 * g));
        }
        let src: &[i16] = if let Some(up) = p.up.as_mut() {
            up.run_into(&p.shaped, &mut p.up_buf);
            &p.up_buf
        } else {
            &p.shaped
        };
        let mut b = Vec::with_capacity(src.len() * 2);
        for &s in src {
            b.extend_from_slice(&s.to_le_bytes());
        }
        let q = p.queue.as_ref().ok_or("плеер закрыт")?.clone();
        let queued = p.queued.clone();
        Ok((b, q, queued))
    }

    /// Кусок PCM; не блокирует (если aplay завис, ответ теряется, а не встаёт всё).
    pub fn write(&self, pcm: &[i16]) -> Result<(), &'static str> {
        let (b, q, queued) = self.prepare_bytes(pcm)?;
        match q.try_send(b) {
            Ok(()) => {
                queued.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(_) => Err("плеер не успевает"),
        }
    }

    /// Кусок PCM с мягким ожиданием освобождения очереди, чтобы длинные тексты никогда не обрезались.
    pub fn write_wait(&self, pcm: &[i16], abort: impl Fn() -> bool) -> Result<(), &'static str> {
        let (mut pending, q, queued) = self.prepare_bytes(pcm)?;
        loop {
            if abort() {
                return Err("прервано");
            }
            match q.try_send(pending) {
                Ok(()) => {
                    queued.fetch_add(1, Ordering::Relaxed);
                    return Ok(());
                }
                Err(TrySendError::Full(b)) => {
                    pending = b;
                    thread::sleep(Duration::from_millis(15));
                }
                Err(TrySendError::Disconnected(_)) => return Err("плеер закрыт"),
            }
        }
    }

    /// Доиграть то, что уже отдано, и закрыть.
    pub fn finish(&self, timeout: Duration) {
        self.finish_cancelable(timeout, || false);
    }

    /// Доиграть буфер до конца, но мгновенно прерваться, если пришёл «Стоп» или перебили.
    pub fn finish_cancelable(&self, timeout: Duration, abort: impl Fn() -> bool) {
        let Some(mut p) = self.cur.lock().unwrap().take() else { return };
        p.queue.take(); // закрыть очередь: насос допишет и закроет поток
        let t0 = Instant::now();
        match &mut p.backend {
            PlaybackBackend::Aplay { child, pump } => {
                if let Some(h) = pump.take() {
                    while !h.is_finished() && t0.elapsed() < timeout {
                        if abort() {
                            interrupt(child);
                            wait_child(child, Duration::from_secs(2));
                            return;
                        }
                        thread::sleep(Duration::from_millis(20));
                    }
                }
                if abort() {
                    interrupt(child);
                    wait_child(child, Duration::from_secs(2));
                    return;
                }
                wait_child(child, timeout.saturating_sub(t0.elapsed()).max(Duration::from_millis(200)));
            }
            PlaybackBackend::Alsa { stop_now, pump } => {
                if let Some(h) = pump.take() {
                    while !h.is_finished() && t0.elapsed() < timeout {
                        if abort() {
                            stop_now.store(true, Ordering::SeqCst);
                            break;
                        }
                        thread::sleep(Duration::from_millis(15));
                    }
                    if !h.is_finished() {
                        stop_now.store(true, Ordering::SeqCst);
                        let deadline = Instant::now() + Duration::from_millis(800);
                        while !h.is_finished() && Instant::now() < deadline {
                            thread::sleep(Duration::from_millis(15));
                        }
                    }
                    if h.is_finished()
                        && let Ok(Some(sink)) = h.join()
                    {
                        *self.idle_sink.lock().unwrap() = Some(sink);
                    }
                }
            }
        }
    }

    /// Оборвать сразу (now) или дать доиграть буфер ALSA.
    pub fn stop(&self, now: bool) {
        let Some(mut p) = self.cur.lock().unwrap().take() else { return };
        p.queue.take();
        match &mut p.backend {
            PlaybackBackend::Aplay { child, .. } => {
                if now {
                    interrupt(child);
                }
                wait_child(child, Duration::from_secs(2));
            }
            PlaybackBackend::Alsa { stop_now, pump } => {
                if now {
                    stop_now.store(true, Ordering::SeqCst);
                }
                if let Some(h) = pump.take() {
                    let deadline = Instant::now() + Duration::from_secs(2);
                    while !h.is_finished() && Instant::now() < deadline {
                        thread::sleep(Duration::from_millis(15));
                    }
                    if h.is_finished()
                        && let Ok(Some(sink)) = h.join()
                    {
                        *self.idle_sink.lock().unwrap() = Some(sink);
                    }
                }
            }
        }
    }
}

fn pump_alsasink(
    mut sink: AlsaSink,
    rx: Receiver<Vec<u8>>,
    queued: Arc<AtomicUsize>,
    out_rate: usize,
    stop_now: Arc<std::sync::atomic::AtomicBool>,
) -> Option<AlsaSink> {
    use std::sync::mpsc::RecvTimeoutError;
    let rate = out_rate.max(8_000);
    let bytes_per_sec = (rate * 2) as f64;
    let chunk_max = ((rate / 25) * 2).max(1600); // ~40 мс чанки, чтобы stop_now срабатывал мгновенно
    let lead_in = vec![0u8; (rate / 12) * 2]; // ~83 мс тишины на прогрев dmix и усилителя
    let keepalive = vec![0u8; (rate / 25) * 2];
    let keepalive_dur = Duration::from_millis(40);

    let write_sliced = |sink: &mut AlsaSink, data: &[u8], stop: &std::sync::atomic::AtomicBool| -> Result<bool, std::io::Error> {
        for part in data.chunks(chunk_max) {
            if stop.load(Ordering::SeqCst) {
                return Ok(true);
            }
            sink.write_chunk(part)?;
        }
        Ok(false)
    };

    let first = match rx.recv() {
        Ok(b) => b,
        Err(_) => {
            return sink.close_pcm(false).ok().map(|()| sink);
        }
    };
    queued.fetch_sub(1, Ordering::Relaxed);
    if stop_now.load(Ordering::SeqCst) {
        return sink.close_pcm(false).ok().map(|()| sink);
    }
    let first_len = first.len();
    match write_sliced(&mut sink, &lead_in, &stop_now).and_then(|aborted| {
        if aborted { Ok(true) } else { write_sliced(&mut sink, &first, &stop_now) }
    }) {
        Ok(true) => return sink.close_pcm(false).ok().map(|()| sink),
        Ok(false) => {}
        Err(e) => {
            log::warn!("динамик (alsasink): ошибка записи ({e})");
            return None;
        }
    }

    let now = Instant::now();
    let mut buffered_until = now + Duration::from_secs_f64((lead_in.len() + first_len) as f64 / bytes_per_sec);
    loop {
        if stop_now.load(Ordering::SeqCst) {
            return sink.close_pcm(false).ok().map(|()| sink);
        }
        let wait = buffered_until
            .saturating_duration_since(Instant::now())
            .saturating_sub(Duration::from_millis(55))
            .clamp(Duration::from_millis(15), Duration::from_millis(60));
        match rx.recv_timeout(wait) {
            Ok(b) => {
                queued.fetch_sub(1, Ordering::Relaxed);
                let len = b.len();
                match write_sliced(&mut sink, &b, &stop_now) {
                    Ok(true) => return sink.close_pcm(false).ok().map(|()| sink),
                    Ok(false) => {}
                    Err(e) => {
                        log::warn!("динамик (alsasink): ошибка записи ({e})");
                        return None;
                    }
                }
                let now = Instant::now();
                buffered_until = buffered_until.max(now) + Duration::from_secs_f64(len as f64 / bytes_per_sec);
            }
            Err(RecvTimeoutError::Timeout) => {
                if stop_now.load(Ordering::SeqCst) {
                    return sink.close_pcm(false).ok().map(|()| sink);
                }
                let now = Instant::now();
                if buffered_until <= now + Duration::from_millis(80) {
                    if let Err(e) = sink.write_chunk(&keepalive) {
                        log::warn!("динамик (alsasink): ошибка keepalive ({e})");
                        return None;
                    }
                    buffered_until = buffered_until.max(now) + keepalive_dur;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                let drain = !stop_now.load(Ordering::SeqCst);
                return sink.close_pcm(drain).ok().map(|()| sink);
            }
        }
    }
}

fn pump(mut stdin: ChildStdin, rx: Receiver<Vec<u8>>, queued: Arc<AtomicUsize>, out_rate: usize) {
    use std::io::Write;
    use std::sync::mpsc::RecvTimeoutError;
    let rate = out_rate.max(8_000);
    let bytes_per_sec = (rate * 2) as f64;
    // 100 мс прогрева тишиной пробуждают ALSA dmix и усилитель колонки, чтобы не съедался первый слог.
    let lead_in = vec![0u8; (rate / 10) * 2];
    // 40 мс тишины во время сетевых пауз держат DMA-буфер ALSA живым без XRUN/underrun и без раздувания задержки.
    let keepalive = vec![0u8; (rate / 25) * 2];
    let keepalive_dur = Duration::from_millis(40);
    let first = match rx.recv() {
        Ok(b) => b,
        Err(_) => return,
    };
    let first_len = first.len();
    let res = stdin.write_all(&lead_in).and_then(|_| stdin.write_all(&first));
    queued.fetch_sub(1, Ordering::Relaxed);
    if let Err(e) = res {
        log::warn!("динамик (aplay): ошибка записи ({e})");
        return;
    }
    let now = Instant::now();
    let mut buffered_until = now + Duration::from_secs_f64((lead_in.len() + first_len) as f64 / bytes_per_sec);
    loop {
        let wait = buffered_until
            .saturating_duration_since(Instant::now())
            .saturating_sub(Duration::from_millis(60))
            .clamp(Duration::from_millis(20), Duration::from_millis(80));
        match rx.recv_timeout(wait) {
            Ok(b) => {
                let len = b.len();
                let res = stdin.write_all(&b);
                queued.fetch_sub(1, Ordering::Relaxed);
                if let Err(e) = res {
                    log::warn!("динамик (aplay): ошибка записи ({e})");
                    return; // плеер умер — остаток выбрасываем (rx закроется вместе с потоком)
                }
                let now = Instant::now();
                buffered_until = buffered_until.max(now) + Duration::from_secs_f64(len as f64 / bytes_per_sec);
            }
            Err(RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                // Доливаем 40 мс тишины только когда в буфере ALSA остаётся меньше ~80 мс —
                // буфер никогда не пересыхает (нет underrun), но и не копит лишнюю задержку >120 мс.
                if buffered_until <= now + Duration::from_millis(80) {
                    if let Err(e) = stdin.write_all(&keepalive) {
                        log::warn!("динамик (aplay): ошибка записи ({e})");
                        return;
                    }
                    buffered_until = buffered_until.max(now) + keepalive_dur;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn log_aplay_exit(child: &mut Child, status: std::process::ExitStatus) {
    if status.success() {
        return;
    }
    // Если процесс завершился не по нашему SIGINT, а сам с кодом ошибки — покажем stderr ALSA в журнале.
    let Some(code) = status.code() else { return };
    use std::io::Read;
    let mut err = String::new();
    if let Some(stderr) = child.stderr.as_mut() {
        let _ = stderr.take(512).read_to_string(&mut err);
    }
    let detail = err.trim().replace('\n', " | ");
    if detail.is_empty() {
        log::warn!("динамик (aplay): завершился с кодом {code}");
    } else {
        log::warn!("динамик (aplay): код {code}: {detail}");
    }
}

fn interrupt(child: &Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
    #[cfg(not(unix))]
    let _ = child;
}

fn wait_child(child: &mut Child, timeout: Duration) {
    let t0 = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            log_aplay_exit(child, status);
            return;
        }
        if t0.elapsed() > timeout {
            interrupt(child);
            thread::sleep(Duration::from_secs(1));
            if let Ok(Some(status)) = child.try_wait() {
                log_aplay_exit(child, status);
            } else {
                let _ = child.kill();
                let _ = child.wait();
            }
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Отправить WAV (16 кГц моно) в сокет микрофона кусками по 10 мс в реальном времени — для проверки на колонке.
#[cfg(unix)]
pub fn send_wav(sock: &std::path::Path, wav: &std::path::Path) -> std::io::Result<()> {
    let raw = std::fs::read(wav)?;
    let pcm = if raw.len() > 44 && &raw[..4] == b"RIFF" { &raw[44..] } else { &raw[..] };
    let s = std::os::unix::net::UnixDatagram::unbound()?;
    let t0 = Instant::now();
    for (i, c) in pcm.chunks(320).enumerate() {
        let due = t0 + Duration::from_millis(10 * i as u64);
        if let Some(d) = due.checked_duration_since(Instant::now()) {
            thread::sleep(d);
        }
        s.send_to(c, sock)?;
    }
    Ok(())
}

/// Проверить содержимое /proc/<pid>/maps процесса maind --platformd на наличие vqe_tap.so.
pub(crate) fn classify_platformd_maps(maps: &str) -> &'static str {
    let mut loaded = false;
    for line in maps.lines() {
        if line.contains("vqe_tap.so") {
            if line.contains("(deleted)") {
                return "deleted";
            }
            loaded = true;
        }
    }
    if loaded { "loaded" } else { "missing" }
}

#[cfg(test)]
mod tests {
    use super::classify_platformd_maps;

    #[test]
    fn detects_vqe_tap_state_in_proc_maps() {
        assert_eq!(
            classify_platformd_maps("7f8000-7f9000 r-xp 00000000 /data/opt/jarvisd/vqe_tap.so (deleted)\n"),
            "deleted"
        );
        assert_eq!(
            classify_platformd_maps("7f8000-7f9000 r-xp 00000000 /data/opt/jarvisd/vqe_tap.so\n"),
            "loaded"
        );
        assert_eq!(
            classify_platformd_maps("7f8000-7f9000 r-xp 00000000 /system/vendor/quasar/maind\n"),
            "missing"
        );
    }
}
