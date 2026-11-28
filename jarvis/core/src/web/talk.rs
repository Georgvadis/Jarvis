//! Разговор из панели: «Поговорить», «Стоп», переписка последних реплик.

use super::{App, ok, reply};
use axum::{extract::State, http::StatusCode, response::Response};
use serde_json::json;

pub async fn preview(
    State(app): State<App>,
    axum::Json(request): axum::Json<crate::engine::voice_preview::Request>,
) -> Response {
    match app.engine.clone().preview(request).await {
        Ok(cached) => reply(StatusCode::OK, json!({"ok": true, "cached": cached})),
        Err(error) if error == crate::engine::INTERRUPTED =>
            reply(StatusCode::OK, json!({"ok": true, "interrupted": true})),
        Err(error) => super::bad(error),
    }
}

pub async fn preview_status(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.engine.preview_status())
}

pub async fn preview_cancel(State(app): State<App>) -> Response {
    app.engine.preview_cancel();
    ok()
}

pub async fn wake_replies(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.engine.wake_reply_status())
}

pub async fn wake_replies_generate(State(app): State<App>) -> Response {
    match app.engine.clone().start_wake_reply_generation() {
        Ok(status) => reply(StatusCode::OK, status),
        Err(error) => super::bad(error),
    }
}

pub async fn wake_replies_cancel(State(app): State<App>) -> Response {
    app.engine.cancel_wake_reply_generation();
    reply(StatusCode::OK, app.engine.wake_reply_status())
}

pub async fn wake_replies_clear(State(app): State<App>) -> Response {
    let removed = app.engine.clear_wake_reply_cache();
    let mut status = app.engine.wake_reply_status();
    if let Some(obj) = status.as_object_mut() {
        obj.insert("removed".into(), json!(removed));
    }
    reply(StatusCode::OK, status)
}
pub async fn say(State(app):State<App>,axum::Json(value):axum::Json<serde_json::Value>)->Response {
    let Some(text)=value["text"].as_str().map(str::trim).filter(|t|!t.is_empty() && t.chars().count()<=16000 && t.len()<=48000)else{return super::bad("Текст: от 1 до 16000 символов");};
    let text=text.to_string();
    let direct=value["direct"].as_bool().unwrap_or(false);
    let speak=value["speak"].as_bool().unwrap_or(true);
    let engine=app.engine.clone();
    if direct {
        engine.journal.note("озвучка", &text, "");
        tokio::task::spawn_blocking(move || { engine.say_text(&text); });
        return ok();
    }
    tokio::spawn(async move{engine.text_turn(text,speak).await;});ok()
}

pub async fn mic_level(State(app): State<App>) -> Response {
    let peak = app.engine.take_mic_peak();
    let threshold = app.store.with(|s| s.vad_threshold);
    reply(StatusCode::OK, json!({"peak": peak, "threshold": threshold, "online": app.engine.mic.online()}))
}

fn local_voice_server(app: &App) -> String {
    crate::tools::local_voice_url(&app.plugins)
}

fn valid_id_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

async fn forward_voice_response(
    resp: Result<reqwest::Response, reqwest::Error>,
    default_err: &str,
) -> Response {
    match resp {
        Ok(r) => {
            let status = r.status();
            let data: serde_json::Value = r.json().await.unwrap_or_else(|_| json!({}));
            if status.is_success() {
                reply(StatusCode::OK, data)
            } else {
                let msg = data["detail"]
                    .as_str()
                    .or(data["error"].as_str())
                    .unwrap_or(default_err);
                super::bad(msg)
            }
        }
        Err(e) => super::bad(format!("Сервер jarvis-voice недоступен: {e}")),
    }
}

pub async fn local_voice_upload(State(app): State<App>, axum::Json(body): axum::Json<serde_json::Value>) -> Response {
    let url = format!("{}/voices/upload-b64", local_voice_server(&app));
    match crate::net::direct().post(&url).timeout(std::time::Duration::from_secs(60)).json(&body).send().await {
        Ok(resp) => {
            let status = resp.status();
            let data: serde_json::Value = resp.json().await.unwrap_or_else(|_| json!({}));
            if status.is_success() {
                reply(StatusCode::OK, data)
            } else {
                let msg = data["detail"].as_str().or(data["error"].as_str()).unwrap_or("Ошибка загрузки голоса");
                super::bad(msg)
            }
        }
        Err(e) => super::bad(format!("Сервер голосов недоступен: {e}")),
    }
}

pub async fn local_voice_upload_raw(
    State(app): State<App>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Response {
    let name = query.get("name").map(|s| s.trim()).unwrap_or("");
    if name.is_empty() || name.chars().count() > 60 {
        return super::bad("Название голоса — от 1 до 60 символов");
    }
    if body.is_empty() || body.len() > 15 * 1024 * 1024 {
        return super::bad("Аудиофайл должен быть от 1 байта до 15 МБ");
    }
    let filename = query.get("filename").map(String::as_str).unwrap_or("sample.wav");
    let ref_text = query.get("ref_text").map(String::as_str).unwrap_or("");
    let language = query.get("language").map(String::as_str).unwrap_or("Russian");
    let url = format!("{}/voices/upload-raw", local_voice_server(&app));
    match crate::net::direct()
        .post(&url)
        .timeout(std::time::Duration::from_secs(60))
        .query(&[
            ("name", name),
            ("filename", filename),
            ("ref_text", ref_text),
            ("language", language),
        ])
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let data: serde_json::Value = resp.json().await.unwrap_or_else(|_| json!({}));
            if status.is_success() {
                reply(StatusCode::OK, data)
            } else {
                let msg = data["detail"].as_str().or(data["error"].as_str()).unwrap_or("Ошибка загрузки голоса");
                super::bad(msg)
            }
        }
        Err(e) => super::bad(format!("Сервер голосов недоступен: {e}")),
    }
}

pub async fn local_voice_delete(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if id.is_empty()
        || id.len() > 64
        || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return super::bad("Неверный ID голоса");
    }
    let url = format!("{}/voices/{}", local_voice_server(&app), id);
    match crate::net::direct().delete(&url).timeout(std::time::Duration::from_secs(15)).send().await {
        Ok(resp) if resp.status().is_success() => ok(),
        Ok(resp) => {
            let data: serde_json::Value = resp.json().await.unwrap_or_else(|_| json!({}));
            let msg = data["detail"].as_str().unwrap_or("Не удалось удалить голос");
            super::bad(msg)
        }
        Err(e) => super::bad(format!("Сервер голосов недоступен: {e}")),
    }
}

pub async fn talk(State(app): State<App>) -> Response {
    app.engine.trigger("кнопка в настройках", 1.0);
    ok()
}

pub async fn stop(State(app): State<App>) -> Response {
    app.engine.tools.stop_all_activities(Some(&app.engine), true);
    let e = app.engine.clone();
    let _ = tokio::task::spawn_blocking(move || e.stop("кнопка в настройках")).await;
    ok()
}

pub async fn activity_payload(app: &App) -> serde_json::Value {
    json!({
        "focus": app.focus.status(),
        "modes": app.modes.status(),
        "games": app.games.status(),
        "dj": app.dj.status(),
        "reader": app.reader.status(),
        "ambient": app.ambient.status(),
        "debate_active": crate::debate::is_active(),
        "presence": app.presence.status(&app.store),
        "timers": app.scheduler.active_timers_json(),
        "custom_scenarios": app.scenarios.custom_summary_json(),
    })
}

pub async fn activity(State(app): State<App>) -> Response {
    reply(StatusCode::OK, activity_payload(&app).await)
}

