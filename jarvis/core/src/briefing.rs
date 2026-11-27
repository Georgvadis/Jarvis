//! Персональный конструктор брифинга (погода, расписание/таймеры, текущая книга, заметки CyberNotes).

use crate::engine::Engine;
use serde_json::Value;
use std::{sync::Arc, time::Duration};

const DAYS_RU: [&str; 7] = [
    "воскресенье",
    "понедельник",
    "вторник",
    "среда",
    "четверг",
    "пятница",
    "суббота",
];

pub fn is_briefing_command(text: &str) -> bool {
    let low = text.trim().to_lowercase();
    [
        "утренний брифинг",
        "дневной брифинг",
        "вечерний брифинг",
        "дай брифинг",
        "включи брифинг",
        "запусти брифинг",
        "расскажи сводку",
        "утренняя сводка",
    ]
    .iter()
    .any(|p| low.contains(p))
}

fn encode_city_path(city: &str) -> String {
    let mut out = String::with_capacity(city.len() * 2);
    for b in city.trim().as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_' | b'.') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub async fn fetch_weather_fast(city: &str, tz: &str) -> Result<String, String> {
    let clean = city.trim();
    let effective = if !clean.is_empty() {
        clean.to_string()
    } else if let Some(tz_city) = tz.split('/').next_back().filter(|s| !s.is_empty() && *s != "UTC") {
        tz_city.replace('_', " ")
    } else {
        String::new()
    };
    let path = if effective.is_empty() {
        String::new()
    } else {
        encode_city_path(&effective)
    };
    let url = format!(
        "https://wttr.in/{path}?format=%l:+%C,+%t+(ощущается+%f),+ветер+%w,+влажность+%h&lang=ru"
    );
    let resp = crate::net::direct()
        .get(&url)
        .timeout(Duration::from_secs(4))
        .send()
        .await
        .map_err(|e| format!("Погодный сервис не ответил: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Погодный сервис вернул HTTP {}", resp.status().as_u16()));
    }
    let body = resp.text().await.map_err(|e| e.to_string())?;
    let line = body.lines().next().unwrap_or("").trim().to_string();
    if line.is_empty()
        || line.len() > 220
        || line.contains('<')
        || line.to_lowercase().contains("unknown location")
    {
        return Err("Не удалось распознать город для прогноза погоды.".into());
    }
    Ok(line)
}

async fn fetch_weather(city: &str) -> Option<String> {
    let clean = city.trim();
    if clean.is_empty() {
        return None;
    }
    fetch_weather_fast(clean, "").await.ok()
}

async fn fetch_book_summary(engine: &Engine, srv: &str) -> Option<String> {
    if let Some(reader) = &engine.tools.reader {
        let st = reader.status();
        let title = st["title"].as_str().unwrap_or("").trim();
        if !title.is_empty() {
            let ch = st["chapter"].as_u64().unwrap_or(0) + 1;
            return Some(format!("читаете «{title}» (глава {ch})"));
        }
    }
    let resp = crate::net::direct()
        .get(format!("{srv}/books"))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    let first = v["books"].as_array()?.first()?;
    let title = first["title"].as_str().unwrap_or("").trim();
    if title.is_empty() {
        return None;
    }
    let ch = first["chapter"].as_u64().unwrap_or(0) + 1;
    let pct = first["progress_pct"].as_u64().unwrap_or(0);
    Some(format!("последняя книга «{title}» (глава {ch}, {pct}%)"))
}

async fn fetch_notes_summary(srv: &str) -> Option<String> {
    let resp = crate::net::direct()
        .get(format!("{srv}/notes/list?limit=3"))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    let arr = v["recent"].as_array()?;
    let titles: Vec<String> = arr
        .iter()
        .filter_map(|it| it["title"].as_str())
        .map(|s| format!("«{}»", s.trim()))
        .filter(|s| s.len() > 2)
        .take(3)
        .collect();
    if titles.is_empty() {
        None
    } else {
        Some(format!("свежие заметки: {}", titles.join(", ")))
    }
}

