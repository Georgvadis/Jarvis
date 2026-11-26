//! Фокус-режим (Помодоро) с живым обратным отсчётом MM:SS на переднем LED-дисплее Station Lite 2.

use crate::{ambient::Ambient, engine::Engine, led::Leds};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct ActiveFocus {
    until: Instant,
    total_min: u64,
    stop: Arc<AtomicBool>,
    started_ambient: bool,
}

pub struct Focus {
    leds: Arc<Leds>,
    ambient: Arc<Ambient>,
    active: Arc<Mutex<Option<ActiveFocus>>>,
}

impl Focus {
    pub fn new(leds: Arc<Leds>, ambient: Arc<Ambient>) -> Arc<Self> {
        Arc::new(Self {
            leds,
            ambient,
            active: Arc::new(Mutex::new(None)),
        })
    }

    pub fn status(&self) -> Value {
        let g = self.active.lock().unwrap();
        if let Some(a) = g.as_ref() {
            let rem = a.until.saturating_duration_since(Instant::now()).as_secs();
            json!({"running": true, "remaining_s": rem, "total_min": a.total_min})
        } else {
            json!({"running": false, "remaining_s": 0, "total_min": 0})
        }
    }

    pub fn stop(&self) {
        let prev = self.active.lock().unwrap().take();
        if let Some(a) = prev {
            a.stop.store(true, Ordering::SeqCst);
            self.leds.set_clock_digits(None, false);
            if a.started_ambient {
                self.ambient.stop();
            }
            log::info!("фокус: остановлен");
        }
    }

    pub fn start(self: &Arc<Self>, engine: Arc<Engine>, minutes: u64) -> String {
        let mins = minutes.clamp(1, 99);
        self.stop();

        let started_ambient = if self.ambient.current_id().is_none() {
            self.ambient.start("rain").is_ok()
        } else {
            false
        };

        let stop = Arc::new(AtomicBool::new(false));
        let until = Instant::now() + Duration::from_secs(mins * 60);
        *self.active.lock().unwrap() = Some(ActiveFocus {
            until,
            total_min: mins,
            stop: stop.clone(),
            started_ambient,
        });

        let me = self.clone();
        let stop_bg = stop.clone();
        tokio::spawn(async move {
            log::info!("фокус: запущен на {mins} мин");
            let mut tick = 0u64;
            loop {
                if stop_bg.load(Ordering::Relaxed) {
                    break;
                }
                let rem = until.saturating_duration_since(Instant::now()).as_secs();
                if rem == 0 {
                    me.leds.set_clock_digits(None, false);
                    let mut should_stop_amb = false;
                    {
                        let mut g = me.active.lock().unwrap();
                        if g.as_ref().is_some_and(|a| Arc::ptr_eq(&a.stop, &stop_bg))
                            && let Some(finished) = g.take()
                        {
                            should_stop_amb = finished.started_ambient;
                        }
                    }
                    if should_stop_amb {
                        me.ambient.stop();
                    }
                    engine
                        .journal
                        .note("фокус", "", &format!("Фокус {mins} мин завершён!"));
                    let e = engine.clone();
                    tokio::task::spawn_blocking(move || {
                        e.earcon(true);
                        e.say_text(&format!(
                            "Время фокуса {mins} минут вышло! Отличная работа, сделай небольшой перерыв."
                        ));
                    });
                    break;
                }
                let m = (rem / 60).min(99) as u8;
                let s = (rem % 60) as u8;
                let digits = [m / 10, m % 10, s / 10, s % 10];
                me.leds.set_clock_digits(Some(digits), tick % 2 == 0);
                tick += 1;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });

        format!("Фокус на {mins} минут запущен. Обратный отсчёт на дисплее колонки.")
    }

    pub fn fast_command(self: &Arc<Self>, engine: &Arc<Engine>, text: &str) -> Option<String> {
        let t = text.trim().to_lowercase();
        if !(t.contains("фокус") || t.contains("помодоро") || t.contains("pomodoro")) {
            return None;
        }
        if t.contains("покажи фокус")
            || t.contains("расскажи фокус")
            || t.contains("какой фокус")
            || t.contains("в чем фокус")
            || t.contains("в чём фокус")
            || t.contains("фокус с ")
            || t.contains("фокусы")
        {
            return None;
        }
        if t.contains("выключи")
            || t.contains("останови")
            || t.contains("отмени")
            || t.contains("закончи")
            || t.contains("стоп")
        {
            self.stop();
            return Some("Фокус-режим остановлен.".into());
        }
        let has_start_verb = t.contains("включи")
            || t.contains("запусти")
            || t.contains("поставь")
            || t.contains("начни")
            || t.contains("старт");
        let explicit = has_start_verb
            || t.contains("помодоро")
            || t.contains("pomodoro")
            || t.contains("фокус-режим")
            || t.contains("режим фокуса")
            || t.contains("фокус на ");
        if !explicit {
            return None;
        }
        let mut mins = 25u64;
        for tok in t.split_whitespace() {
            let digits: String = tok.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse::<u64>() && n > 0 {
                mins = n;
                break;
            }
        }
        Some(self.start(engine.clone(), mins))
    }
}