/// Поток Server-Sent Events (`/api/events`): мгновенно пушит изменения состояния разговора,
/// журнала, активностей и музыки без постоянного 2–4 с опроса панели.
pub async fn events(
    State(app): State<App>,
) -> axum::response::sse::Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use std::time::{Duration, Instant};

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Event>(32);
    tokio::spawn(async move {
        let mut last_journal_sig = String::new();
        let mut last_activity_sig = String::new();
        let mut last_music_sig = String::new();
        let mut last_sys_push = Instant::now() - Duration::from_secs(60);
        let mut last_vol_read = Instant::now() - Duration::from_secs(60);
        let mut cached_vol: i64 = -1;

        loop {
            if tx.is_closed() {
                break;
            }
            if last_vol_read.elapsed() >= Duration::from_secs(3) {
                cached_vol = tokio::task::spawn_blocking(super::vol).await.unwrap_or(cached_vol);
                last_vol_read = Instant::now();
            }
            let e = &app.engine;
            let talking = e.talking();
            let phase = if talking { e.leds.phase().name() } else { "" };
            let mic = e.mic.online();
            let items = e.journal.items(50);
            let items_sig = items
                .last()
                .map(|it| format!("{}:{}:{}:{}:{}:{}", items.len(), it.id, it.ts, it.text.len(), it.answer.len(), it.error.len()))
                .unwrap_or_else(|| "0".into());
            let j_sig = format!("{mic}|{talking}|{phase}|{cached_vol}|{items_sig}");
            let need_sys = last_sys_push.elapsed() >= Duration::from_secs(6);
            if j_sig != last_journal_sig || need_sys {
                last_journal_sig = j_sig;
                last_sys_push = Instant::now();
                let sys = app.telemetry.live_system().await;
                let payload = json!({
                    "items": items,
                    "mic": mic,
                    "mic_why": e.mic.why(),
                    "talking": talking,
                    "phase": phase,
                    "sys": sys,
                    "volume": cached_vol,
                    "version": crate::version(),
                });
                if tx.send(Event::default().event("journal").data(payload.to_string())).await.is_err() {
                    break;
                }
            }

            let act_val = activity_payload(&app).await;
            // Стабильная подпись без секундных тиков (их клиент считает локально)
            let act_sig = format!(
                "{}|{}|{}|{}|{}|{}|{}|{}",
                act_val["focus"]["running"],
                act_val["modes"]["mode"],
                act_val["games"]["active_game"],
                act_val["dj"]["active"],
                act_val["dj"]["current_title"],
                act_val["reader"]["running"],
                act_val["ambient"]["active"],
                act_val["timers"].as_array().map_or(0, Vec::len),
            );
            if act_sig != last_activity_sig {
                last_activity_sig = act_sig;
                if tx.send(Event::default().event("activity").data(act_val.to_string())).await.is_err() {
                    break;
                }
            }

            let ms = app.engine.music.state();
            let cur_id = ms.cur["ID"].as_str().or_else(|| ms.cur["id"].as_str()).unwrap_or("");
            let cur_title = ms.cur["Title"].as_str().or_else(|| ms.cur["title"].as_str()).unwrap_or("");
            let m_sig = format!("{}|{}|{}|{}", ms.playing, ms.paused, cur_id, cur_title);
            if m_sig != last_music_sig {
                last_music_sig = m_sig;
                let m_payload = json!({
                    "playing": ms.playing,
                    "paused": ms.paused,
                    "id": cur_id,
                    "title": cur_title,
                });
                if tx.send(Event::default().event("music").data(m_payload.to_string())).await.is_err() {
                    break;
                }
            }

            let step = if talking { Duration::from_millis(250) } else { Duration::from_millis(600) };
            tokio::time::sleep(step).await;
        }
    });

    let stream = futures_util::stream::poll_fn(move |cx| {
        match rx.poll_recv(cx) {
            std::task::Poll::Ready(Some(ev)) => std::task::Poll::Ready(Some(Ok(ev))),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}

pub async fn scenario_run(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let id = value["id"].as_str().unwrap_or("");
    match crate::tools::run_scenario(app.engine.clone(), id) {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg})),
        Err(e) => super::bad(e),
    }
}

fn parse_scenario_input(value: &serde_json::Value) -> Result<(String, String, String, Vec<crate::scenarios::ScenarioStep>), String> {
    let name = value["name"].as_str().unwrap_or("").to_string();
    let phrase = value["phrase"].as_str().unwrap_or("").to_string();
    let icon = value["icon"].as_str().unwrap_or("sparkles").to_string();
    let arr = value["steps"].as_array().ok_or("Нужен массив шагов")?;
    let mut steps = Vec::with_capacity(arr.len());
    for item in arr {
        steps.push(crate::scenarios::ScenarioStep {
            kind: item["kind"].as_str().unwrap_or("").to_string(),
            value: item["value"].as_str().unwrap_or("").to_string(),
            number: item.get("number").and_then(serde_json::Value::as_i64),
        });
    }
    Ok((name, phrase, icon, steps))
}

pub async fn scenarios_list(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.scenarios.list_json())
}

pub async fn scenarios_create(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let (name, phrase, icon, steps) = match parse_scenario_input(&value) {
        Ok(parsed) => parsed,
        Err(e) => return super::bad(e),
    };
    match app.scenarios.create(name, phrase, icon, steps) {
        Ok(list) => reply(StatusCode::OK, list),
        Err(e) => super::bad(e),
    }
}

pub async fn scenarios_update(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Json(value): axum::Json<serde_json::Value>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID сценария");
    }
    let (name, phrase, icon, steps) = match parse_scenario_input(&value) {
        Ok(parsed) => parsed,
        Err(e) => return super::bad(e),
    };
    match app.scenarios.update(&id, name, phrase, icon, steps) {
        Ok(list) => reply(StatusCode::OK, list),
        Err(e) => super::bad(e),
    }
}

pub async fn scenarios_delete(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID сценария");
    }
    match app.scenarios.delete(&id) {
        Ok(list) => reply(StatusCode::OK, list),
        Err(e) => super::bad(e),
    }
}

pub async fn scenarios_generate(
    State(app): State<App>,
    axum::Json(value): axum::Json<serde_json::Value>,
) -> Response {
    let prompt = value["prompt"].as_str().unwrap_or("").trim().to_string();
    let sys = r##"Ты конструктор сценариев для умной колонки Jarvis. Собери сценарий из 2–6 шагов по запросу пользователя.
Верни ТОЛЬКО валидный JSON без пояснений:
{"name":"Короткое название (2–4 слова)","phrase":"короткая голосовая фраза запуска","icon":"sparkles|sun|moon|flame|briefcase|coffee|bolt|music|monitor|lock|radio|home|heart|gamepad","steps":[{"kind":"...","value":"...","number":null}]}
Допустимые kind:
- "stop_all": остановить всё (value:"", number:null)
- "volume": громкость колонки 0..10 (value:"", number:5)
- "ambient": фон ("rain"|"fireplace"|"ocean"|"white"|"lofi"|"jazz"|"chill", number: минуты сна или null)
- "dj": AI-DJ волна (value:"настроение", number:null)
- "music": музыка YouTube (value:"запрос трека/жанра", number:null)
- "focus": фокус-таймер (value:"", number:25)
- "timer": таймер (value:"метка", number:10)
- "wait": пауза в секундах (value:"", number:60)
- "say": произнести фразу (value:"текст", number:null)
- "ai": спросить ИИ и озвучить (value:"промпт для ИИ", number:null)
- "briefing": сводка дня (value:"акцент или пусто", number:null)
- "alice": команда умному дому / Алисе (value:"выключи свет", number:null)
- "telegram": отправить в Telegram (value:"текст сообщения", number:null)
- "led": подсветка кольца (value:"#ff9900" или "off", number:минуты)
- "sunrise": умный рассвет (value:"", number:минуты 1..30)
- "course": запустить урок курса (value:"название курса или пусто", number:null)
- "mode": режим ("rpg:тема"|"quiz:тема"|"chat"|"translator:английский"|"normal", number:null)
- "pc": действие на ПК (value:"media_play_pause"|"media_next"|"media_prev"|"volume_mute"|"lock_screen"|"sleep_timer"|"screenshot_save", number:минуты для sleep_timer)"##;
    let user_msg = if prompt.is_empty() {
        format!("Придумай оригинальный полезный сценарий для дома (вариант #{})", crate::settings::rand_hex(2))
    } else {
        prompt
    };
    let s = app.store.get();
    let p = s.active_profile();
    let raw = match crate::llm::ask_with_pool(
        &app.plugins,
        &s,
        &p,
        sys,
        &user_msg,
        &app.engine.tools,
        Vec::new(),
        Some(&app.engine.gemini_pool),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return super::bad(format!("Не удалось сгенерировать сценарий: {e}")),
    };
    let trimmed = raw.trim();
    let json_slice = trimmed
        .find('{')
        .and_then(|a| trimmed.rfind('}').filter(|&b| b > a).map(|b| &trimmed[a..=b]))
        .unwrap_or(trimmed);
    let parsed: serde_json::Value = match serde_json::from_str(json_slice) {
        Ok(v) => v,
        Err(_) => return super::bad("Модель вернула некорректный формат сценария — попробуйте ещё раз"),
    };
    reply(StatusCode::OK, parsed)
}

pub async fn modes_get(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.modes.status())
}

pub async fn courses_list(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.courses.list_json())
}

