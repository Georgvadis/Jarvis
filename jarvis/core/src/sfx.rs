//! Выразительная речь: парсинг тегов эмоций `[happy]` / `[whisper]`, сцен `[scene:tavern]`,
//! многоголосовых реплик `[Имя]: текст` и процедурный синтез коротких SFX `[sfx:chime]` (24 кГц PCM16).

use std::f32::consts::TAU;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Sfx(String),
    Scene(String),
    Speech {
        text: String,
        emotion: Option<String>,
        speaker: Option<String>,
    },
}

pub fn normalize_emotion(raw: &str) -> Option<&'static str> {
    match raw.trim().to_lowercase().as_str() {
        "happy" | "joy" | "радостно" | "радость" | "весело" => Some("happy"),
        "sarcastic" | "ironic" | "сарказм" | "ирония" => Some("sarcastic"),
        "whisper" | "whispers" | "шёпот" | "шепот" | "тихо" => Some("whisper"),
        "excited" | "восторг" | "азарт" | "бодро" => Some("excited"),
        "sad" | "грустно" | "грусть" | "печально" => Some("sad"),
        "angry" | "злость" | "сердито" | "строго" => Some("angry"),
        "calm" | "спокойно" => Some("calm"),
        _ => None,
    }
}

pub fn emotion_led_color(emotion: &str) -> Option<&'static str> {
    match emotion {
        "happy" => Some("#22c55e"),
        "excited" => Some("#f59e0b"),
        "sarcastic" => Some("#a855f7"),
        "whisper" => Some("#6366f1"),
        "sad" => Some("#3b82f6"),
        "angry" => Some("#ef4444"),
        "calm" => Some("#14b8a6"),
        _ => None,
    }
}

pub fn emotion_speed_gain(emotion: &str) -> (f64, f32) {
    match emotion {
        "whisper" => (0.94, 0.42),
        "excited" => (1.07, 1.08),
        "happy" => (1.03, 1.03),
        "sarcastic" => (0.96, 1.0),
        "sad" => (0.90, 0.85),
        "angry" => (1.05, 1.10),
        "calm" => (0.94, 0.90),
        _ => (1.0, 1.0),
    }
}

/// Детерминированный цвет LED-кольца и сдвиг темпа для произвольного персонажа (RPG / Детектив / Книга).
pub fn speaker_style(name: &str) -> (f64, &'static str) {
    const SPEEDS: &[f64] = &[0.91, 1.08, 0.96, 1.13, 0.87, 1.03];
    const COLORS: &[&str] = &["#f59e0b", "#38bdf8", "#ec4899", "#22c55e", "#a855f7", "#ef4444"];
    let hash = name.bytes().fold(5381u32, |h, b| h.wrapping_mul(33).wrapping_add(b as u32)) as usize;
    (SPEEDS[hash % SPEEDS.len()], COLORS[hash % COLORS.len()])
}

