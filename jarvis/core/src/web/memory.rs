//! Страница «Память»: досье, конспект, летопись и архив реплик — посмотреть, поправить, удалить.
//! Любая правка отменяет сжатие, которое шло в этот момент (оно повторится само).

use super::{App, bad, reply};
use crate::memory::{Memory, clip};
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::StatusCode,
    response::Response,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

fn mem(app: &App) -> Option<Arc<Memory>> {
    app.engine.tools.memory.clone()
}

fn body<T: for<'de> Deserialize<'de>>(b: &Bytes) -> Option<T> {
    serde_json::from_slice(b).ok()
}

pub async fn get(State(app): State<App>) -> Response {
    let Some(m) = mem(&app) else { return bad("память выключена в этой сборке") };
    let mut st = m.state();
    if let Some(o) = st.as_object_mut() {
        o.remove("recent"); // реплики страница листает сама — через архив
    }
    reply(StatusCode::OK, json!({"memory": st, "settings": super::state::memory_state(&app.store.get(), &app.plugins)}))
}

pub async fn put(State(app): State<App>, b: Bytes) -> Response {
    #[derive(Deserialize)]
    struct Req {
        profile: Option<String>,
        summary: Option<String>,
    }
    let (Some(m), Some(req)) = (mem(&app), body::<Req>(&b)) else { return bad("пустой запрос") };
    if let Some(p) = req.profile {
        if let Err(e) = m.set_profile(&p) {
            return bad(e);
        }
        log::info!("память: досье изменено в панели");
    }
    if let Some(s) = req.summary {
        if let Err(e) = m.set_summary(&s) {
            return bad(e);
        }
        log::info!("память: конспект изменён в панели");
    }
    get(State(app)).await
}

pub async fn month(State(app): State<App>, b: Bytes) -> Response {
    #[derive(Deserialize)]
    struct Req {
        #[serde(default)]
        month: String,
        #[serde(default)]
        text: String,
    }
    let (Some(m), Some(req)) = (mem(&app), body::<Req>(&b)) else { return bad("пустой запрос") };
    if req.month.is_empty() {
        return bad("пустой запрос");
    }
    if let Err(e) = m.set_month(&req.month, &req.text) {
        return bad(e);
    }
    log::info!("память: летопись за {} изменена в панели", req.month);
    get(State(app)).await
}

pub async fn archive(State(app): State<App>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(m) = mem(&app) else { return reply(StatusCode::OK, json!({"turns": []})) };
    let num = |k: &str| q.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    let q_str = clip(q.get("q").map_or("", String::as_str), 200);
    let before = num("before");
    let limit = num("limit").max(0) as usize;
    let (turns, more) = tokio::task::spawn_blocking(move || m.archive_page(&q_str, before, limit))
        .await
        .unwrap_or_default();
    reply(StatusCode::OK, json!({"turns": turns, "more": more}))
}

pub async fn forget(State(app): State<App>, b: Bytes) -> Response {
    #[derive(Deserialize)]
    struct Req {
        #[serde(default)]
        n: Vec<i64>,
    }
    let (Some(m), Some(req)) = (mem(&app), body::<Req>(&b)) else { return bad("какие реплики удалить?") };
    if req.n.is_empty() || req.n.len() > 500 {
        return bad("какие реплики удалить?");
    }
    let ids = req.n;
    let n = tokio::task::spawn_blocking(move || m.delete_turns(&ids))
        .await
        .unwrap_or(0);
    log::info!("память: из архива удалено реплик в панели: {n}");
    reply(StatusCode::OK, json!({"ok": true, "deleted": n}))
}

/// «Обновить досье сейчас»: сжать реплики в досье и конспект, не дожидаясь длинного разговора.
pub async fn compact(State(app): State<App>) -> Response {
    let Some(m) = mem(&app) else { return bad("память выключена в этой сборке") };
    match tokio::time::timeout(std::time::Duration::from_secs(180), m.compact_now()).await {
        Ok(Ok(n)) => reply(StatusCode::OK, json!({"ok": true, "turns": n})),
        Ok(Err(e)) => bad(clip(&e, 300)),
        Err(_) => bad("модель не ответила за 3 минуты"),
    }
}

/// «Начать новый чат» (конспект и недавние реплики забыть, архив остаётся); ?all=1 — стереть всю память.
pub async fn new_chat(State(app): State<App>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(m) = mem(&app) else { return bad("память выключена в этой сборке") };
    if q.get("all").map(String::as_str) == Some("1") {
        m.erase();
        log::info!("память: стёрта целиком из панели");
    } else {
        m.reset();
    }
    reply(StatusCode::OK, json!({"ok": true}))
}