pub async fn courses_generate(
    State(app): State<App>,
    axum::Json(value): axum::Json<serde_json::Value>,
) -> Response {
    let topic = value["topic"].as_str().unwrap_or("").trim().to_string();
    let level = value["level"].as_str().unwrap_or("Начинающий").trim().to_string();
    let goal = value["goal"].as_str().unwrap_or("").trim().to_string();
    if topic.is_empty() {
        return super::bad("Укажите предмет или тему курса");
    }
    let sys = r##"Ты методист и составитель голосовых учебных курсов. Построй чёткую, прогрессивную программу обучения из 10 уроков (от простого к сложному) и 6 стартовых карточек для запоминания.
Верни ТОЛЬКО валидный JSON без пояснений и markdown:
{"title":"Короткое название курса","subject":"Предмет","level":"Уровень","goal":"Цель в 1 предложении","bilingual":true,"lessons":[{"title":"Название урока","goal":"Чему научимся","key_points":["пункт 1","пункт 2","пункт 3"]}],"cards":[{"front":"слово / вопрос","back":"перевод / ответ"}]}
Ставь "bilingual": true, если курс по изучению иностранного языка (английский, японский, немецкий и т.д.), иначе false."##;
    let user_msg = format!(
        "Предмет/тема: {topic}. Уровень: {level}.{}",
        if goal.is_empty() { String::new() } else { format!(" Цель: {goal}.") }
    );
    let s = app.store.get();
    let p = s.active_profile();
    let raw = match crate::llm::ask_with_pool(
        &app.plugins,
        &s,
        &p,
        sys,
        &user_msg,
        &app.engine.tools,
        Vec::new(),
        Some(&app.engine.gemini_pool),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return super::bad(format!("Не удалось составить курс: {e}")),
    };
    let trimmed = raw.trim();
    let json_slice = trimmed
        .find('{')
        .and_then(|a| trimmed.rfind('}').filter(|&b| b > a).map(|b| &trimmed[a..=b]))
        .unwrap_or(trimmed);
    let parsed: serde_json::Value = match serde_json::from_str(json_slice) {
        Ok(v) => v,
        Err(_) => return super::bad("Модель вернула некорректный JSON — попробуйте ещё раз"),
    };
    let lessons: Vec<crate::courses::Lesson> = parsed["lessons"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .take(20)
                .filter_map(|l| {
                    let t = l["title"].as_str()?.trim();
                    if t.is_empty() {
                        return None;
                    }
                    let key_points = l["key_points"]
                        .as_array()
                        .map(|kp| kp.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).take(6).collect())
                        .unwrap_or_default();
                    Some(crate::courses::Lesson {
                        title: t.chars().take(100).collect(),
                        goal: l["goal"].as_str().unwrap_or("").trim().chars().take(200).collect(),
                        key_points,
                        status: "todo".into(),
                        notes: String::new(),
                        completed_at: 0,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let cards: Vec<crate::courses::ReviewCard> = parsed["cards"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .take(20)
                .filter_map(|c| {
                    let f = c["front"].as_str()?.trim();
                    let b = c["back"].as_str()?.trim();
                    if f.is_empty() || b.is_empty() {
                        return None;
                    }
                    Some(crate::courses::ReviewCard {
                        front: f.chars().take(120).collect(),
                        back: b.chars().take(200).collect(),
                        interval_days: 1,
                        next_review: 0,
                        hits: 0,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let course = crate::courses::Course {
        id: String::new(),
        title: parsed["title"].as_str().unwrap_or(&topic).trim().chars().take(80).collect(),
        subject: parsed["subject"].as_str().unwrap_or(&topic).trim().chars().take(60).collect(),
        level: parsed["level"].as_str().unwrap_or(&level).trim().chars().take(40).collect(),
        goal: parsed["goal"].as_str().unwrap_or(&goal).trim().chars().take(200).collect(),
        bilingual: parsed["bilingual"].as_bool().unwrap_or(false),
        current_lesson: 0,
        lessons,
        cards,
        weak_spots: Vec::new(),
        created_at: 0,
        updated_at: 0,
    };
    match app.courses.create(course) {
        Ok(created) => reply(StatusCode::OK, json!({"course": created, "courses": app.courses.list_json()["courses"]})),
        Err(e) => super::bad(e),
    }
}

pub async fn courses_delete(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID курса");
    }
    match app.courses.delete(&id) {
        Ok(list) => reply(StatusCode::OK, list),
        Err(e) => super::bad(e),
    }
}

pub async fn courses_start(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Json(value): axum::Json<serde_json::Value>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID курса");
    }
    let Some(existing) = app.courses.get(&id) else {
        return super::bad("Курс не найден");
    };
    let lesson_idx = value
        .get("lesson_idx")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(existing.current_lesson);
    let (course, idx) = match app.courses.mark_lesson_started(&id, lesson_idx) {
        Ok(v) => v,
        Err(e) => return super::bad(e),
    };
    app.engine.tools.stop_for(&app.engine, "modes");
    let prompt = crate::courses::CourseStore::build_tutor_prompt(&course, idx);
    let title = course.lessons[idx].title.clone();
    let greeting = app.modes.activate(
        crate::modes::SpecialMode::Tutor {
            course_id: course.id.clone(),
            lesson_idx: idx,
            title,
            prompt,
            bilingual: course.bilingual,
        },
        Some(&app.ambient),
    );
    let eng = app.engine.clone();
    tokio::task::spawn_blocking(move || {
        eng.say_text(&greeting);
        if eng.mic.online() {
            eng.trigger("урок репетитора", 1.0);
        }
    });
    reply(StatusCode::OK, json!({"ok": true, "course": course, "mode": app.modes.status()}))
}

pub async fn courses_finish(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Json(value): axum::Json<serde_json::Value>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID курса");
    }
    let Some(course) = app.courses.get(&id) else {
        return super::bad("Курс не найден");
    };
    let lesson_idx = value
        .get("lesson_idx")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(course.current_lesson)
        .min(course.lessons.len().saturating_sub(1));
    let send_tg = value["telegram"].as_bool().unwrap_or(false);

    let history = app.modes.history();
    let mut notes = value["notes"].as_str().unwrap_or("").trim().to_string();
    let mut new_cards: Vec<(String, String)> = Vec::new();
    let mut weak: Vec<String> = Vec::new();

    if !history.is_empty() {
        let mut transcript = String::new();
        for t in history.iter().rev().take(24).rev() {
            let who = if t.role == "user" { "Ученик" } else { "Репетитор" };
            transcript.push_str(&format!("{who}: {}\n", t.content));
        }
        let sys = r#"Подведи краткий итог пройденного голосового урока. Верни ТОЛЬКО JSON:
{"notes":"Краткий конспект в 2–3 предложениях: что разобрали и как справился ученик","cards":[{"front":"новое слово/правило","back":"перевод/объяснение"}],"weak_spots":["слабое место (если было)"]}"#;
        let s = app.store.get();
        let p = s.active_profile();
        if let Ok(raw) = crate::llm::ask_with_pool(
            &app.plugins,
            &s,
            &p,
            sys,
            &transcript,
            &app.engine.tools,
            Vec::new(),
            Some(&app.engine.gemini_pool),
        )
        .await
        {
            let trimmed = raw.trim();
            let slice = trimmed
                .find('{')
                .and_then(|a| trimmed.rfind('}').filter(|&b| b > a).map(|b| &trimmed[a..=b]))
                .unwrap_or(trimmed);
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(slice) {
                if notes.is_empty() && let Some(n) = v["notes"].as_str() {
                    notes = n.trim().to_string();
                }
                if let Some(arr) = v["cards"].as_array() {
                    for c in arr.iter().take(6) {
                        if let (Some(f), Some(b)) = (c["front"].as_str(), c["back"].as_str()) {
                            new_cards.push((f.to_string(), b.to_string()));
                        }
                    }
                }
                if let Some(arr) = v["weak_spots"].as_array() {
                    for w in arr.iter().take(3) {
                        if let Some(ws) = w.as_str() {
                            weak.push(ws.to_string());
                        }
                    }
                }
            }
        }
    }
    if notes.is_empty() {
        notes = format!("Урок «{}» завершён.", course.lessons[lesson_idx].title);
    }

    let updated = match app.courses.complete_lesson(&id, lesson_idx, notes, new_cards, weak) {
        Ok(c) => c,
        Err(e) => return super::bad(e),
    };
    app.modes.reset(Some(&app.ambient));
    let eng = app.engine.clone();
    tokio::task::spawn_blocking(move || eng.end_chat("урок завершён"));
    if send_tg {
        let md = crate::courses::CourseStore::build_summary_markdown(&updated, Some(lesson_idx));
        crate::telegram_bot::notify_telegram(&app.plugins, &app.proxy, &md).await;
    }
    if app.store.with(|s| s.cybernotes_enabled) {
        let md = crate::courses::CourseStore::build_summary_markdown(&updated, Some(lesson_idx));
        let title = format!("Урок {} — {}", lesson_idx + 1, updated.lessons[lesson_idx].title);
        let folder = format!("Обучение/{}", updated.title);
        let _ = crate::tools::save_cybernote(&app.plugins, &title, &md, &["обучение".into(), updated.subject.to_lowercase()], &folder).await;
    }
    reply(StatusCode::OK, json!({"ok": true, "course": updated, "courses": app.courses.list_json()["courses"]}))
}

pub async fn courses_blitz(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID курса");
    }
    let Some(course) = app.courses.get(&id) else {
        return super::bad("Курс не найден");
    };
    app.engine.tools.stop_for(&app.engine, "modes");
    let prompt = crate::courses::CourseStore::build_blitz_prompt(&course);
    let greeting = app.modes.activate(
        crate::modes::SpecialMode::Tutor {
            course_id: course.id.clone(),
            lesson_idx: course.current_lesson,
            title: format!("Блиц: {}", course.title),
            prompt,
            bilingual: course.bilingual,
        },
        Some(&app.ambient),
    );
    let eng = app.engine.clone();
    tokio::task::spawn_blocking(move || {
        eng.say_text(&greeting);
        if eng.mic.online() {
            eng.trigger("блиц по карточкам", 1.0);
        }
    });
    reply(StatusCode::OK, json!({"ok": true, "mode": app.modes.status()}))
}

pub async fn courses_card(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Json(value): axum::Json<serde_json::Value>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID курса");
    }
    let op = value["op"].as_str().unwrap_or("add");
    let res = match op {
        "review" => {
            let idx = value["index"].as_u64().unwrap_or(0) as usize;
            let ok = value["ok"].as_bool().unwrap_or(true);
            app.courses.review_card(&id, idx, ok)
        }
        "delete" => {
            let idx = value["index"].as_u64().unwrap_or(0) as usize;
            app.courses.delete_card(&id, idx)
        }
        _ => {
            let front = value["front"].as_str().unwrap_or("").to_string();
            let back = value["back"].as_str().unwrap_or("").to_string();
            app.courses.add_card(&id, front, back)
        }
    };
    match res {
        Ok(course) => reply(StatusCode::OK, json!({"course": course, "courses": app.courses.list_json()["courses"]})),
        Err(e) => super::bad(e),
    }
}

pub async fn courses_export(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Json(value): axum::Json<serde_json::Value>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID курса");
    }
    let Some(course) = app.courses.get(&id) else {
        return super::bad("Курс не найден");
    };
    let idx = value.get("lesson_idx").and_then(|v| v.as_u64()).map(|n| n as usize);
    let md = crate::courses::CourseStore::build_summary_markdown(&course, idx);
    match crate::telegram_bot::notify_telegram_checked(&app.plugins, &app.proxy, &md).await {
        Ok(()) => reply(StatusCode::OK, json!({"ok": true})),
        Err(e) => super::bad(e),
    }
}

pub async fn modes_set(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let mode = value["mode"].as_str().unwrap_or("").trim().to_lowercase();
    match mode.as_str() {
        "" | "off" | "normal" => {
            app.modes.reset(Some(&app.ambient));
            reply(StatusCode::OK, app.modes.status())
        }
        "bedtime" => {
            app.engine.tools.stop_for(&app.engine, "modes");
            let topic = value["topic"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("уютный маяк у ночного океана");
            let prompt = format!("расскажи историю на ночь про {topic}");
            let eng = app.engine.clone();
            tokio::spawn(async move {
                eng.text_turn(prompt, true).await;
            });
            reply(StatusCode::OK, json!({"mode": "bedtime", "ok": true}))
        }
        "rpg" | "quiz" | "translator" | "custom" => {
            app.engine.tools.stop_for(&app.engine, "modes");
            let topic = value["topic"].as_str().unwrap_or("").trim().to_string();
            let target = match mode.as_str() {
                "rpg" => crate::modes::SpecialMode::Rpg(topic),
                "quiz" => crate::modes::SpecialMode::Quiz(topic),
                "custom" => {
                    let title = value["title"].as_str().unwrap_or("Своя игра").trim().to_string();
                    let prompt = value["prompt"].as_str().unwrap_or("").trim().to_string();
                    if prompt.is_empty() {
                        return super::bad("Опишите правила или сценарий игры");
                    }
                    crate::modes::SpecialMode::Custom { title, prompt }
                }
                _ => {
                    let lang = value["lang"]
                        .as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or("английский")
                        .to_string();
                    crate::modes::SpecialMode::Translator(lang)
                }
            };
            let greeting = app.modes.activate(target, Some(&app.ambient));
            app.engine.journal.note("режим", "", &greeting);
            let eng = app.engine.clone();
            let line = greeting.clone();
            tokio::task::spawn_blocking(move || {
                eng.say_text(&line);
                if eng.mic.online() {
                    eng.trigger("режим из панели", 1.0);
                }
            });
            reply(StatusCode::OK, app.modes.status())
        }
        _ => super::bad("Неизвестный режим"),
    }
}

pub async fn focus_get(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.focus.status())
}

pub async fn focus_set(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let mins = value["minutes"].as_u64().unwrap_or(0);
    if mins == 0 {
        app.focus.stop();
    } else {
        app.focus.start(app.engine.clone(), mins);
    }
    reply(StatusCode::OK, app.focus.status())
}

pub async fn sound_preview(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let theme = value["theme"].as_str().unwrap_or("scifi").to_string();
    let player = app.engine.player.clone();
    tokio::task::spawn_blocking(move || {
        let pcm = crate::classic_voice::thinking_chime_for(&theme);
        if player.start(24000).is_ok() {
            let _ = player.write(&pcm);
            player.finish(std::time::Duration::from_secs(2));
        }
    });
    ok()
}

pub async fn ambient_get(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.ambient.status())
}

pub async fn ambient_set(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    if let Some(vol) = value.get("volume").and_then(|v| v.as_i64()) {
        return reply(StatusCode::OK, app.ambient.set_volume(vol));
    }
    if let Some(mins) = value.get("sleep_min").and_then(|v| v.as_u64()) {
        app.ambient.set_sleep(mins);
        return reply(StatusCode::OK, app.ambient.status());
    }
    let id = value["id"].as_str().unwrap_or("").trim();
    if id.is_empty() || id == "off" || id == "stop" {
        app.ambient.stop();
        return reply(StatusCode::OK, app.ambient.status());
    }
    match app.ambient.start(id) {
        Ok(_) => reply(StatusCode::OK, app.ambient.status()),
        Err(e) => super::bad(e),
    }
}

pub async fn ambient_station_add(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let name = value["name"].as_str().unwrap_or("");
    let url = value["url"].as_str().unwrap_or("");
    match app.ambient.add_station(name, url) {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn ambient_station_delete(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match app.ambient.remove_station(&id) {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn notify(
    State(app): State<App>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    axum::Json(value): axum::Json<serde_json::Value>,
) -> Response {
    let ip = peer.ip();
    if let Err(r) = super::auth::check_rate_limit(&app, ip) {
        return r;
    }
    let expected = app.store.with(|s| s.notify_token.clone());
    if expected.is_empty() {
        return super::bad("Токен уведомлений не задан (настройте на странице «Колонка»)");
    }
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")))
        .or_else(|| headers.get("x-notify-token").and_then(|v| v.to_str().ok()))
        .or_else(|| query.get("token").map(String::as_str))
        .or_else(|| value["token"].as_str())
        .unwrap_or("")
        .trim();
    let eq = provided.len() == expected.len()
        && provided
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |a, (x, y)| a | (x ^ y))
            == 0;
    if !eq {
        super::auth::record_auth_fail(&app, ip);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        return reply(StatusCode::UNAUTHORIZED, json!({"error": "Неверный токен уведомлений"}));
    }
    super::auth::clear_auth_fail(&app, ip);
    let Some(text) = value["text"].as_str().map(str::trim).filter(|t| !t.is_empty() && t.len() <= 4000) else {
        return super::bad("Нужно поле text (1..4000 байт)");
    };
    let force = value["force"].as_bool().unwrap_or(false);
    let chime = value["chime"].as_bool().unwrap_or(true);
    if !force && app.store.is_night() {
        return reply(StatusCode::OK, json!({"ok": true, "skipped": "night_mode"}));
    }
    let text = text.to_string();
    app.engine.journal.note("уведомление", &text, "");
    let engine = app.engine.clone();
    tokio::task::spawn_blocking(move || {
        if chime {
            engine.earcon(true);
        }
        engine.say_text(&text);
    });
    ok()
}

/// Громкость колонки 0..10 из панели.
pub async fn volume(State(app): State<App>, body: axum::body::Bytes) -> Response {
    let Some(lvl) = serde_json::from_slice::<serde_json::Value>(&body).ok().and_then(|v| v.get("level").or(v.get("Level")).and_then(|x| x.as_i64())) else {
        return super::bad("пустой запрос");
    };
    let lvl = lvl.clamp(0, 10);
    match tokio::task::spawn_blocking(move || crate::speakervol::set_alice_volume(lvl)).await.unwrap_or_else(|e| Err(e.to_string())) {
        Ok(()) => {
            crate::engine::flash_volume_clock(&app.engine, lvl);
            reply(StatusCode::OK, json!({"volume": lvl}))
        }
        Err(e) => super::bad(e),
    }
}

pub async fn journal_clear(State(app): State<App>) -> Response {
    app.engine.journal.clear();
    ok()
}

pub async fn debate_start(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let Some(topic) = value["topic"].as_str().map(str::trim).filter(|t| !t.is_empty() && t.chars().count() <= 300 && t.len() <= 1200) else {
        return super::bad("Укажите тему спора (1..300 символов)");
    };
    if crate::debate::is_active() {
        return super::bad("Спор уже идёт");
    }
    let p1_id = value["p1_id"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let p2_id = value["p2_id"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let rounds = value["rounds"].as_u64().unwrap_or(2) as usize;
    crate::debate::spawn_debate(
        app.engine.clone(),
        crate::debate::DebateParams {
            topic: topic.to_string(),
            p1_id,
            p2_id,
            rounds,
        },
    );
    ok()
}

pub async fn gpu_status(State(app): State<App>) -> Response {
    let url = format!("{}/health", local_voice_server(&app));
    match crate::net::direct().get(&url).timeout(std::time::Duration::from_secs(2)).send().await {
        Ok(resp) if resp.status().is_success() => {
            let mut v: serde_json::Value = resp.json().await.unwrap_or_else(|_| json!({}));
            if let Some(obj) = v.as_object_mut() {
                obj.insert("online".into(), json!(true));
            }
            reply(StatusCode::OK, v)
        }
        _ => reply(StatusCode::OK, json!({"online": false})),
    }
}

pub async fn gpu_unload(State(app): State<App>) -> Response {
    let url = format!("{}/unload", local_voice_server(&app));
    match crate::net::direct().post(&url).timeout(std::time::Duration::from_secs(10)).send().await {
        Ok(resp) if resp.status().is_success() => {
            let mut v: serde_json::Value = resp.json().await.unwrap_or_else(|_| json!({}));
            if let Some(obj) = v.as_object_mut() {
                obj.insert("online".into(), json!(true));
            }
            reply(StatusCode::OK, v)
        }
        Ok(resp) => super::bad(format!("GPU-сервер вернул {}", resp.status())),
        Err(e) => super::bad(format!("GPU-сервер недоступен: {e}")),
    }
}

pub async fn ollama_models(State(app): State<App>) -> Response {
    let url = format!("{}/llm/models", local_voice_server(&app));
    match crate::net::direct()
        .get(&url)
        .timeout(std::time::Duration::from_secs(4))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            let v: serde_json::Value = r.json().await.unwrap_or_else(|_| json!({"ok": false, "online": false, "models": []}));
            reply(StatusCode::OK, v)
        }
        _ => reply(StatusCode::OK, json!({"ok": false, "online": false, "models": []})),
    }
}

pub async fn briefing_run(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let speak = value["speak"].as_bool().unwrap_or(true);
    let hint = value["hint"].as_str().unwrap_or("").trim();
    match crate::briefing::run_briefing(app.engine.clone(), hint, speak).await {
        Ok(text) => reply(StatusCode::OK, json!({"ok": true, "text": text})),
        Err(e) => super::bad(e),
    }
}

pub async fn journal(State(app): State<App>) -> Response {
    let (vol, sys) = tokio::join!(
        async { tokio::task::spawn_blocking(super::vol).await.unwrap_or(-1) },
        app.telemetry.live_system(),
    );
    let e = &app.engine;
    let talking = e.talking();
    let phase = if talking { e.leds.phase().name() } else { "" };
    reply(
        StatusCode::OK,
        json!({"items": e.journal.items(50), "mic": e.mic.online(), "mic_why": e.mic.why(), "talking": talking, "phase": phase,
               "sys": sys, "volume": vol, "version": crate::version()}),
    )
}


pub async fn local_voice_emotion_upload(
    State(app): State<App>,
    axum::extract::Path((id, emotion)): axum::extract::Path<(String, String)>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Response {
    if !valid_id_segment(&id) || !valid_id_segment(&emotion) {
        return super::bad("Неверный ID голоса или эмоции");
    }
    if body.is_empty() || body.len() > 15 * 1024 * 1024 {
        return super::bad("Аудиофайл должен быть от 1 байта до 15 МБ");
    }
    let filename = query.get("filename").map(String::as_str).unwrap_or("emotion.wav");
    let ref_text = query.get("ref_text").map(String::as_str).unwrap_or("");
    let url = format!("{}/voices/{}/emotions/{}", local_voice_server(&app), id, emotion);
    let resp = crate::net::direct()
        .post(&url)
        .timeout(std::time::Duration::from_secs(60))
        .query(&[("filename", filename), ("ref_text", ref_text)])
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(body)
        .send()
        .await;
    forward_voice_response(resp, "Ошибка загрузки эмоционального образца").await
}

pub async fn local_voice_emotion_delete(
    State(app): State<App>,
    axum::extract::Path((id, emotion)): axum::extract::Path<(String, String)>,
) -> Response {
    if !valid_id_segment(&id) || !valid_id_segment(&emotion) {
        return super::bad("Неверный ID голоса или эмоции");
    }
    let url = format!("{}/voices/{}/emotions/{}", local_voice_server(&app), id, emotion);
    let resp = crate::net::direct().delete(&url).timeout(std::time::Duration::from_secs(15)).send().await;
    forward_voice_response(resp, "Не удалось удалить эмоцию").await
}

pub async fn notebook_start(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    if crate::debate::is_active() {
        return super::bad("Подкаст или спор уже запущен");
    }
    let url = value["url"].as_str().unwrap_or("").trim().to_string();
    let text = value["text"].as_str().unwrap_or("").trim().to_string();
    let title = value["title"].as_str().unwrap_or("").trim().to_string();
    if url.is_empty() && text.is_empty() {
        return super::bad("Укажите URL (YouTube / статья) или вставьте текст");
    }
    let p1_id = value["p1_id"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let p2_id = value["p2_id"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let turns = value["turns"].as_u64().unwrap_or(6) as usize;
    crate::debate::spawn_notebook(
        app.engine.clone(),
        crate::debate::NotebookParams { url, text, title, p1_id, p2_id, turns },
    );
    ok()
}

pub async fn notebook_upload(
    State(app): State<App>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Response {
    if crate::debate::is_active() {
        return super::bad("Подкаст или спор уже запущен");
    }
    if body.is_empty() || body.len() > 22 * 1024 * 1024 {
        return super::bad("Файл должен быть от 1 байта до 22 МБ");
    }
    let filename = query.get("filename").map(String::as_str).unwrap_or("document.txt");
    let url = format!("{}/extract-file", local_voice_server(&app));
    let resp = match crate::net::direct()
        .post(&url)
        .timeout(std::time::Duration::from_secs(60))
        .query(&[("filename", filename), ("max_chars", "12000")])
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return super::bad(format!("Сервер jarvis-voice недоступен: {e}")),
    };
    let status = resp.status();
    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        return super::bad(data["detail"].as_str().unwrap_or("Не удалось прочитать документ"));
    }
    let text = data["text"].as_str().unwrap_or("").trim().to_string();
    let title = data["title"].as_str().unwrap_or(filename).trim().to_string();
    let p1_id = query.get("p1_id").map(|s| s.trim()).filter(|s| !s.is_empty()).map(str::to_string);
    let p2_id = query.get("p2_id").map(|s| s.trim()).filter(|s| !s.is_empty()).map(str::to_string);
    let turns = query.get("turns").and_then(|s| s.parse::<usize>().ok()).unwrap_or(6).clamp(2, 16);
    crate::debate::spawn_notebook(
        app.engine.clone(),
        crate::debate::NotebookParams {
            url: String::new(),
            text,
            title: title.clone(),
            p1_id,
            p2_id,
            turns,
        },
    );
    reply(StatusCode::OK, json!({"ok": true, "title": title}))
}

pub async fn books_list(State(app): State<App>) -> Response {
    let url = format!("{}/books", local_voice_server(&app));
    let mut base = match crate::net::direct().get(&url).timeout(std::time::Duration::from_secs(6)).send().await {
        Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.unwrap_or_else(|_| json!({"books": []})),
        _ => json!({"ok": false, "books": []}),
    };
    if let Some(obj) = base.as_object_mut() {
        obj.insert("reader".into(), app.reader.status());
    }
    reply(StatusCode::OK, base)
}

pub async fn books_upload(
    State(app): State<App>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Response {
    if body.is_empty() || body.len() > 25 * 1024 * 1024 {
        return super::bad("Книга должна быть от 1 байта до 25 МБ");
    }
    let filename = query.get("filename").map(String::as_str).unwrap_or("book.txt");
    let title = query.get("title").map(String::as_str).unwrap_or("");
    let url = format!("{}/books/upload-raw", local_voice_server(&app));
    let resp = crate::net::direct()
        .post(&url)
        .timeout(std::time::Duration::from_secs(60))
        .query(&[("filename", filename), ("title", title)])
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(body)
        .send()
        .await;
    forward_voice_response(resp, "Не удалось загрузить книгу").await
}

pub async fn books_url(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let url = format!("{}/books/from-url", local_voice_server(&app));
    let resp = crate::net::direct()
        .post(&url)
        .timeout(std::time::Duration::from_secs(45))
        .json(&value)
        .send()
        .await;
    forward_voice_response(resp, "Не удалось загрузить книгу по ссылке").await
}

pub async fn books_play(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let book_id = value["book_id"].as_str().map(str::to_string);
    let multi_voice = value["multi_voice"].as_bool();
    match app.reader.start(app.engine.clone(), book_id, multi_voice).await {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn books_stop(State(app): State<App>) -> Response {
    app.reader.stop();
    let e = app.engine.clone();
    tokio::task::spawn_blocking(move || e.end_chat("пауза книги"));
    reply(StatusCode::OK, app.reader.status())
}

pub async fn books_nav(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let delta = value["delta"].as_i64().unwrap_or(1) as i32;
    match app.reader.step_chapter(app.engine.clone(), delta.clamp(-1, 1)).await {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg, "reader": app.reader.status()})),
        Err(e) => super::bad(e),
    }
}

pub async fn books_recap(State(app): State<App>) -> Response {
    match app.reader.recap(app.engine.clone()).await {
        Ok(msg) => {
            app.engine.journal.note("книга", "Пересказ", &msg);
            let e = app.engine.clone();
            let line = msg.clone();
            tokio::task::spawn_blocking(move || e.say_text(&line));
            reply(StatusCode::OK, json!({"ok": true, "text": msg}))
        }
        Err(e) => super::bad(e),
    }
}

pub async fn books_delete(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID книги");
    }
    if app.reader.status()["book_id"].as_str() == Some(id.as_str()) {
        app.reader.stop();
    }
    let url = format!("{}/books/{}", local_voice_server(&app), id);
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    let resp = client.delete(&url).send().await;
    forward_voice_response(resp, "Не удалось удалить книгу").await
}

pub async fn books_chapters(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID книги");
    }
    let url = format!("{}/books/{}/chapters", local_voice_server(&app), id);
    let resp = crate::net::direct()
        .get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await;
    forward_voice_response(resp, "Не удалось загрузить оглавление книги").await
}

pub async fn books_jump(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let book_id = value["book_id"].as_str().map(str::to_string);
    let chapter = value["chapter"].as_u64().unwrap_or(0) as usize;
    match app.reader.jump_chapter(app.engine.clone(), book_id, chapter).await {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg, "reader": app.reader.status()})),
        Err(e) => super::bad(e),
    }
}

pub async fn books_sleep(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let mins = value["minutes"].as_u64().unwrap_or(0);
    reply(StatusCode::OK, app.reader.set_sleep(mins))
}

pub async fn books_quote(State(app): State<App>) -> Response {
    match app.reader.save_quote(&app.engine).await {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg})),
        Err(e) => super::bad(e),
    }
}

