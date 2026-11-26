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
    let Some(text)=value["text"].as_str().map(str::trim).filter(|t|!t.is_empty() && t.len()<=16000)else{return super::bad("Текст: от 1 до 16000 байт");};
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
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    match client.post(&url).json(&body).send().await {
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
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    match client
        .post(&url)
        .query(&[
            ("name", name),
            ("filename", filename),
            ("ref_text", ref_text),
            ("language", language),
        ])
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(body.to_vec())
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
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    match client.delete(&url).send().await {
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

pub async fn activity(State(app): State<App>) -> Response {
    reply(
        StatusCode::OK,
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
        }),
    )
}

pub async fn scenario_run(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let id = value["id"].as_str().unwrap_or("");
    match crate::tools::run_scenario(app.engine.clone(), id) {
        Ok(msg) => reply(StatusCode::OK, json!({"ok": true, "message": msg})),
        Err(e) => super::bad(e),
    }
}

pub async fn modes_get(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.modes.status())
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
        "rpg" | "quiz" | "translator" => {
            app.engine.tools.stop_for(&app.engine, "modes");
            let target = match mode.as_str() {
                "rpg" => crate::modes::SpecialMode::Rpg,
                "quiz" => crate::modes::SpecialMode::Quiz,
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
pub async fn volume(body: axum::body::Bytes) -> Response {
    let Some(lvl) = serde_json::from_slice::<serde_json::Value>(&body).ok().and_then(|v| v.get("level").or(v.get("Level")).and_then(|x| x.as_i64())) else {
        return super::bad("пустой запрос");
    };
    let lvl = lvl.clamp(0, 10);
    match tokio::task::spawn_blocking(move || crate::speakervol::set_alice_volume(lvl)).await.unwrap_or_else(|e| Err(e.to_string())) {
        Ok(()) => reply(StatusCode::OK, json!({"volume": lvl})),
        Err(e) => super::bad(e),
    }
}

pub async fn journal_clear(State(app): State<App>) -> Response {
    app.engine.journal.clear();
    ok()
}

pub async fn debate_start(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let Some(topic) = value["topic"].as_str().map(str::trim).filter(|t| !t.is_empty() && t.len() <= 300) else {
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
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(2)).build() {
        Ok(c) => c,
        Err(_) => return reply(StatusCode::OK, json!({"online": false})),
    };
    match client.get(&url).send().await {
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
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    match client.post(&url).send().await {
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
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    let resp = client
        .post(&url)
        .query(&[("filename", filename), ("ref_text", ref_text)])
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(body.to_vec())
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
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    let resp = client.delete(&url).send().await;
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
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    let resp = match client
        .post(&url)
        .query(&[("filename", filename), ("max_chars", "12000")])
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(body.to_vec())
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
    crate::debate::spawn_notebook(
        app.engine.clone(),
        crate::debate::NotebookParams {
            url: String::new(),
            text,
            title: title.clone(),
            p1_id,
            p2_id,
            turns: 6,
        },
    );
    reply(StatusCode::OK, json!({"ok": true, "title": title}))
}

pub async fn books_list(State(app): State<App>) -> Response {
    let url = format!("{}/books", local_voice_server(&app));
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(6)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    let mut base = match client.get(&url).send().await {
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
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    let resp = client
        .post(&url)
        .query(&[("filename", filename), ("title", title)])
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(body.to_vec())
        .send()
        .await;
    forward_voice_response(resp, "Не удалось загрузить книгу").await
}

pub async fn books_url(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let url = format!("{}/books/from-url", local_voice_server(&app));
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(45)).build() {
        Ok(c) => c,
        Err(e) => return super::bad(e.to_string()),
    };
    let resp = client.post(&url).json(&value).send().await;
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

pub async fn games_status(State(app): State<App>) -> Response {
    reply(StatusCode::OK, app.games.status())
}

pub async fn games_melody(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let theme = value["theme"].as_str().unwrap_or("аниме опенинги").to_string();
    match app.games.start_melody(app.engine.clone(), theme).await {
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
    app.dj.next();
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

pub async fn notes_list(
    State(app): State<App>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let limit = query.get("limit").and_then(|s| s.parse::<u32>().ok()).unwrap_or(20).clamp(1, 100);
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

pub async fn notes_save(State(app): State<App>, axum::Json(value): axum::Json<serde_json::Value>) -> Response {
    let content = value["content"].as_str().unwrap_or("").trim();
    if content.is_empty() || content.len() > 20000 {
        return super::bad("Текст заметки должен быть от 1 до 20000 байт");
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