/// Разобрать текст ответа на последовательность SFX, смены сцен и реплик с эмоциями/спикерами.
pub fn parse_segments(input: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut cur_emotion: Option<String> = None;

    for raw_line in input.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let mut line_speaker: Option<String> = None;
        if let Some(after_open) = line.strip_prefix('[')
            && let Some(close_idx) = after_open.find("]:")
        {
            let tag = after_open[..close_idx].trim();
            if !tag.is_empty()
                && !tag.contains(':')
                && normalize_emotion(tag).is_none()
                && tag.chars().count() <= 40
            {
                line_speaker = Some(tag.to_string());
                line = after_open[close_idx + 2..].trim();
            }
        }

        let mut buf = String::new();
        let bytes = line.as_bytes();
        let mut i = 0;
        let flush_speech = |buf: &mut String, out: &mut Vec<Segment>, emo: &Option<String>, spk: &Option<String>| {
            let trimmed = buf.trim();
            if !trimmed.is_empty() {
                out.push(Segment::Speech {
                    text: trimmed.to_string(),
                    emotion: emo.clone(),
                    speaker: spk.clone(),
                });
            }
            buf.clear();
        };

        while i < bytes.len() {
            if bytes[i] == b'['
                && let Some(rel_end) = line[i + 1..].find(']')
                && rel_end <= 40
            {
                let inner = line[i + 1..i + 1 + rel_end].trim();
                let low = inner.to_lowercase();
                if let Some(sfx_name) = low.strip_prefix("sfx:").map(str::trim) {
                    flush_speech(&mut buf, &mut out, &cur_emotion, &line_speaker);
                    if !sfx_name.is_empty() {
                        out.push(Segment::Sfx(sfx_name.to_string()));
                    }
                    i += rel_end + 2;
                    continue;
                }
                if let Some(scene_name) = low.strip_prefix("scene:").or_else(|| low.strip_prefix("сцена:")).map(str::trim) {
                    flush_speech(&mut buf, &mut out, &cur_emotion, &line_speaker);
                    if !scene_name.is_empty() {
                        out.push(Segment::Scene(scene_name.to_string()));
                    }
                    i += rel_end + 2;
                    continue;
                }
                if let Some(norm) = normalize_emotion(inner) {
                    flush_speech(&mut buf, &mut out, &cur_emotion, &line_speaker);
                    cur_emotion = Some(norm.to_string());
                    i += rel_end + 2;
                    continue;
                }
            }
            let ch = line[i..].chars().next().unwrap();
            buf.push(ch);
            i += ch.len_utf8();
        }
        flush_speech(&mut buf, &mut out, &cur_emotion, &line_speaker);
    }
    out
}

/// Убрать служебные теги `[happy]`, `[sfx:...]`, `[scene:...]` для чистого текста в чате и Telegram.
pub fn strip_tags(input: &str) -> String {
    let mut lines_out = Vec::new();
    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let mut clean = String::with_capacity(line.len());
        let mut prefix_speaker = String::new();
        let mut work = line;
        if let Some(after_open) = work.strip_prefix('[')
            && let Some(close_idx) = after_open.find("]:")
        {
            let tag = after_open[..close_idx].trim();
            if !tag.is_empty() && !tag.contains(':') && normalize_emotion(tag).is_none() {
                prefix_speaker = format!("{tag}: ");
                work = after_open[close_idx + 2..].trim();
            }
        }
        let bytes = work.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'['
                && let Some(rel_end) = work[i + 1..].find(']')
                && rel_end <= 40
            {
                let inner = work[i + 1..i + 1 + rel_end].trim();
                let low = inner.to_lowercase();
                if low.starts_with("sfx:") || low.starts_with("scene:") || low.starts_with("сцена:") || normalize_emotion(inner).is_some() {
                    i += rel_end + 2;
                    continue;
                }
            }
            let ch = work[i..].chars().next().unwrap();
            clean.push(ch);
            i += ch.len_utf8();
        }
        let collapsed = clean.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
            lines_out.push(format!("{prefix_speaker}{collapsed}"));
        }
    }
    lines_out.join("\n")
}

