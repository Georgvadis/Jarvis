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
    let url = app.plugins.plugin_setting_str("local-rvc-voice", "server_url");
    let u = if url.is_empty() { "http://192.168.1.3:8768" } else { url.as_str() };
    u.trim_end_matches('/').to_string()
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
    app.modes.reset(Some(&app.ambient));
    app.focus.stop();
    app.ambient.stop();
    let e = app.engine.clone();
    let _ = tokio::task::spawn_blocking(move || e.stop("кнопка в настройках")).await;
    ok()
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
