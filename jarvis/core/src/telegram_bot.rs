//! Двусторонний Telegram-бот: интерком (текст + голосовые), /ask с ответом ИИ и управление колонкой.
//! Работает через SOCKS5-прокси ядра (если для плагинов включён VPN) и переиспользует настройки `telegram-sender`.

use crate::{
    ambient::Ambient,
    engine::Engine,
    focus::Focus,
    plugins::Host,
    vpn::{Proxy, Route},
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

fn voice_server(plugins: &Host) -> String {
    let u = plugins.plugin_setting_str("local-rvc-voice", "server_url");
    if u.is_empty() {
        "http://192.168.1.3:8768".into()
    } else {
        u.trim_end_matches('/').to_string()
    }
}

fn build_tg_client(route: &Route) -> Result<(String, reqwest::Client), String> {
    match route {
        Route::Direct => {
            let c = reqwest::Client::builder()
                .user_agent(crate::net::user_agent())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(35))
                .build()
                .map_err(|e| e.to_string())?;
            Ok(("direct".into(), c))
        }
        Route::Tunnel(t) => {
            let key = format!("socks:{}:{}:{}", t.socks, t.user, t.pass);
            let c = crate::net::via_socks(&t.socks, &t.user, &t.pass, Duration::from_secs(35))?;
            Ok((key, c))
        }
        Route::Blocked(why) => Err(why.clone()),
    }
}

fn route_key(route: &Route) -> Option<String> {
    match route {
        Route::Direct => Some("direct".into()),
        Route::Tunnel(t) => Some(format!("socks:{}:{}:{}", t.socks, t.user, t.pass)),
        Route::Blocked(_) => None,
    }
}

fn clip_tg(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let head: String = text.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{head}…")
    }
}