pub async fn music_sleep(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let mins = value["minutes"].as_u64().unwrap_or(0);
    reply(StatusCode::OK, app.dj.set_music_sleep(app.engine.clone(), mins))
}

pub async fn podcasts_list(State(app): State<App>) -> Response {
    let url = format!("{}/podcasts", local_voice_server(&app));
    let resp = crate::net::direct()
        .get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await;
    forward_voice_response(resp, "Не удалось загрузить архив подкастов").await
}

pub async fn podcasts_get(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID подкаста");
    }
    let url = format!("{}/podcasts/{}", local_voice_server(&app), id);
    let resp = crate::net::direct()
        .get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await;
    forward_voice_response(resp, "Не удалось загрузить выпуск подкаста").await
}

pub async fn podcasts_play(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID подкаста");
    }
    let url = format!("{}/podcasts/{}", local_voice_server(&app), id);
    let resp = match crate::net::direct()
        .get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.unwrap_or_default(),
        Ok(r) => return super::bad(format!("Подкаст не найден (HTTP {})", r.status())),
        Err(e) => return super::bad(format!("Сервер jarvis-voice недоступен: {e}")),
    };
    let title = resp["title"].as_str().unwrap_or("Выпуск").to_string();
    let turns: Vec<(String, String)> = resp["turns"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let txt = t["text"].as_str()?.trim().to_string();
                    if txt.is_empty() {
                        return None;
                    }
                    Some((t["speaker"].as_str().unwrap_or("").to_string(), txt))
                })
                .collect()
        })
        .unwrap_or_default();
    match crate::debate::spawn_replay_podcast(app.engine.clone(), title, turns, None, None) {
        Ok(()) => ok(),
        Err(e) => super::bad(e),
    }
}

