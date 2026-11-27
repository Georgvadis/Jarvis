//! Журнал: «2006/01/02 15:04:05 текст» в часовом поясе хозяина (часы колонки идут по Гринвичу).
//! Формат тот же, что у Go-версии: панель («Журнал») и jarvisd.sh читают его одинаково.

use std::{
    io::Write,
    sync::{
        OnceLock, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

static TZ: OnceLock<RwLock<jiff::tz::TimeZone>> = OnceLock::new();
static LINE_COUNT: AtomicU64 = AtomicU64::new(0);

#[cfg(unix)]
fn maybe_rotate_runtime_log() {
    use std::{fs, io::Seek, io::Read, os::unix::fs::MetadataExt};
    const MAX_BYTES: u64 = 2 * 1024 * 1024;
    const KEEP_TAIL: u64 = 256 * 1024;
    let path = std::path::Path::new("/tmp/jarvisd/jarvisd.log");
    let Ok(meta) = fs::metadata(path) else { return };
    if meta.len() <= MAX_BYTES {
        return;
    }
    // Проверяем, что stderr (fd 2) действительно указывает на этот же файл (O_APPEND от jarvis-supervisor).
    let Ok(fd2) = fs::metadata("/proc/self/fd/2") else { return };
    if fd2.ino() != meta.ino() || fd2.dev() != meta.dev() {
        return;
    }
    if let Ok(mut f) = fs::File::open(path) {
        let start = meta.len().saturating_sub(KEEP_TAIL);
        if f.seek(std::io::SeekFrom::Start(start)).is_ok() {
            let mut tail = Vec::with_capacity(KEEP_TAIL as usize);
            if f.read_to_end(&mut tail).is_ok() {
                let _ = fs::write("/tmp/jarvisd/jarvisd.log.1", tail);
            }
        }
    }
    let _ = fs::OpenOptions::new().write(true).truncate(true).open(path);
}

fn tz() -> &'static RwLock<jiff::tz::TimeZone> {
    TZ.get_or_init(|| RwLock::new(jiff::tz::TimeZone::UTC))
}

pub fn set_timezone(name: &str) {
    if let Ok(z) = jiff::tz::TimeZone::get(name) {
        *tz().write().unwrap() = z;
    }
}

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info && m.target().starts_with("jarvis")
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let now = jiff::Timestamp::now().to_zoned(tz().read().unwrap().clone());
        let line = format!("{} {}\n", now.strftime("%Y/%m/%d %H:%M:%S"), r.args());
        let _ = std::io::stderr().lock().write_all(line.as_bytes());
        #[cfg(unix)]
        if LINE_COUNT.fetch_add(1, Ordering::Relaxed).is_multiple_of(512) {
            maybe_rotate_runtime_log();
        }
    }

    fn flush(&self) {}
}

pub fn init() {
    static L: Logger = Logger;
    let _ = log::set_logger(&L);
    log::set_max_level(log::LevelFilter::Info);
}