/// Процедурный синтез звуковых эффектов (24 кГц PCM16).
pub fn generate_sfx(name: &str) -> Option<Vec<i16>> {
    let rate = 24000.0_f32;
    let norm = name.trim().to_lowercase();
    let key = match norm.as_str() {
        "chime" | "динь" | "колокольчик" => "chime",
        "bell" | "звон" | "рында" => "bell",
        "fanfare" | "фанфары" | "победа" | "win" => "fanfare",
        "buzz" | "ошибка" | "гудок" | "fail" => "buzz",
        "tick" | "тик" | "часы" => "tick",
        "thunder" | "гром" | "молния" => "thunder",
        "door" | "дверь" | "стук" => "door",
        "sigh" | "вздох" => "sigh",
        "clap" | "аплодисменты" | "хлопок" => "clap",
        "magic" | "магия" | "искра" => "magic",
        "coin" | "монета" => "coin",
        "alert" | "сигнал" => "alert",
        _ => return None,
    };

    Some(match key {
        "chime" => tone_seq(rate, &[(523.25, 0.0, 0.14), (659.25, 0.06, 0.16), (783.99, 0.12, 0.20)], 3800.0),
        "coin" => tone_seq(rate, &[(987.77, 0.0, 0.07), (1318.51, 0.07, 0.18)], 3600.0),
        "alert" => tone_seq(rate, &[(784.0, 0.0, 0.09), (988.0, 0.11, 0.14)], 3400.0),
        "fanfare" => tone_seq(
            rate,
            &[(523.25, 0.0, 0.11), (659.25, 0.10, 0.11), (783.99, 0.20, 0.11), (1046.5, 0.30, 0.30)],
            4200.0,
        ),
        "bell" => {
            let n = (rate * 0.45) as usize;
            (0..n)
                .map(|i| {
                    let t = i as f32 / rate;
                    let env = (-6.5 * t).exp() * (t * 60.0).min(1.0);
                    let s = (TAU * 880.0 * t).sin() * 0.65 + (TAU * 1760.0 * t).sin() * 0.25 + (TAU * 2640.0 * t).sin() * 0.10;
                    (s * env * 4400.0).clamp(-32767.0, 32767.0) as i16
                })
                .collect()
        }
        "buzz" => {
            let n = (rate * 0.32) as usize;
            (0..n)
                .map(|i| {
                    let t = i as f32 / rate;
                    let env = (1.0 - t / 0.32).max(0.0) * (t * 45.0).min(1.0);
                    let s = ((TAU * 155.0 * t).sin() + 0.5 * (TAU * 232.0 * t).sin()).tanh();
                    (s * env * 3400.0) as i16
                })
                .collect()
        }
        "tick" => {
            let n = (rate * 0.05) as usize;
            (0..n)
                .map(|i| {
                    let t = i as f32 / rate;
                    let env = (-80.0 * t).exp();
                    ((TAU * 1450.0 * t).sin() * env * 4800.0) as i16
                })
                .collect()
        }
        "door" => {
            // Двойной глухой деревянный стук в дверь
            let n = (rate * 0.38) as usize;
            (0..n)
                .map(|i| {
                    let t = i as f32 / rate;
                    let pulse = |offset: f32| -> f32 {
                        let dt = t - offset;
                        if dt < 0.0 || dt > 0.12 {
                            0.0
                        } else {
                            (-28.0 * dt).exp() * ((TAU * 145.0 * dt).sin() * 0.7 + (TAU * 310.0 * dt).sin() * 0.3)
                        }
                    };
                    ((pulse(0.0) + pulse(0.17)) * 6800.0).clamp(-32767.0, 32767.0) as i16
                })
                .collect()
        }
        "thunder" => {
            let n = (rate * 0.75) as usize;
            let mut rng = 0x1234_5678_u32;
            let mut lp = 0.0_f32;
            (0..n)
                .map(|i| {
                    let t = i as f32 / rate;
                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let white = ((rng >> 16) as i16 as f32) / 32768.0;
                    lp = lp * 0.96 + white * 0.04;
                    let rumble = (TAU * 52.0 * t).sin() * 0.35 + lp * 2.4;
                    let env = if t < 0.08 { t / 0.08 } else { (-(t - 0.08) * 2.8).exp() };
                    (rumble * env * 9000.0).clamp(-32767.0, 32767.0) as i16
                })
                .collect()
        }
        "sigh" => {
            let n = (rate * 0.42) as usize;
            let mut rng = 0x9E37_79B9_u32;
            let mut prev = 0.0_f32;
            (0..n)
                .map(|i| {
                    let t = i as f32 / rate;
                    let p = i as f32 / n as f32;
                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let white = ((rng >> 16) as i16 as f32) / 32768.0;
                    let bp = (white - prev) * 0.25;
                    prev = prev * 0.88 + white * 0.12;
                    let tone = (TAU * (240.0 - 70.0 * p) * t).sin() * 0.22;
                    let env = (std::f32::consts::PI * p).sin().powf(1.2);
                    ((bp + tone) * env * 3600.0).clamp(-32767.0, 32767.0) as i16
                })
                .collect()
        }
        "clap" => {
            let n = (rate * 0.46) as usize;
            let mut rng = 0xCAFE_BABE_u32;
            (0..n)
                .map(|i| {
                    let t = i as f32 / rate;
                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let white = ((rng >> 16) as i16 as f32) / 32768.0;
                    let hit = |off: f32| {
                        let dt = t - off;
                        if dt >= 0.0 && dt < 0.10 { (-36.0 * dt).exp() } else { 0.0 }
                    };
                    let env = hit(0.0) + hit(0.13) + hit(0.26);
                    (white * env * 4500.0).clamp(-32767.0, 32767.0) as i16
                })
                .collect()
        }
        "magic" => tone_seq(
            rate,
            &[(659.25, 0.0, 0.10), (783.99, 0.06, 0.10), (987.77, 0.12, 0.11), (1318.51, 0.18, 0.16), (1567.98, 0.24, 0.20)],
            3400.0,
        ),
        _ => return None,
    })
}