pub async fn podcasts_mp3(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID подкаста");
    }
    let url = format!("{}/podcasts/{}/mp3", local_voice_server(&app), id);
    let resp = match crate::net::direct()
        .get(&url)
        .timeout(std::time::Duration::from_secs(180))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return super::bad(format!("Сервер jarvis-voice недоступен: {e}")),
    };
    if !resp.status().is_success() {
        let v: serde_json::Value = resp.json().await.unwrap_or_default();
        return super::bad(v["detail"].as_str().unwrap_or("Не удалось собрать MP3 подкаста"));
    }
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return super::bad(format!("Ошибка чтения MP3: {e}")),
    };
    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "audio/mpeg")
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"podcast-{id}.mp3\""),
        )
        .body(axum::body::Body::from(bytes))
        .unwrap_or_else(|_| super::bad("Ошибка формирования ответа MP3"))
}

pub async fn podcasts_telegram(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID подкаста");
    }
    let base = local_voice_server(&app);
    let meta: serde_json::Value = match crate::net::direct()
        .get(format!("{base}/podcasts/{id}"))
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        Ok(r) => return super::bad(format!("Подкаст не найден (HTTP {})", r.status())),
        Err(e) => return super::bad(format!("Сервер jarvis-voice недоступен: {e}")),
    };
    let mp3_resp = match crate::net::direct()
        .get(format!("{base}/podcasts/{id}/mp3"))
        .timeout(std::time::Duration::from_secs(180))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return super::bad(format!("Сервер jarvis-voice недоступен: {e}")),
    };
    if !mp3_resp.status().is_success() {
        let v: serde_json::Value = mp3_resp.json().await.unwrap_or_default();
        return super::bad(v["detail"].as_str().unwrap_or("Не удалось собрать MP3"));
    }
    let bytes = match mp3_resp.bytes().await {
        Ok(b) => b,
        Err(e) => return super::bad(format!("Ошибка чтения MP3: {e}")),
    };
    let title = meta["title"].as_str().unwrap_or("Подкаст YaJarvis");
    match crate::telegram_bot::send_audio_telegram(&app.plugins, &app.proxy, &bytes, &format!("podcast-{id}.mp3"), title).await {
        Ok(()) => ok(),
        Err(e) => super::bad(e),
    }
}