fn build_stt_multipart(boundary: &str, ogg_bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(ogg_bytes.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"audio\"; filename=\"voice.ogg\"\r\nContent-Type: audio/ogg\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(ogg_bytes);
    body.extend_from_slice(
        format!(
            "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"language\"\r\n\r\nru\r\n--{boundary}--\r\n"
        )
        .as_bytes(),
    );
    body
}

async fn send_tg(client: &reqwest::Client, token: &str, chat_id: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let send_url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let _ = client
        .post(&send_url)
        .json(&json!({
            "chat_id": chat_id,
            "text": clip_tg(text, 3900),
        }))
        .send()
        .await;
}

pub async fn notify_telegram(plugins: &Host, proxy: &Proxy, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let token = plugins.plugin_setting_str("telegram-sender", "bot_token");
    let chat_id = plugins.plugin_setting_str("telegram-sender", "chat_id");
    if token.trim().is_empty() || chat_id.trim().is_empty() {
        return;
    }
    let route = proxy.route("plugins");
    if let Ok((_, client)) = build_tg_client(&route) {
        send_tg(&client, token.trim(), chat_id.trim(), text).await;
    }
}

pub fn spawn(
    engine: Arc<Engine>,
    plugins: Arc<Host>,
    proxy: Arc<Proxy>,
    ambient: Arc<Ambient>,
    focus: Arc<Focus>,
) {
    tokio::spawn(async move {
        let mut offset: i64 = 0;
        let mut initialized_token = String::new();
        let mut cached_client: Option<(String, reqwest::Client)> = None;

        loop {
            let token = plugins.plugin_setting_str("telegram-sender", "bot_token");
            let allowed_chat = plugins.plugin_setting_str("telegram-sender", "chat_id");
            if token.is_empty() || allowed_chat.is_empty() {
                initialized_token.clear();
                tokio::time::sleep(Duration::from_secs(15)).await;
                continue;
            }

            let route = proxy.route("plugins");
            let Some(desired_key) = route_key(&route) else {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            };
            let client = match &cached_client {
                Some((k, c)) if *k == desired_key => c.clone(),
                _ => match build_tg_client(&route) {
                    Ok((k, c)) => {
                        cached_client = Some((k, c.clone()));
                        c
                    }
                    Err(e) => {
                        log::warn!("telegram-bot: клиент недоступен: {e}");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                },
            };

            // При первом старте с новым токеном пропускаем старые накопившиеся апдейты.
            if initialized_token != token {
                let url = format!("https://api.telegram.org/bot{token}/getUpdates?offset=-1&timeout=1");
                if let Ok(resp) = client.get(&url).send().await
                    && let Ok(v) = resp.json::<Value>().await
                    && let Some(arr) = v["result"].as_array()
                    && let Some(last) = arr.last()
                    && let Some(uid) = last["update_id"].as_i64()
                {
                    offset = uid + 1;
                }
                initialized_token = token.clone();
                log::info!("telegram-bot: двусторонний режим активен (chat_id={allowed_chat})");
            }

            let url = format!(
                "https://api.telegram.org/bot{token}/getUpdates?offset={offset}&timeout=25&allowed_updates=[\"message\"]"
            );
            let resp = match client.get(&url).send().await {
                Ok(r) => r,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
            let body: Value = match resp.json().await {
                Ok(v) => v,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            };
            let Some(updates) = body["result"].as_array() else {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            };
            for upd in updates {
                if let Some(uid) = upd["update_id"].as_i64() {
                    offset = offset.max(uid + 1);
                }
                let msg = &upd["message"];
                let chat_id = msg["chat"]["id"]
                    .as_i64()
                    .map(|n| n.to_string())
                    .or_else(|| msg["chat"]["id"].as_str().map(str::to_string))
                    .unwrap_or_default();
                if chat_id != allowed_chat {
                    continue;
                }

                if let Some(file_id) = msg["voice"]["file_id"].as_str() {
                    let file_size = msg["voice"]["file_size"].as_u64().unwrap_or(0);
                    let caption = msg["caption"].as_str().unwrap_or("").trim().to_string();
                    let reply_text = handle_voice(
                        &client,
                        &token,
                        &engine,
                        &plugins,
                        file_id,
                        file_size,
                        &caption,
                    )
                    .await;
                    send_tg(&client, &token, &allowed_chat, &reply_text).await;
                    continue;
                }

                let Some(raw_text) = msg["text"].as_str() else {
                    continue;
                };
                let text = raw_text.trim();
                if text.is_empty() {
                    continue;
                }

                let reply_text = handle_message(&engine, &ambient, &focus, text).await;
                send_tg(&client, &token, &allowed_chat, &reply_text).await;
            }
        }
    });
}

async fn handle_voice(
    tg_client: &reqwest::Client,
    token: &str,
    engine: &Arc<Engine>,
    plugins: &Host,
    file_id: &str,
    file_size: u64,
    caption: &str,
) -> String {
    if file_size > 8 * 1024 * 1024 {
        return "Голосовое сообщение слишком большое (макс. 8 МБ).".into();
    }
    let meta_url = format!("https://api.telegram.org/bot{token}/getFile?file_id={file_id}");
    let meta: Value = match tg_client.get(&meta_url).send().await {
        Ok(r) if r.status().is_success() => match r.json().await {
            Ok(v) => v,
            Err(e) => return format!("Ошибка чтения getFile: {e}"),
        },
        Ok(r) => return format!("Telegram getFile HTTP {}", r.status()),
        Err(e) => return format!("Не удалось получить файл из Telegram: {e}"),
    };
    let Some(file_path) = meta["result"]["file_path"].as_str() else {
        return "Telegram не вернул путь к голосовому файлу.".into();
    };
    let file_url = format!("https://api.telegram.org/file/bot{token}/{file_path}");
    let ogg_bytes = match tg_client.get(&file_url).send().await {
        Ok(r) if r.status().is_success() => match r.bytes().await {
            Ok(b) if !b.is_empty() && b.len() <= 8 * 1024 * 1024 => b.to_vec(),
            Ok(_) => return "Пустой или слишком большой аудиофайл.".into(),
            Err(e) => return format!("Ошибка скачивания аудио: {e}"),
        },
        Ok(r) => return format!("Ошибка скачивания аудио HTTP {}", r.status()),
        Err(e) => return format!("Ошибка скачивания аудио: {e}"),
    };

    let srv = voice_server(plugins);
    let cap_low = caption.to_lowercase();
    if cap_low.starts_with("/ask") {
        let speak = cap_low.starts_with("/ask!");
        let boundary = format!("jarvis-tg-{}", crate::settings::rand_hex(8));
        let body = build_stt_multipart(&boundary, &ogg_bytes);
        let stt_resp = match crate::net::direct()
            .post(format!("{srv}/stt"))
            .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
            .body(body)
            .timeout(Duration::from_secs(30))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r.json::<Value>().await.unwrap_or_default(),
            Ok(r) => return format!("STT-сервер вернул HTTP {}", r.status()),
            Err(e) => return format!("STT-сервер недоступен: {e}"),
        };
        let transcript = stt_resp["text"].as_str().unwrap_or("").trim().to_string();
        if transcript.is_empty() {
            return "Не удалось распознать речь в голосовом сообщении.".into();
        }
        return match engine.clone().ask_text(transcript.clone(), speak).await {
            Ok(ans) => format!("🎤 «{transcript}»\n\n{ans}"),
            Err(e) => format!("🎤 «{transcript}»\n\nОшибка: {e}"),
        };
    }

    let force = caption == "!";
    if !force && engine.store.is_night() {
        return "Ночной режим активен — голосовое сообщение не воспроизведено (добавьте подпись «!» для срочного проигрывания).".into();
    }

    let pcm_bytes = match crate::net::direct()
        .post(format!("{srv}/decode?rate=24000"))
        .body(ogg_bytes)
        .timeout(Duration::from_secs(25))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => match r.bytes().await {
            Ok(b) => b,
            Err(e) => return format!("Ошибка чтения декодированного звука: {e}"),
        },
        Ok(r) => return format!("Сервер декодирования вернул HTTP {}", r.status()),
        Err(e) => return format!("Сервер jarvis-voice недоступен: {e}"),
    };
    if pcm_bytes.len() < 4 || pcm_bytes.len() % 2 != 0 {
        return "Неверный формат декодированного аудио.".into();
    }
    let samples: Vec<i16> = pcm_bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();

    engine.journal.note("telegram", "🎤 Голосовое сообщение", "");
    let e = engine.clone();
    match tokio::task::spawn_blocking(move || {
        e.earcon(true);
        e.play_pcm_24k(&samples)
    })
    .await
    {
        Ok(Ok(())) => "🔊 Голосовое сообщение проиграно на колонке.".into(),
        Ok(Err(err)) => format!("Не удалось воспроизвести: {err}"),
        Err(err) => format!("Ошибка воспроизведения: {err}"),
    }
}

async fn handle_message(
    engine: &Arc<Engine>,
    ambient: &Arc<Ambient>,
    focus: &Arc<Focus>,
    text: &str,
) -> String {
    let (cmd_raw, rest) = match text.split_once(char::is_whitespace) {
        Some((c, r)) => (c, r.trim()),
        None => (text, ""),
    };
    let cmd = cmd_raw.split('@').next().unwrap_or(cmd_raw).to_lowercase();
    let lower = text.trim().to_lowercase();

    if cmd == "/start" || cmd == "/help" {
        return "Управление колонкой YaJarvis:\n\
• Любой текст или /say <текст> — озвучить на колонке (/say! — даже ночью)\n\
• Голосовое сообщение — проиграть интеркомом (подпись /ask — спросить Джарвиса)\n\
• /ask <вопрос> — спросить Джарвиса с ответом сюда (/ask! — также озвучить вслух)\n\
• /timer [минуты|10м|90с|1ч [метка] | list | off] — таймеры и напоминания\n\
• /scenario <утро|работа|вечер|сон> — быстрый сценарий на колонке\n\
• /briefing — персональный брифинг (погода, расписание, книга, заметки; /briefing! — также озвучить)\n\
• /pc <screen|play|next|prev|vol+|vol-|mute|shot|lock|window|clip|open|info|macro> — управление ПК\n\
• /podcast <ссылка|текст> — подкаст двумя голосами (YouTube / статья / текст)\n\
• /dj <вайб|next|stop> — AI-DJ под настроение\n\
• /note <текст> — сохранить быструю заметку в CyberNotes\n\
• /book [play|stop|recap] — читалка книг\n\
• /focus [мин|off] — таймер Помодоро на дисплее\n\
• /ambient <rain|fireplace|ocean|white|lofi|jazz|chill|vol N|off> — фон/радио\n\
• /vol <0-10> — громкость колонки\n\
• /stop — остановить речь, игры, книгу, фон и режимы\n\
• /status — статус колонки".into();
    }

    if cmd == "/stop" || lower == "стоп" {
        if let Some(reader) = &engine.tools.reader {
            reader.stop();
        }
        if let Some(dj) = &engine.tools.dj {
            dj.stop(Some(engine));
        }
        if let Some(games) = &engine.tools.games {
            games.stop_all(Some(engine));
        }
        if let Some(modes) = &engine.tools.modes {
            modes.reset(Some(ambient));
        }
        focus.stop();
        ambient.stop();
        let e = engine.clone();
        let _ = tokio::task::spawn_blocking(move || e.stop("Telegram")).await;
        return "Остановил.".into();
    }

    if cmd == "/status" {
        let s = engine.store.get();
        let p = s.active_profile();
        let mic = if engine.mic.online() { "вкл" } else { "выкл" };
        let amb = ambient.current_label().unwrap_or_else(|| "выкл".into());
        let focus_st = focus.status();
        let focus_str = if focus_st["running"].as_bool().unwrap_or(false) {
            let rem = focus_st["remaining_s"].as_u64().unwrap_or(0);
            format!("{:02}:{:02}", rem / 60, rem % 60)
        } else {
            "выкл".into()
        };
        let mode_st = engine
            .tools
            .modes
            .as_ref()
            .map(|m| m.status()["mode"].as_str().unwrap_or("normal").to_string())
            .unwrap_or_else(|| "normal".into());
        let mut lines = vec![
            format!("Колонка онлайн ({})", crate::version()),
            format!("Ассистент: {}", p.name),
            format!("Микрофон: {mic}"),
            format!("Режим: {mode_st}{}", if engine.store.is_night() { " (ночь)" } else { "" }),
            format!("Фокус: {focus_str}"),
            format!("Фон/радио: {amb}"),
        ];
        if let Some(sched) = &engine.tools.scheduler {
            let timers = sched.active_timers_json();
            if let Some(arr) = timers.as_array()
                && !arr.is_empty()
            {
                let nearest = arr[0]["remaining_s"].as_u64().unwrap_or(0);
                lines.push(format!(
                    "Таймеры: {} (ближайший через {})",
                    arr.len(),
                    crate::scheduler::format_duration_ru(nearest)
                ));
            }
        }
        if let Some(dj) = &engine.tools.dj {
            let st = dj.status();
            if st["active"].as_bool().unwrap_or(false) {
                let wave = st["station"].as_str().filter(|s| !s.is_empty())
                    .or_else(|| st["mood"].as_str())
                    .unwrap_or("эфир");
                lines.push(format!("AI-DJ: {wave}"));
            }
        }
        if let Some(reader) = &engine.tools.reader {
            let st = reader.status();
            if st["running"].as_bool().unwrap_or(false) {
                let title = st["title"].as_str().unwrap_or("книга");
                let ch = st["chapter"].as_u64().unwrap_or(0) + 1;
                lines.push(format!("Книга: {title} (гл. {ch})"));
            }
        }
        if let Some(games) = &engine.tools.games {
            let st = games.status();
            let gname = st["active_game"].as_str().unwrap_or("");
            if !gname.is_empty() {
                lines.push(format!("Игра: {gname}"));
            }
        }
        return lines.join("\n");
    }

    if cmd == "/timer" || cmd == "/timers" {
        let Some(sched) = &engine.tools.scheduler else {
            return "Планировщик недоступен.".into();
        };
        let low_rest = rest.trim().to_lowercase();
        if low_rest.is_empty() || matches!(low_rest.as_str(), "list" | "список" | "?") {
            let timers = sched.active_timers_json();
            let Some(arr) = timers.as_array() else {
                return "Активных таймеров сейчас нет.".into();
            };
            if arr.is_empty() {
                return "Активных таймеров сейчас нет.".into();
            }
            let items: Vec<String> = arr
                .iter()
                .map(|t| {
                    let lbl = t["label"].as_str().unwrap_or("Таймер");
                    let rem = t["remaining_s"].as_u64().unwrap_or(0);
                    format!("• {lbl} — через {}", crate::scheduler::format_duration_ru(rem))
                })
                .collect();
            return format!("⏱ Активные таймеры:\n{}", items.join("\n"));
        }
        if matches!(low_rest.as_str(), "off" | "stop" | "cancel" | "отмена" | "стоп") {
            let n = sched.cancel_all_timers();
            return if n == 0 {
                "Активных таймеров сейчас нет.".into()
            } else {
                format!("Отменил активные таймеры ({n}).")
            };
        }
        return match parse_tg_timer_arg(rest) {
            Ok((secs, label)) => match sched.add_timer(label.clone(), secs) {
                Ok(_) => {
                    let dur = crate::scheduler::format_duration_ru(secs);
                    format!("⏱ Поставил «{label}» на {dur}.")
                }
                Err(e) => e,
            },
            Err(e) => e,
        };
    }

    if cmd == "/scenario" {
        if rest.trim().is_empty() {
            let mut msg = "Сценарии: утро, работа, вечер, сон".to_string();
            if let Some(sc_store) = &engine.tools.scenarios {
                let list = sc_store.custom_summary_json();
                if let Some(arr) = list.as_array()
                    && !arr.is_empty()
                {
                    let names: Vec<&str> = arr.iter().filter_map(|s| s["name"].as_str()).collect();
                    if !names.is_empty() {
                        msg.push_str(&format!("\nПользовательские: {}", names.join(", ")));
                    }
                }
            }
            return format!("{msg}\n\nЗапуск: /scenario <название>");
        }
        let target = map_tg_scenario(rest).unwrap_or_else(|| rest.trim());
        return match crate::tools::run_scenario(engine.clone(), target) {
            Ok(msg) => msg,
            Err(e) => e,
        };
    }

    if cmd == "/pc" {
        let (sub_raw, sub_rest) = match rest.trim().split_once(char::is_whitespace) {
            Some((s, r)) => (s, r.trim()),
            None => (rest.trim(), ""),
        };
        let sub_low = sub_raw.to_lowercase();
        if sub_low.is_empty() {
            return "Команды ПК: /pc <screen|play|next|prev|vol+|vol-|mute|shot|lock|window|clip|open|info|macro>".into();
        }
        if sub_low == "screen" || sub_low == "экран" {
            let q = if sub_rest.is_empty() {
                "Кратко опиши, что сейчас на экране."
            } else {
                sub_rest
            };
            return match crate::tools::capture_and_describe_screen(&engine.host, &engine.store, q).await {
                Ok(ans) => {
                    engine.journal.note("экран", q, &ans);
                    format!("🖥 {ans}")
                }
                Err(e) => format!("Ошибка ПК: {e}"),
            };
        }
        if sub_low == "clip" || sub_low == "буфер" {
            return if sub_rest.is_empty() {
                match crate::tools::pc_clipboard_read(&engine.store).await {
                    Ok(txt) if txt.is_empty() => "📋 Буфер обмена ПК пуст.".into(),
                    Ok(txt) => format!("📋 Буфер ПК:\n{txt}"),
                    Err(e) => format!("Ошибка ПК: {e}"),
                }
            } else {
                match crate::tools::pc_clipboard_write(&engine.store, sub_rest).await {
                    Ok(msg) => format!("📋 {msg}"),
                    Err(e) => format!("Ошибка ПК: {e}"),
                }
            };
        }
        if sub_low == "open" || sub_low == "url" {
            if sub_rest.is_empty() {
                return "Укажите ссылку: /pc open https://...".into();
            }
            return match crate::tools::pc_open_url(&engine.store, sub_rest).await {
                Ok(msg) => format!("🌐 {msg}"),
                Err(e) => format!("Ошибка ПК: {e}"),
            };
        }
        if sub_low == "info" || sub_low == "status" {
            return match crate::tools::pc_system_info(&engine.store).await {
                Ok(info) => format!("🖥 {}", crate::tools::format_pc_system_summary(&info)),
                Err(e) => format!("Ошибка ПК: {e}"),
            };
        }
        if sub_low == "macro" {
            if sub_rest.is_empty() {
                return match crate::tools::pc_macros_list(&engine.store).await {
                    Ok(v) => {
                        let list = v["macros"].as_array().cloned().unwrap_or_default();
                        if list.is_empty() {
                            "Макросы не настроены (~/.jarvis-pc-macros.json на ПК).".into()
                        } else {
                            let lines: Vec<String> = list
                                .iter()
                                .map(|m| format!("• {} — {}", m["id"].as_str().unwrap_or(""), m["name"].as_str().unwrap_or("")))
                                .collect();
                            format!("⚡ Макросы ПК:\n{}", lines.join("\n"))
                        }
                    }
                    Err(e) => format!("Ошибка ПК: {e}"),
                };
            }
            return match crate::tools::pc_macro_run(&engine.store, sub_rest).await {
                Ok(msg) => format!("⚡ {msg}"),
                Err(e) => format!("Ошибка ПК: {e}"),
            };
        }
        let Some(action) = map_tg_pc_action(&sub_low) else {
            return "Неизвестная команда ПК. Доступны: screen, play, next, prev, vol+, vol-, mute, shot, lock, window, clip, open, info, macro.".into();
        };
        return match crate::tools::call_pc_action(&engine.store, action, 30).await {
            Ok(msg) => format!("🖥 {msg}"),
            Err(e) => format!("Ошибка ПК: {e}"),
        };
    }

    if cmd == "/vol" || cmd == "громкость" {
        if let Ok(lvl) = rest.parse::<i64>() {
            let lvl = lvl.clamp(0, 10);
            let res = tokio::task::spawn_blocking(move || crate::speakervol::set_alice_volume(lvl))
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            return match res {
                Ok(()) => {
                    crate::engine::flash_volume_clock(engine, lvl);
                    format!("Громкость: {lvl}/10")
                }
                Err(e) => format!("Ошибка громкости: {e}"),
            };
        }
        return "Укажите число от 0 до 10, например: /vol 5".into();
    }

    if cmd == "/focus" {
        let arg = rest.to_lowercase();
        if arg == "off" || arg == "stop" || arg == "выкл" {
            focus.stop();
            return "Фокус-режим остановлен.".into();
        }
        let mins = if arg.is_empty() {
            25
        } else {
            match arg.parse::<u64>() {
                Ok(m) if m > 0 => m,
                _ => return "Укажите минуты (1..99) или /focus off".into(),
            }
        };
        return focus.start(engine.clone(), mins);
    }

    if cmd == "/ambient" || cmd == "/radio" {
        let arg = rest.to_lowercase();
        if let Some(vrest) = arg.strip_prefix("vol ") {
            if let Ok(v) = vrest.trim().parse::<i64>() {
                let st = ambient.set_volume(v);
                return format!("Громкость фона/радио: {}%", st["volume"].as_i64().unwrap_or(v));
            }
            return "Укажите громкость от 5 до 100, например: /ambient vol 50".into();
        }
        if arg.is_empty() || arg == "off" || arg == "stop" || arg == "выкл" {
            ambient.stop();
            return "Фон и радио выключены.".into();
        }
        return match ambient.start(&arg) {
            Ok(label) => format!("Включил: {label}"),
            Err(e) => e,
        };
    }

    if cmd == "/podcast" {
        if rest.is_empty() {
            return "Укажите ссылку (YouTube/статья) или текст после /podcast".into();
        }
        if crate::debate::is_active() {
            return "Подкаст или спор уже запущен.".into();
        }
        let is_url = rest.starts_with("http://") || rest.starts_with("https://");
        crate::debate::spawn_notebook(
            engine.clone(),
            crate::debate::NotebookParams {
                url: if is_url { rest.to_string() } else { String::new() },
                text: if is_url { String::new() } else { rest.to_string() },
                title: String::new(),
                p1_id: None,
                p2_id: None,
                turns: 6,
            },
        );
        return "🎙 Запускаю подкаст двумя голосами!".into();
    }

    if cmd == "/dj" {
        let Some(dj) = &engine.tools.dj else {
            return "Модуль AI-DJ недоступен.".into();
        };
        let arg = rest.trim().to_lowercase();
        if arg == "stop" || arg == "off" || arg == "стоп" {
            dj.stop(Some(engine));
            return "AI-DJ остановлен.".into();
        }
        if arg == "next" || arg == "дальше" {
            dj.next();
            return "Переключаю следующий трек DJ.".into();
        }
        if arg == "like" || arg == "лайк" {
            return dj.like(engine).await;
        }
        let mood = if rest.is_empty() { "ночной Токио синтвейв" } else { rest };
        return match dj.start(engine.clone(), mood.to_string()) {
            Ok(st) => format!("🎧 Волна «{}» запущена!", st["station"].as_str().unwrap_or(mood)),
            Err(e) => format!("Ошибка AI-DJ: {e}"),
        };
    }

    if cmd == "/note" {
        if rest.is_empty() {
            return "Напишите текст заметки после /note".into();
        }
        let (title, content) = match rest.split_once('\n') {
            Some((t, b)) if !t.trim().is_empty() && !b.trim().is_empty() => (t.trim(), b.trim()),
            _ => ("", rest),
        };
        return match crate::tools::save_cybernote(&engine.host, title, content, &[], "Голосовые заметки").await {
            Ok(path) => format!("📝 Сохранено в CyberNotes: {path}"),
            Err(e) => format!("Ошибка CyberNotes: {e}"),
        };
    }

    if cmd == "/book" {
        let Some(reader) = &engine.tools.reader else {
            return "Читалка недоступна.".into();
        };
        let arg = rest.trim().to_lowercase();
        if arg == "stop" || arg == "pause" || arg == "стоп" {
            reader.stop();
            return "Чтение книги остановлено.".into();
        }
        if arg == "recap" || arg == "пересказ" {
            return match reader.recap(engine.clone()).await {
                Ok(t) => format!("📖 На чём остановились:\n\n{t}"),
                Err(e) => format!("Ошибка пересказа: {e}"),
            };
        }
        return match reader.start(engine.clone(), None, None).await {
            Ok(_) => "📖 Продолжаю чтение книги с закладки!".into(),
            Err(e) => format!("Не удалось запустить книгу: {e}"),
        };
    }

    if cmd == "/ask" || cmd == "/ask!" {
        if rest.is_empty() {
            return "Напишите вопрос после /ask".into();
        }
        let speak = cmd == "/ask!";
        return match engine.clone().ask_text(rest.to_string(), speak).await {
            Ok(ans) => ans,
            Err(e) => format!("Ошибка: {e}"),
        };
    }

    if cmd == "/briefing" || cmd == "/briefing!" {
        let speak = cmd == "/briefing!";
        return match crate::briefing::run_briefing(engine.clone(), rest, speak).await {
            Ok(ans) => format!("🌅 {ans}"),
            Err(e) => format!("Ошибка брифинга: {e}"),
        };
    }

    if cmd.starts_with('/') && cmd != "/say" && cmd != "/say!" {
        return "Неизвестная команда. Отправьте /help для списка команд.".into();
    }

    let force = cmd == "/say!";
    let to_say = if cmd == "/say" || cmd == "/say!" {
        rest.to_string()
    } else {
        text.strip_prefix("скажи: ")
            .or_else(|| text.strip_prefix("Скажи: "))
            .or_else(|| text.strip_prefix("скажи "))
            .or_else(|| text.strip_prefix("Скажи "))
            .unwrap_or(text)
            .trim()
            .to_string()
    };
    if to_say.is_empty() {
        return String::new();
    }
    if !force && engine.store.is_night() {
        return "Ночной режим активен — озвучка пропущена (для срочного сообщения используйте /say! <текст>).".into();
    }

    engine.journal.note("telegram", &to_say, "");
    let e = engine.clone();
    let say_copy = to_say.clone();
    tokio::task::spawn_blocking(move || {
        e.earcon(true);
        e.say_text(&say_copy);
    });
    format!("Озвучил на колонке: «{to_say}»")
}

pub(crate) fn parse_tg_timer_arg(rest: &str) -> Result<(u64, String), String> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return Err("Укажите длительность, например: /timer 10 кофе".into());
    }
    let (head, mut tail) = match trimmed.split_once(char::is_whitespace) {
        Some((h, t)) => (h.trim(), t.trim()),
        None => (trimmed, ""),
    };
    let head_low = head.to_lowercase();
    let (num_str, mut mult): (&str, Option<u64>) = if let Some(s) = head_low.strip_suffix('s').or_else(|| head_low.strip_suffix('с')) {
        (s, Some(1))
    } else if let Some(s) = head_low.strip_suffix('m').or_else(|| head_low.strip_suffix('м')) {
        (s, Some(60))
    } else if let Some(s) = head_low.strip_suffix('h').or_else(|| head_low.strip_suffix('ч')) {
        (s, Some(3600))
    } else {
        (head_low.as_str(), None)
    };

    let Ok(val) = num_str.parse::<u64>() else {
        return Err("Укажите число (минуты или с суффиксом s/m/h, с/м/ч), например: /timer 5м чай".into());
    };
    if mult.is_none() && !tail.is_empty() {
        let (uword, after) = match tail.split_once(char::is_whitespace) {
            Some((u, a)) => (u.trim().to_lowercase(), a.trim()),
            None => (tail.to_lowercase(), ""),
        };
        if uword.starts_with("сек") {
            mult = Some(1);
            tail = after;
        } else if uword.starts_with("мин") {
            mult = Some(60);
            tail = after;
        } else if uword.starts_with("час") {
            mult = Some(3600);
            tail = after;
        }
    }
    let secs = val.saturating_mul(mult.unwrap_or(60));
    if !(5..=86400).contains(&secs) {
        return Err("Длительность таймера: от 5 секунд до 24 часов.".into());
    }
    let label = if tail.is_empty() {
        "Таймер".to_string()
    } else {
        tail.chars().take(80).collect()
    };
    Ok((secs, label))
}