fn tone_seq(rate: f32, notes: &[(f32, f32, f32)], amp: f32) -> Vec<i16> {
    let total_s = notes.iter().map(|(_, start, dur)| start + dur).fold(0.0_f32, f32::max);
    let len = (rate * total_s) as usize;
    let mut out = vec![0.0_f32; len];
    for &(freq, start, dur) in notes {
        let s0 = (start * rate) as usize;
        let dn = (dur * rate) as usize;
        for j in 0..dn {
            let idx = s0 + j;
            if idx >= len {
                break;
            }
            let t = j as f32 / rate;
            let attack = (t * 65.0).min(1.0);
            let decay = (-5.5 * (t / dur)).exp();
            out[idx] += (TAU * freq * t).sin() * attack * decay * amp;
        }
    }
    out.into_iter().map(|v| v.clamp(-32767.0, 32767.0) as i16).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_emotions_sfx_scenes_and_speakers() {
        let raw = "[scene:tavern][sfx:door] [happy] Добро пожаловать!\n[Бармен]: [сарказм] Опять ты, странник? [sfx:sigh]";
        let segs = parse_segments(raw);
        assert!(matches!(segs[0], Segment::Scene(ref s) if s == "tavern"));
        assert!(matches!(segs[1], Segment::Sfx(ref s) if s == "door"));
        assert!(matches!(segs[2], Segment::Speech { ref text, emotion: Some(ref e), speaker: None } if text == "Добро пожаловать!" && e == "happy"));
        assert!(matches!(segs[3], Segment::Speech { ref text, emotion: Some(ref e), speaker: Some(ref sp) } if text == "Опять ты, странник?" && e == "sarcastic" && sp == "Бармен"));
        assert!(matches!(segs[4], Segment::Sfx(ref s) if s == "sigh"));

        let stripped = strip_tags(raw);
        assert_eq!(stripped, "Добро пожаловать!\nБармен: Опять ты, странник?");
    }

    #[test]
    fn synthesizes_all_sfx_non_empty() {
        for name in ["chime", "bell", "fanfare", "buzz", "tick", "thunder", "door", "sigh", "clap", "magic", "coin", "alert"] {
            let pcm = generate_sfx(name).unwrap_or_else(|| panic!("missing {name}"));
            assert!(pcm.len() >= 1000, "{name} too short");
            assert!(pcm.iter().any(|&v| v.abs() > 50), "{name} silent");
        }
        assert!(generate_sfx("nonexistent_sfx").is_none());
    }
}