async fn build_briefing_pair(engine: &Arc<Engine>, extra_hint: &str) -> (String, String) {
    let s = engine.store.get();
    let p = s.active_profile();
    let now = s.now();
    let wd = DAYS_RU[now.weekday().to_sunday_zero_offset() as usize];
    let when_str = format!("{}, {}", now.strftime("%d.%m.%Y %H:%M"), wd);
    let srv = crate::tools::local_voice_url(&engine.host);

    let weather_fut = async {
        if s.briefing_weather {
            fetch_weather(&s.city).await
        } else {
            None
        }
    };
    let book_fut = async {
        if s.briefing_books {
            fetch_book_summary(engine, &srv).await
        } else {
            None
        }
    };
    let notes_fut = async {
        if s.briefing_notes && s.cybernotes_enabled {
            fetch_notes_summary(&srv).await
        } else {
            None
        }
    };
    let (weather, book, notes) = tokio::join!(weather_fut, book_fut, notes_fut);

    let schedule = if s.briefing_schedule {
        engine
            .tools
            .scheduler
            .as_ref()
            .map(|sc| sc.today_summary())
            .filter(|t| !t.is_empty())
    } else {
        None
    };

    let mut facts = vec![format!("Сейчас: {when_str}.")];
    if let Some(w) = &weather {
        facts.push(format!("Погода — {w}."));
    }
    if let Some(sc) = &schedule {
        facts.push(format!("Дела и таймеры — {sc}."));
    }
    if let Some(b) = &book {
        facts.push(format!("Чтение — {b}."));
    }
    if let Some(n) = &notes {
        facts.push(format!("CyberNotes — {n}."));
    }

    let mut topic_hint = String::new();
    let custom = s.briefing_custom.trim();
    if !custom.is_empty() {
        topic_hint.push_str(&format!("\nОбязательная персональная тема: {custom}."));
    }
    let extra = extra_hint.trim();
    if !extra.is_empty() {
        topic_hint.push_str(&format!("\nДополнительное пожелание: {extra}."));
    }

    let base_prompt = s.prompt_text(&p.prompt);
    let sys_prompt = format!(
        "{base_prompt}\nТы проводишь краткий, живой голосовой брифинг для хозяина на основе реальных данных. \
        Отвечай по-русски, естественно, без списков и без Markdown, 4–6 коротких предложений (максимум 75 слов)."
    );
    let user_prompt = format!(
        "Собери бодрый брифинг по этим данным:\n{}{}",
        facts.join("\n"),
        topic_hint
    );

    let fallback_text = facts.join(" ");
    let raw = match crate::llm::ask_with_pool(
        &engine.host,
        &s,
        &p,
        &sys_prompt,
        &user_prompt,
        &engine.tools,
        Vec::new(),
        Some(&engine.gemini_pool),
    )
    .await
    {
        Ok(ans) if !ans.trim().is_empty() => ans.trim().to_string(),
        _ => fallback_text.clone(),
    };
    let clean = crate::sfx::strip_tags(&raw);
    let clean = if clean.trim().is_empty() { fallback_text } else { clean };
    (raw, clean)
}

pub async fn generate_briefing_text(engine: &Arc<Engine>, extra_hint: &str) -> Result<String, String> {
    let (_raw, clean) = build_briefing_pair(engine, extra_hint).await;
    Ok(clean)
}

pub async fn run_briefing(engine: Arc<Engine>, extra_hint: &str, speak: bool) -> Result<String, String> {
    let (raw, clean) = build_briefing_pair(&engine, extra_hint).await;
    engine.journal.note("брифинг", "Персональная сводка", &clean);
    if speak {
        // Если настроено 2+ персонажа — делаем мини-радиошоу: второй персонаж добавляет короткую реплику в своём характере
        let s = engine.store.get();
        let active_p = s.active_profile();
        let cohost = if s.profiles.len() > 1 {
            s.profiles.iter().find(|p| p.id != active_p.id).cloned()
        } else {
            None
        };
        let cohost_line = if let Some(ref cp) = cohost {
            let sys = format!(
                "{}\nТы соведущий утреннего/дневного радиошоу на умной колонке. Твой напарник ({}) только что зачитал сводку. \
                 Добавь короткий живой комментарий или пожелание в своём характере (1–2 предложения, без markdown).",
                s.prompt_text(&cp.prompt),
                active_p.name
            );
            crate::llm::ask_with_pool(
                &engine.host,
                &s,
                cp,
                &sys,
                &format!("Сводка напарника:\n{clean}"),
                &engine.tools,
                Vec::new(),
                Some(&engine.gemini_pool),
            )
            .await
            .ok()
            .map(|t| crate::sfx::strip_tags(&t).trim().to_string())
            .filter(|t| !t.is_empty())
        } else {
            None
        };
        let eng = engine.clone();
        tokio::task::spawn_blocking(move || {
            eng.say_text(&raw);
            if let (Some(cp), Some(line)) = (cohost, cohost_line) {
                let _ = eng.say_aloud_opts(&line, Duration::from_secs(25), Some(&cp), false);
            }
        });
    }
    Ok(clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_briefing_voice_commands() {
        assert!(is_briefing_command("Джарвис, включи утренний брифинг"));
        assert!(is_briefing_command("расскажи сводку на сегодня"));
        assert!(is_briefing_command("Дай брифинг"));
        assert!(!is_briefing_command("включи музыку"));
    }

    #[test]
    fn encodes_cyrillic_city_names() {
        assert_eq!(encode_city_path("Москва"), "%D0%9C%D0%BE%D1%81%D0%BA%D0%B2%D0%B0");
        assert_eq!(encode_city_path("New York"), "New%20York");
    }
}