pub async fn podcasts_delete(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID подкаста");
    }
    let url = format!("{}/podcasts/{}", local_voice_server(&app), id);
    let resp = crate::net::direct()
        .delete(&url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await;
    forward_voice_response(resp, "Не удалось удалить выпуск подкаста").await
}

pub async fn games_status(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.games.status())
}

pub async fn games_melody(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let theme = value["theme"].as_str().unwrap_or("аниме опенинги").to_string();
    let rounds = value["rounds"].as_u64().unwrap_or(5) as usize;
    let snippet_s = value["snippet_s"].as_u64().unwrap_or(8) as u8;
    let seek_s = if value["from_start"].as_bool().unwrap_or(false) { 0.0 } else { 30.0 };
    match app.games.start_melody_opts(app.engine.clone(), theme, rounds, snippet_s, seek_s, true).await {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn games_melody_skip(State(app): State<App>) -> Response {
    match app.games.skip_melody(&app.engine).await {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

/// Генератор свежих идей для любого игрового режима и конструктора своей игры.
pub async fn games_idea(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let kind = value["kind"].as_str().unwrap_or("custom").trim().to_lowercase();
    let hint = value["hint"].as_str().unwrap_or("").trim().to_string();
    let sys = match kind.as_str() {
        "melody" => "Придумай одну оригинальную и весёлую тему для музыкальной игры «Угадай мелодию» (2–5 слов, без кавычек). Верни ТОЛЬКО JSON: {\"theme\":\"...\"}",
        "rpg" => "Придумай необычный и атмосферный сеттинг для голосового RPG-квеста (3–8 слов). Верни ТОЛЬКО JSON: {\"theme\":\"...\"}",
        "quiz" => "Придумай увлекательную и небанальную тему для голосовой викторины (2–6 слов). Верни ТОЛЬКО JSON: {\"theme\":\"...\"}",
        "detective" => "Придумай яркую завязку и сеттинг для нуарного детектива (4–10 слов). Верни ТОЛЬКО JSON: {\"theme\":\"...\"}",
        _ => "Придумай крутую оригинальную голосовую игру для умной колонки. Верни ТОЛЬКО JSON: {\"title\":\"Короткое название\",\"rules\":\"Понятные правила и сеттинг в 2–3 предложениях\"}",
    };
    let user_msg = if hint.is_empty() {
        format!("Удиви меня свежей идеей (вариант #{})", crate::settings::rand_hex(2))
    } else {
        format!("Идея с уклоном в: {hint}")
    };
    let s = app.store.get();
    let p = s.active_profile();
    let raw = match crate::llm::ask_with_pool(
        &app.plugins,
        &s,
        &p,
        sys,
        &user_msg,
        &app.engine.tools,
        Vec::new(),
        Some(&app.engine.gemini_pool),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return super::bad(format!("Не удалось придумать идею: {e}")),
    };
    let trimmed = raw.trim();
    let json_slice = trimmed
        .find('{')
        .and_then(|a| trimmed.rfind('}').filter(|&b| b > a).map(|b| &trimmed[a..=b]))
        .unwrap_or(trimmed);
    let parsed: serde_json::Value = serde_json::from_str(json_slice).unwrap_or_else(|_| {
        json!({ "theme": trimmed.trim_matches('"'), "title": "Своя игра", "rules": trimmed })
    });
    reply(StatusCode::OK, parsed)
}

pub async fn games_detective(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let theme = value["theme"].as_str().unwrap_or("").to_string();
    match app.games.start_detective(app.engine.clone(), theme).await {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn games_party_start(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let game = value["game"].as_str().unwrap_or("mafia").to_string();
    let discussion_s = value["discussion_s"].as_u64().unwrap_or(60);
    let players: Vec<crate::games::PartyPlayer> = match serde_json::from_value(value["players"].clone()) {
        Ok(p) => p,
        Err(_) => return super::bad("Укажите список игроков"),
    };
    match app.games.start_party(app.engine.clone(), game, players, discussion_s).await {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn games_party_next(State(app): State<App>) -> Response {
    match app.games.party_next_phase(&app.engine).await {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg, "games": app.games.status()})),
        Err(e) => super::bad(e),
    }
}

pub async fn games_stop(State(app): State<App>) -> Response {
    app.games.stop_all(Some(&app.engine));
    reply(StatusCode::OK, app.games.status())
}

pub async fn dj_status(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.dj.status())
}

pub async fn dj_start(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let mood = value["mood"].as_str().unwrap_or("ночной Токио синтвейв").to_string();
    match app.dj.start(app.engine.clone(), mood) {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn dj_next(State(app): State<App>) -> Response {
    app.dj.next_with(&app.engine);
    reply(StatusCode::OK, app.dj.status())
}

pub async fn dj_like(State(app): State<App>) -> Response {
    let msg = app.dj.like(&app.engine).await;
    reply(StatusCode::OK, json!({"ok": true, "message": msg, "dj": app.dj.status()}))
}

pub async fn dj_vibe(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let hint = value["hint"].as_str().unwrap_or("").to_string();
    match app.dj.adjust_vibe(app.engine.clone(), hint) {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn dj_stop(State(app): State<App>) -> Response {
    app.dj.stop(Some(&app.engine));
    reply(StatusCode::OK, app.dj.status())
}

pub async fn pc_ping(State(app): State<App>) -> Response {
    let (url, tok) = app.store.with(|s| (s.pc_agent_url.trim().trim_end_matches('/').to_string(), s.pc_agent_token.clone()));
    if url.is_empty() {
        return super::bad("URL ПК-компаньона не задан");
    }
    match crate::net::direct()
        .get(format!("{url}/health"))
        .bearer_auth(tok.trim())
        .timeout(std::time::Duration::from_secs(4))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            let v: serde_json::Value = r.json().await.unwrap_or_default();
            reply(StatusCode::OK, v)
        }
        Ok(r) => super::bad(format!("ПК-компаньон ответил HTTP {}", r.status())),
        Err(e) => super::bad(format!("ПК-компаньон недоступен: {e}")),
    }
}

pub async fn pc_screen(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let q = value["question"].as_str().unwrap_or("Кратко опиши, что сейчас на экране.");
    match crate::tools::capture_and_describe_screen(&app.plugins, &app.store, q).await {
        Ok(ans) => {
            app.engine.journal.note("экран", q, &ans);
            reply(StatusCode::OK, json!({"ok": true, "text": ans}))
        }
        Err(e) => super::bad(e),
    }
}

pub async fn pc_action(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let action = value["action"].as_str().unwrap_or("").trim();
    const ALLOWED: &[&str] = &[
        "media_play_pause",
        "media_next",
        "media_prev",
        "volume_up",
        "volume_down",
        "volume_mute",
        "lock_screen",
        "sleep_timer",
        "cancel_sleep",
        "screenshot_save",
        "active_window",
    ];
    if !ALLOWED.contains(&action) {
        return super::bad("Недопустимое действие ПК");
    }
    let mins = value["minutes"].as_i64().unwrap_or(30).clamp(1, 720);
    match crate::tools::call_pc_action(&app.store, action, mins).await {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg})),
        Err(e) => super::bad(e),
    }
}

pub async fn pc_system(State(app): State<App>) -> Response {
    match crate::tools::pc_system_info(&app.store).await {
        Ok(v) => reply(StatusCode::OK, v),
        Err(e) => super::bad(e),
    }
}

pub async fn pc_macros(State(app): State<App>) -> Response {
    match crate::tools::pc_macros_list(&app.store).await {
        Ok(v) => reply(StatusCode::OK, v),
        Err(e) => super::bad(e),
    }
}

pub async fn pc_macro_run(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let id = value["id"].as_str().unwrap_or("").trim();
    if !valid_id_segment(id) {
        return super::bad("Неверный ID макроса");
    }
    match crate::tools::pc_macro_run(&app.store, id).await {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg})),
        Err(e) => super::bad(e),
    }
}

