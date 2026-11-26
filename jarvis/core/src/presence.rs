//! «Живое присутствие» в домашнем Wi-Fi без датчиков: периодически проверяем доступность IP телефона
//! по ping и /proc/net/arp; если хозяина не было >= presence_away_min минут и он вернулся домой,
//! колонка тепло приветствует его своим голосом (с учётом ночного режима).

use crate::{engine::Engine, settings::Store};
use serde_json::{Value, json};
use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum LinkState {
    Unknown,
    Home,
    Away,
}

impl LinkState {
    fn as_str(self) -> &'static str {
        match self {
            LinkState::Unknown => "unknown",
            LinkState::Home => "home",
            LinkState::Away => "away",
        }
    }
}

struct State {
    ip: String,
    link: LinkState,
    away_since: Option<Instant>,
    last_seen: Option<Instant>,
    last_greeted: Option<Instant>,
    misses: u32,
}

pub struct Presence {
    st: Mutex<State>,
}

impl Presence {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            st: Mutex::new(State {
                ip: String::new(),
                link: LinkState::Unknown,
                away_since: None,
                last_seen: None,
                last_greeted: None,
                misses: 0,
            }),
        })
    }

    pub fn status(&self, store: &Store) -> Value {
        let s = store.get();
        let g = self.st.lock().unwrap();
        json!({
            "configured": !s.presence_phone_ip.trim().is_empty(),
            "ip": s.presence_phone_ip,
            "away_min": s.presence_away_min,
            "greeting": s.presence_greeting,
            "state": g.link.as_str(),
            "last_seen_ago_s": g.last_seen.map(|t| t.elapsed().as_secs()),
            "away_for_s": g.away_since.map(|t| t.elapsed().as_secs()),
        })
    }

    pub fn spawn(self: Arc<Self>, engine: Arc<Engine>) {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            loop {
                let s = engine.store.get();
                let ip = s.presence_phone_ip.trim().to_string();
                if let Some(valid_ip) = valid_lan_ipv4(&ip) {
                    let ip_probe = valid_ip.to_string();
                    let reachable = tokio::task::spawn_blocking(move || probe_ip(&ip_probe))
                        .await
                        .unwrap_or(false);
                    let away_thresh = Duration::from_secs((s.presence_away_min.clamp(5, 240) as u64) * 60);
                    let should_greet = self.update_probe(&ip, reachable, away_thresh, &engine.store);
                    if should_greet {
                        self.greet_home(&engine).await;
                    }
                } else {
                    let mut g = self.st.lock().unwrap();
                    if !g.ip.is_empty() {
                        g.ip.clear();
                        g.link = LinkState::Unknown;
                        g.misses = 0;
                        g.away_since = None;
                    }
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
    }

    fn update_probe(&self, ip: &str, reachable: bool, away_thresh: Duration, store: &Store) -> bool {
        let mut g = self.st.lock().unwrap();
        if g.ip != ip {
            g.ip = ip.to_string();
            g.link = LinkState::Unknown;
            g.misses = 0;
            g.away_since = None;
            g.last_seen = None;
        }
        let now = Instant::now();
        if reachable {
            let was_away_long = g.link == LinkState::Away
                && g.away_since.is_some_and(|since| since.elapsed() >= away_thresh);
            let cooldown_ok = g.last_greeted.is_none_or(|t| t.elapsed() >= Duration::from_secs(1200));
            g.link = LinkState::Home;
            g.last_seen = Some(now);
            g.misses = 0;
            g.away_since = None;
            if was_away_long && cooldown_ok && !store.is_night() {
                g.last_greeted = Some(now);
                return true;
            }
            return false;
        }

        // Unreachable
        match g.link {
            LinkState::Unknown => {
                g.misses += 1;
                if g.misses >= 2 {
                    g.link = LinkState::Away;
                    g.away_since = Some(now);
                }
            }
            LinkState::Home => {
                g.misses += 1;
                if g.misses >= 4 {
                    g.link = LinkState::Away;
                    g.away_since = Some(now);
                }
            }
            LinkState::Away => {}
        }
        false
    }

    pub async fn greet_home(&self, engine: &Arc<Engine>) {
        if engine.talking() {
            return;
        }
        let s = engine.store.get();
        let p = s.active_profile();
        let custom = s.presence_greeting.trim().to_string();
        let text = if !custom.is_empty() {
            custom
        } else {
            let now = s.now();
            let base = s.prompt_text(&p.prompt);
            let sys = format!(
                "{base}\nХозяин только что вернулся домой (сейчас {:02}:{:02}). Поприветствуй его тепло и в своём характере в 1–2 коротких предложения без вопросов в конце. Можно начать с [happy].",
                now.hour(),
                now.minute()
            );
            crate::llm::ask_with_pool(
                &engine.host,
                &s,
                &p,
                &sys,
                "Я вернулся домой!",
                &engine.tools,
                Vec::new(),
                Some(&engine.gemini_pool),
            )
            .await
            .unwrap_or_else(|_| "[happy] С возвращением домой! Всё на своих местах.".into())
        };
        let clean = crate::sfx::strip_tags(&text);
        engine.journal.note("присутствие", "Возвращение домой", &clean);
        let speak_payload = format!("[sfx:chime] {text}");
        let e = engine.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let _ = e.say_aloud(&speak_payload, Duration::from_secs(30));
        })
        .await;
    }
}

pub fn valid_lan_ipv4(raw: &str) -> Option<Ipv4Addr> {
    let ip: Ipv4Addr = raw.trim().parse().ok()?;
    if ip.is_private() && !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified() {
        Some(ip)
    } else {
        None
    }
}

fn probe_ip(ip: &str) -> bool {
    let ping_ok = std::process::Command::new("ping")
        .args(["-c", "1", "-W", "1", ip])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|st| st.success());
    if ping_ok {
        return true;
    }
    arp_has_complete(ip)
}

fn arp_has_complete(ip: &str) -> bool {
    let Ok(arp) = std::fs::read_to_string("/proc/net/arp") else {
        return false;
    };
    arp_line_reachable(&arp, ip)
}

fn arp_line_reachable(arp_table: &str, ip: &str) -> bool {
    for line in arp_table.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 4 && cols[0] == ip {
            let flags = cols[2];
            let mac = cols[3];
            return flags == "0x2" && mac != "00:00:00:00:00:00";
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_private_ipv4_and_arp_parsing() {
        assert!(valid_lan_ipv4("192.168.1.50").is_some());
        assert!(valid_lan_ipv4("10.0.0.15").is_some());
        assert!(valid_lan_ipv4("127.0.0.1").is_none());
        assert!(valid_lan_ipv4("8.8.8.8").is_none());
        assert!(valid_lan_ipv4("not-an-ip").is_none());

        let sample = "IP address       HW type     Flags       HW address            Mask     Device\n\
192.168.1.1      0x1         0x2         50:ff:20:85:80:1a     *        wlan0\n\
192.168.1.50     0x1         0x0         00:00:00:00:00:00     *        wlan0\n";
        assert!(arp_line_reachable(sample, "192.168.1.1"));
        assert!(!arp_line_reachable(sample, "192.168.1.50"));
    }
}