pub(crate) fn map_tg_scenario(arg: &str) -> Option<&'static str> {
    match arg.trim().to_lowercase().as_str() {
        "morning" | "утро" | "утренний" => Some("morning"),
        "work" | "работа" | "рабочий" | "фокус" => Some("work"),
        "evening" | "вечер" | "вечерний" | "relax" | "релакс" => Some("evening"),
        "sleep" | "сон" | "ночь" | "ночной" => Some("sleep"),
        _ => None,
    }
}

pub(crate) fn map_tg_pc_action(arg: &str) -> Option<&'static str> {
    match arg.trim().to_lowercase().as_str() {
        "play" | "pause" | "media" | "пауза" | "плей" => Some("media_play_pause"),
        "next" | "след" | "дальше" => Some("media_next"),
        "prev" | "пред" | "назад" => Some("media_prev"),
        "vol+" | "up" | "громче" => Some("volume_up"),
        "vol-" | "down" | "тише" => Some("volume_down"),
        "mute" | "беззвука" => Some("volume_mute"),
        "shot" | "screenshot" | "скрин" | "скриншот" => Some("screenshot_save"),
        "lock" | "блок" | "заблокировать" => Some("lock_screen"),
        "window" | "окно" => Some("active_window"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tg_timer_durations_and_labels() {
        assert_eq!(parse_tg_timer_arg("5"), Ok((300, "Таймер".into())));
        assert_eq!(parse_tg_timer_arg("10 кофе"), Ok((600, "кофе".into())));
        assert_eq!(parse_tg_timer_arg("90s"), Ok((90, "Таймер".into())));
        assert_eq!(parse_tg_timer_arg("45с"), Ok((45, "Таймер".into())));
        assert_eq!(parse_tg_timer_arg("15m духовка"), Ok((900, "духовка".into())));
        assert_eq!(parse_tg_timer_arg("15м духовка"), Ok((900, "духовка".into())));
        assert_eq!(parse_tg_timer_arg("2h проветрить"), Ok((7200, "проветрить".into())));
        assert_eq!(parse_tg_timer_arg("1ч"), Ok((3600, "Таймер".into())));
        assert_eq!(parse_tg_timer_arg("10 минут пельмени"), Ok((600, "пельмени".into())));
        assert!(parse_tg_timer_arg("2s").is_err());
        assert!(parse_tg_timer_arg("25h").is_err());
        assert!(parse_tg_timer_arg("abc").is_err());
    }

    #[test]
    fn maps_tg_scenarios_and_pc_actions() {
        assert_eq!(map_tg_scenario("утро"), Some("morning"));
        assert_eq!(map_tg_scenario("Работа"), Some("work"));
        assert_eq!(map_tg_scenario("relax"), Some("evening"));
        assert_eq!(map_tg_scenario("сон"), Some("sleep"));
        assert_eq!(map_tg_scenario("other"), None);

        assert_eq!(map_tg_pc_action("play"), Some("media_play_pause"));
        assert_eq!(map_tg_pc_action("след"), Some("media_next"));
        assert_eq!(map_tg_pc_action("vol+"), Some("volume_up"));
        assert_eq!(map_tg_pc_action("скрин"), Some("screenshot_save"));
        assert_eq!(map_tg_pc_action("lock"), Some("lock_screen"));
        assert_eq!(map_tg_pc_action("unknown"), None);
    }
}