pub async fn pc_clipboard_get(State(app): State<App>) -> Response {
    match crate::tools::pc_clipboard_read(&app.store).await {
        Ok(text) => reply(StatusCode::OK, json!({"ok": true, "text": text})),
        Err(e) => super::bad(e),
    }
}

pub async fn pc_clipboard_post(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    if value["speak"].as_bool() == Some(true) {
        return match crate::tools::pc_clipboard_read(&app.store).await {
            Ok(text) if text.trim().is_empty() => super::bad("Буфер обмена ПК пуст"),
            Ok(text) => {
                app.engine.journal.note("буфер ПК", "", &text);
                let eng = app.engine.clone();
                let say_copy = text.chars().take(4000).collect::<String>();
                tokio::task::spawn_blocking(move || { eng.say_text(&say_copy); });
                reply(StatusCode::OK, json!({"ok": true, "text": text}))
            }
            Err(e) => super::bad(e),
        };
    }
    let Some(text) = value["text"].as_str().map(str::trim).filter(|t| !t.is_empty() && t.len() <= 16000) else {
        return super::bad("Текст буфера должен быть от 1 до 16000 байт");
    };
    match crate::tools::pc_clipboard_write(&app.store, text).await {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg})),
        Err(e) => super::bad(e),
    }
}

pub async fn pc_open_url(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let url = value["url"].as_str().unwrap_or("");
    match crate::tools::pc_open_url(&app.store, url).await {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg})),
        Err(e) => super::bad(e),
    }
}

pub async fn schedule_get(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.scheduler.status())
}

pub async fn schedule_timer_add(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let label = value["label"].as_str().unwrap_or("Таймер").to_string();
    let secs = value["seconds"]
        .as_u64()
        .or_else(|| value["minutes"].as_u64().map(|m| m.saturating_mul(60)))
        .unwrap_or(300);
    match app.scheduler.add_timer(label, secs) {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn schedule_timer_del(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID таймера");
    }
    let _ = app.scheduler.cancel_timer(&id);
    reply(StatusCode::OK, app.scheduler.status())
}

fn parse_routine_input(value: &serde_json::Value) -> (String, String, Vec<u8>, String, String, Option<i64>) {
    let title = value["title"].as_str().unwrap_or("").to_string();
    let time = value["time"].as_str().unwrap_or("").to_string();
    let action = value["action"].as_str().unwrap_or("say").to_string();
    let payload = value["payload"].as_str().unwrap_or("").to_string();
    let volume = value.get("volume").and_then(serde_json::Value::as_i64).filter(|&v| v > 0);
    let days: Vec<u8> = value["days"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(serde_json::Value::as_u64)
                .filter(|&d| (1..=7).contains(&d))
                .map(|d| d as u8)
                .collect()
        })
        .unwrap_or_default();
    (title, time, days, action, payload, volume)
}

pub async fn schedule_routine_add(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let (title, time, days, action, payload, volume) = parse_routine_input(&value);
    match app.scheduler.add_routine(title, time, days, action, payload, volume) {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn schedule_routine_update(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Json(value): axum::Json<serde_json::Value>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID рутины");
    }
    let (title, time, days, action, payload, volume) = parse_routine_input(&value);
    match app.scheduler.update_routine(&id, title, time, days, action, payload, volume) {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn schedule_routine_toggle(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Json(value): axum::Json<serde_json::Value>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID рутины");
    }
    let enabled = value["enabled"].as_bool().unwrap_or(true);
    match app.scheduler.toggle_routine(&id, enabled) {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn schedule_routine_run(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID рутины");
    }
    match app.scheduler.run_routine_now(app.engine.clone(), &id) {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg})),
        Err(e) => super::bad(e),
    }
}

pub async fn schedule_routine_del(
    State(app): State<App>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    if !valid_id_segment(&id) {
        return super::bad("Неверный ID рутины");
    }
    match app.scheduler.delete_routine(&id) {
        Ok(st) => reply(StatusCode::OK, st),
        Err(e) => super::bad(e),
    }
}

pub async fn presence_status(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.presence.status(&app.store))
}

pub async fn presence_test(State(app): State<App>) -> Response {
    app.presence.greet_home(&app.engine).await;
    ok()
}

pub async fn notes_status(State(app): State<App>) -> Response {
    let url = format!("{}/notes/list?limit=5", local_voice_server(&app));
    match crate::net::direct()
        .get(&url)
        .timeout(std::time::Duration::from_secs(4))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            let v: serde_json::Value = r.json().await.unwrap_or_default();
            reply(StatusCode::OK, v)
        }
        _ => reply(StatusCode::OK, json!({"ok": false, "available": false, "total_notes": 0})),
    }
}

fn valid_note_path(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 300
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains('\0')
        && !p.chars().any(char::is_control)
        && p.to_lowercase().ends_with(".md")
        && p.split('/').all(|seg| !seg.is_empty() && seg != "." && seg != ".." && !seg.starts_with('.'))
}

fn sanitize_audio_ext(raw: &str) -> (&'static str, &'static str) {
    match raw.trim().trim_start_matches('.').to_lowercase().as_str() {
        "ogg" => ("ogg", "audio/ogg"),
        "opus" => ("opus", "audio/opus"),
        "wav" => ("wav", "audio/wav"),
        "mp4" => ("mp4", "audio/mp4"),
        "m4a" => ("m4a", "audio/mp4"),
        "mp3" => ("mp3", "audio/mpeg"),
        _ => ("webm", "audio/webm"),
    }
}

pub(crate) fn build_audio_multipart(boundary: &str, filename: &str, mime: &str, lang: &str, data: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(data.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"audio\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(
        format!(
            "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"language\"\r\n\r\n{lang}\r\n--{boundary}--\r\n"
        )
        .as_bytes(),
    );
    body
}

pub async fn notes_list(
    State(app): State<App>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let limit = query.get("limit").and_then(|s| s.parse::<u32>().ok()).unwrap_or(40).clamp(1, 200);
    let url = format!("{}/notes/list?limit={limit}", local_voice_server(&app));
    let resp = crate::net::direct()
        .get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await;
    forward_voice_response(resp, "Не удалось загрузить заметки CyberNotes").await
}

pub async fn notes_search(
    State(app): State<App>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let q = query.get("q").map(String::as_str).unwrap_or("");
    let limit = query.get("limit").and_then(|s| s.parse::<u32>().ok()).unwrap_or(15).clamp(1, 20);
    let url = format!("{}/notes/search", local_voice_server(&app));
    let resp = crate::net::direct()
        .get(&url)
        .query(&[("q", q), ("limit", &limit.to_string())])
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await;
    forward_voice_response(resp, "Не удалось поискать заметки CyberNotes").await
}

pub async fn notes_read(
    State(app): State<App>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let path = query.get("path").map(String::as_str).unwrap_or("").trim();
    if !valid_note_path(path) {
        return super::bad("Недопустимый путь заметки");
    }
    let url = format!("{}/notes/read", local_voice_server(&app));
    let resp = crate::net::direct()
        .get(&url)
        .query(&[("path", path)])
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await;
    forward_voice_response(resp, "Не удалось прочитать заметку CyberNotes").await
}