pub async fn rag(State(app): State<App>, b: Bytes) -> Response {
    #[derive(Deserialize)]
    struct Req {
        #[serde(default)]
        query: String,
        #[serde(default)]
        ask: bool,
        limit: Option<usize>,
    }
    let Some(req) = body::<Req>(&b) else { return bad("Неверный формат запроса") };
    let q = req.query.trim();
    if q.is_empty() {
        return bad("Введите поисковый запрос");
    }
    let limit = req.limit.unwrap_or(6).clamp(1, 16);
    let mem_ref = mem(&app);

    let sem = match crate::tools::semantic_search_all(&app.plugins, mem_ref.as_deref(), q, limit).await {
        Ok(v) => v,
        Err(_) => {
            let fallback = mem_ref.as_deref().map(|m| m.search(q, "")).unwrap_or_default();
            return reply(
                StatusCode::OK,
                json!({
                    "ok": true,
                    "query": q,
                    "answer": if req.ask { fallback } else { String::new() },
                    "hits": [],
                    "offline_fallback": true,
                }),
            );
        }
    };

    let hits = sem["hits"].as_array().cloned().unwrap_or_default();
    if !req.ask {
        return reply(
            StatusCode::OK,
            json!({
                "ok": true,
                "query": q,
                "total_notes": sem["total_notes"],
                "answer": "",
                "hits": hits,
            }),
        );
    }

    if hits.is_empty() {
        let local_mem = mem_ref
            .as_deref()
            .map(|m| m.search(q, ""))
            .filter(|s| !s.is_empty() && !s.contains("ничего не нашлось"))
            .unwrap_or_else(|| "В CyberNotes и долгой памяти подходящих записей не найдено.".into());
        return reply(
            StatusCode::OK,
            json!({
                "ok": true,
                "query": q,
                "total_notes": sem["total_notes"],
                "answer": local_mem,
                "hits": [],
            }),
        );
    }

    let mut ctx_blocks = Vec::new();
    let mut total_chars = 0usize;
    for (i, h) in hits.iter().enumerate() {
        let src = if h["source"].as_str() == Some("note") { "Заметка" } else { "Память" };
        let title = h["title"].as_str().unwrap_or("Без названия");
        let folder = h["folder"].as_str().unwrap_or("");
        let snippet = h["snippet"].as_str().unwrap_or("");
        let block = if folder.is_empty() {
            format!("[{}] ({}: {})\n{}", i + 1, src, title, snippet)
        } else {
            format!("[{}] ({}: {}, папка: {})\n{}", i + 1, src, title, folder, snippet)
        };
        if total_chars + block.len() > 5500 {
            break;
        }
        total_chars += block.len();
        ctx_blocks.push(block);
    }

    let s = app.store.get();
    let p = s.active_profile();
    let sys_prompt = "Ты — Джарвис. Дай точный, структурированный ответ на русском языке (в Markdown, коротко и по делу) \
        ТОЛЬКО по приведённым фрагментам из CyberNotes и памяти. Обязательно ссылайся на источники в скобках вроде **[Название]**. \
        Если точного ответа в фрагментах нет — честно скажи об этом.";
    let user_prompt = format!("Вопрос: {q}\n\nФрагменты:\n\n{}", ctx_blocks.join("\n\n---\n\n"));

    let answer = match crate::llm::ask_with_pool(
        &app.plugins,
        &s,
        &p,
        sys_prompt,
        &user_prompt,
        &app.engine.tools,
        Vec::new(),
        Some(&app.engine.gemini_pool),
    )
    .await
    {
        Ok(ans) if !ans.trim().is_empty() => crate::sfx::strip_tags(ans.trim()),
        _ => {
            let bullets: Vec<String> = hits
                .iter()
                .take(5)
                .map(|h| {
                    let title = h["title"].as_str().unwrap_or("Запись");
                    let snip = h["snippet"].as_str().unwrap_or("");
                    format!("- **[{title}]**: {snip}")
                })
                .collect();
            format!("Найдены подходящие записи:\n\n{}", bullets.join("\n"))
        }
    };

    reply(
        StatusCode::OK,
        json!({
            "ok": true,
            "query": q,
            "total_notes": sem["total_notes"],
            "answer": answer,
            "hits": hits,
        }),
    )
}

pub async fn export_notes(State(app): State<App>) -> Response {
    let Some(m) = mem(&app) else { return bad("память выключена в этой сборке") };
    let md = m.export_markdown();
    if md.trim().is_empty() {
        return bad("Память пока пуста — нечего сохранять");
    }
    match crate::tools::save_cybernote(
        &app.plugins,
        "Досье и конспект Джарвиса",
        &md,
        &["память".into(), "досье".into()],
        "Память Джарвиса",
    )
    .await
    {
        Ok(path) => reply(StatusCode::OK, json!({"ok": true, "path": path})),
        Err(e) => bad(e),
    }
}

/// Что записано — для общего состояния панели («chat»).
pub fn chat_state(app: &App) -> Value {
    mem(app).map_or(Value::Null, |m| m.state())
}