pub async fn notes_update(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let path = value["path"].as_str().unwrap_or("").trim();
    if !valid_note_path(path) {
        return super::bad("Недопустимый путь заметки");
    }
    let Some(content) = value["content"].as_str().filter(|c| !c.is_empty() && c.chars().count() <= 100_000 && c.len() <= 256_000) else {
        return super::bad("Текст заметки должен быть от 1 до 100000 символов");
    };
    let url = format!("{}/notes/update", local_voice_server(&app));
    let resp = crate::net::direct()
        .put(&url)
        .json(&json!({"path": path, "content": content}))
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await;
    forward_voice_response(resp, "Не удалось обновить заметку в CyberNotes").await
}

pub async fn notes_delete(
    State(app): State<App>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let path = query.get("path").map(String::as_str).unwrap_or("").trim();
    if !valid_note_path(path) {
        return super::bad("Недопустимый путь заметки");
    }
    let url = format!("{}/notes/delete", local_voice_server(&app));
    let resp = crate::net::direct()
        .delete(&url)
        .query(&[("path", path)])
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await;
    forward_voice_response(resp, "Не удалось удалить заметку CyberNotes").await
}

pub async fn notes_save(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let content = value["content"].as_str().unwrap_or("").trim();
    if content.is_empty() || content.chars().count() > 20000 || content.len() > 64000 {
        return super::bad("Текст заметки должен быть от 1 до 20000 символов");
    }
    let title = value["title"].as_str().unwrap_or("").trim();
    let folder = value["folder"].as_str().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("Голосовые заметки");
    let tags: Vec<String> = value["tags"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(str::to_string).collect())
        .unwrap_or_default();
    let url = format!("{}/notes/save", local_voice_server(&app));
    let resp = crate::net::direct()
        .post(&url)
        .json(&json!({"title": title, "content": content, "folder": folder, "tags": tags}))
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await;
    forward_voice_response(resp, "Не удалось сохранить заметку в CyberNotes").await
}

pub async fn ptt(
    State(app): State<App>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Response {
    if body.is_empty() || body.len() > 8 * 1024 * 1024 {
        return super::bad("Аудио должно быть от 1 байта до 8 МБ");
    }
    let (ext, mime) = sanitize_audio_ext(query.get("ext").map(String::as_str).unwrap_or("webm"));
    let speak = query.get("speak").map(|v| v != "0" && v != "false").unwrap_or(true);
    let direct = query.get("direct").is_some_and(|v| v == "1" || v == "true");
    let lang = app.store.with(|s| {
        let l = s.language.trim();
        if l.is_empty() || l == "auto" { "ru".to_string() } else { l.to_string() }
    });

    let boundary = format!("jarvis-ptt-{}", crate::settings::rand_hex(8));
    let filename = format!("ptt.{ext}");
    let mp_body = build_audio_multipart(&boundary, &filename, mime, &lang, &body);
    let url = format!("{}/stt", local_voice_server(&app));
    let resp = match crate::net::direct()
        .post(&url)
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(mp_body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return super::bad(format!("Сервер jarvis-voice недоступен: {e}")),
    };
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        let msg = v["detail"].as_str().or(v["error"].as_str()).unwrap_or("Ошибка распознавания речи");
        return super::bad(msg);
    }
    let text = v["text"].as_str().unwrap_or("").trim().to_string();
    if text.is_empty() {
        return reply(StatusCode::OK, json!({"ok": false, "text": "", "error": "Речь не распознана"}));
    }

    let engine = app.engine.clone();
    if direct {
        engine.journal.note("озвучка", &text, "");
        let say_copy = text.clone();
        tokio::task::spawn_blocking(move || { engine.say_text(&say_copy); });
    } else {
        let turn_copy = text.clone();
        tokio::spawn(async move { engine.text_turn(turn_copy, speak).await; });
    }
    reply(StatusCode::OK, json!({"ok": true, "text": text}))
}

pub async fn stack_health(State(app): State<App>) -> Response {
    let voice_base = local_voice_server(&app);
    let (pc_url, pc_tok) = app.store.with(|s| (
        s.pc_agent_url.trim().trim_end_matches('/').to_string(),
        s.pc_agent_token.trim().to_string(),
    ));
    let tg_token = app.plugins.plugin_setting_str("telegram-sender", "bot_token");
    let tg_chat = app.plugins.plugin_setting_str("telegram-sender", "chat_id");
    let tg_configured = !tg_token.trim().is_empty() && !tg_chat.trim().is_empty();
    let route = app.proxy.route("plugins");
    let vpn_json = match route {
        crate::vpn::Route::Direct => json!({"mode": "direct", "ok": true}),
        crate::vpn::Route::Tunnel(t) => json!({"mode": "tunnel", "ok": true, "proxy": t.socks}),
        crate::vpn::Route::Blocked(why) => json!({"mode": "blocked", "ok": false, "error": why}),
    };

    let client = crate::net::direct();
    let voice_fut = async {
        let t0 = std::time::Instant::now();
        match client
            .get(format!("{voice_base}/health"))
            .timeout(std::time::Duration::from_millis(2500))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {
                let latency_ms = t0.elapsed().as_millis() as u64;
                let v: serde_json::Value = r.json().await.unwrap_or_default();
                json!({
                    "ok": v["ok"].as_bool().unwrap_or(true),
                    "latency_ms": latency_ms,
                    "whisper_loaded": v["whisper_loaded"].as_bool().unwrap_or(false),
                    "qwen_loaded": v["qwen_loaded"].as_bool().unwrap_or(false),
                    "gpu": v["gpu"].as_str().unwrap_or(""),
                })
            }
            Ok(r) => json!({"ok": false, "error": format!("HTTP {}", r.status())}),
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        }
    };

    let notes_fut = async {
        match client
            .get(format!("{voice_base}/notes/list?limit=1"))
            .timeout(std::time::Duration::from_millis(2500))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {
                let v: serde_json::Value = r.json().await.unwrap_or_default();
                json!({
                    "ok": v["available"].as_bool().unwrap_or(false),
                    "available": v["available"].as_bool().unwrap_or(false),
                    "total_notes": v["total_notes"].as_u64().unwrap_or(0),
                })
            }
            Ok(r) => json!({"ok": false, "available": false, "error": format!("HTTP {}", r.status())}),
            Err(e) => json!({"ok": false, "available": false, "error": e.to_string()}),
        }
    };

    let pc_fut = async {
        if pc_url.is_empty() {
            return json!({"configured": false});
        }
        let t0 = std::time::Instant::now();
        match client
            .get(format!("{pc_url}/health"))
            .bearer_auth(&pc_tok)
            .timeout(std::time::Duration::from_millis(2500))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {
                let latency_ms = t0.elapsed().as_millis() as u64;
                let v: serde_json::Value = r.json().await.unwrap_or_default();
                json!({
                    "configured": true,
                    "ok": true,
                    "latency_ms": latency_ms,
                    "host": v["host"].as_str().unwrap_or(""),
                    "os": v["os"].as_str().unwrap_or(""),
                })
            }
            Ok(r) => json!({"configured": true, "ok": false, "error": format!("HTTP {}", r.status())}),
            Err(e) => json!({"configured": true, "ok": false, "error": e.to_string()}),
        }
    };

    let (voice_gpu, cybernotes, pc_companion) = tokio::join!(voice_fut, notes_fut, pc_fut);
    let active_profile = app.store.with(|s| s.active_profile().name);
    reply(
        StatusCode::OK,
        json!({
            "speaker": {
                "ok": true,
                "version": crate::version(),
                "mic": app.engine.mic.online(),
                "uptime_s": app.started.elapsed().as_secs(),
                "night": app.store.is_night(),
                "profile": active_profile,
            },
            "voice_gpu": voice_gpu,
            "cybernotes": cybernotes,
            "pc_companion": pc_companion,
            "telegram": {"configured": tg_configured},
            "vpn": vpn_json,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_cybernote_paths() {
        assert!(valid_note_path("note.md"));
        assert!(valid_note_path("Инструкции/Домашняя Сеть/Тест.MD"));
        assert!(!valid_note_path(""));
        assert!(!valid_note_path("/etc/passwd.md"));
        assert!(!valid_note_path("../secret.md"));
        assert!(!valid_note_path("dir/../secret.md"));
        assert!(!valid_note_path(".hidden/note.md"));
        assert!(!valid_note_path("dir\\note.md"));
        assert!(!valid_note_path("note.txt"));
        assert!(!valid_note_path("note\n.md"));
    }

    #[test]
    fn sanitizes_audio_extensions_and_multipart() {
        assert_eq!(sanitize_audio_ext("ogg"), ("ogg", "audio/ogg"));
        assert_eq!(sanitize_audio_ext(".M4A"), ("m4a", "audio/mp4"));
        assert_eq!(sanitize_audio_ext("../../exe"), ("webm", "audio/webm"));

        let body = build_audio_multipart("b123", "ptt.webm", "audio/webm", "ru", b"abc");
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("--b123\r\n"));
        assert!(text.contains("filename=\"ptt.webm\""));
        assert!(text.contains("Content-Type: audio/webm"));
        assert!(text.contains("name=\"language\"\r\n\r\nru"));
        assert!(text.ends_with("--b123--\r\n"));
    }
}
