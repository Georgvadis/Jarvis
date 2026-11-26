//! Читалка книг / ранобэ / фанфиков (`EPUB`/`FB2`/`TXT`/PDF/URL) с закладками
//! и опциональным многоголосием для диалогов. Тяжёлый парсинг выполняется на `jarvis-voice`.

use crate::engine::Engine;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Notify;

#[derive(Default)]
struct ReaderState {
    running: bool,
    generation: u64,
    book_id: String,
    title: String,
    chapter: usize,
    chapter_title: String,
    chapters_count: usize,
    para: usize,
    chapter_paras: usize,
    multi_voice: bool,
    last_error: String,
}

pub struct BookReader {
    st: Mutex<ReaderState>,
    wake: Notify,
}

impl BookReader {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            st: Mutex::new(ReaderState::default()),
            wake: Notify::new(),
        })
    }

    pub fn is_running(&self) -> bool {
        self.st.lock().unwrap().running
    }

    pub fn status(&self) -> Value {
        let g = self.st.lock().unwrap();
        json!({
            "running": g.running,
            "book_id": g.book_id,
            "title": g.title,
            "chapter": g.chapter,
            "chapter_title": g.chapter_title,
            "chapters_count": g.chapters_count,
            "para": g.para,
            "chapter_paras": g.chapter_paras,
            "multi_voice": g.multi_voice,
            "last_error": g.last_error,
        })
    }

    pub fn stop(&self) {
        let mut g = self.st.lock().unwrap();
        if g.running {
            g.running = false;
            g.generation = g.generation.wrapping_add(1);
            drop(g);
            self.wake.notify_waiters();
        }
    }

    pub async fn start(
        self: &Arc<Self>,
        engine: Arc<Engine>,
        book_id: Option<String>,
        multi_voice: Option<bool>,
    ) -> Result<Value, String> {
        engine.tools.stop_for(&engine, "reader");
        let srv = crate::tools::local_voice_url(&engine.host);
        let chosen_id = if let Some(id) = book_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            id
        } else {
            let current = self.st.lock().unwrap().book_id.clone();
            if !current.is_empty() {
                current
            } else {
                let list_resp = crate::net::direct()
                    .get(format!("{srv}/books"))
                    .timeout(Duration::from_secs(6))
                    .send()
                    .await
                    .map_err(|e| format!("Сервер книг недоступен: {e}"))?;
                let v: Value = list_resp.json().await.unwrap_or_default();
                let first = v["books"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|b| b["id"].as_str())
                    .map(str::to_string)
                    .unwrap_or_default();
                if first.is_empty() {
                    return Err("Библиотека пуста — сначала загрузите книгу в веб-панели".into());
                }
                first
            }
        };

        let mv = multi_voice.unwrap_or_else(|| self.st.lock().unwrap().multi_voice);
        let generation = {
            let mut g = self.st.lock().unwrap();
            g.running = true;
            g.generation = g.generation.wrapping_add(1);
            g.book_id = chosen_id.clone();
            g.multi_voice = mv;
            g.last_error.clear();
            g.generation
        };
        self.wake.notify_waiters();

        let me = self.clone();
        let eng = engine.clone();
        tokio::spawn(async move {
            me.run_loop(eng, chosen_id, generation).await;
        });

        Ok(self.status())
    }

    async fn run_loop(self: Arc<Self>, engine: Arc<Engine>, book_id: String, generation: u64) {
        let srv = crate::tools::local_voice_url(&engine.host);
        let s = engine.store.get();
        let (_p1, dialogue_prof) =
            crate::debate::resolve_duo(&engine, &s, None, None, "Персонаж").await;
        let epoch = engine.speech_epoch_now();

        let (mut cur_ch, mut cur_para) = (-1_i64, -1_i64);
        let mut last_chapter_announced: Option<usize> = None;

        loop {
            if self.cancelled(generation) || engine.speech_epoch_now() != epoch {
                break;
            }

            let url = if cur_ch >= 0 && cur_para >= 0 {
                format!("{srv}/books/{book_id}/slice?chapter={cur_ch}&para={cur_para}&count=5")
            } else {
                format!("{srv}/books/{book_id}/slice?count=5")
            };
            let slice_val: Value = match crate::net::direct()
                .get(&url)
                .timeout(Duration::from_secs(10))
                .send()
                .await
            {
                Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
                Ok(r) => {
                    self.fail(generation, format!("Ошибка загрузки книги HTTP {}", r.status()));
                    break;
                }
                Err(e) => {
                    self.fail(generation, format!("Сервер книг недоступен: {e}"));
                    break;
                }
            };

            let title = slice_val["title"].as_str().unwrap_or("Книга").to_string();
            let ch_cnt = slice_val["chapters_count"].as_u64().unwrap_or(1) as usize;
            let ch_paras = slice_val["chapter_paras"].as_u64().unwrap_or(0) as usize;
            {
                let mut g = self.st.lock().unwrap();
                if g.generation != generation {
                    break;
                }
                g.title = title.clone();
                g.chapters_count = ch_cnt;
                g.chapter = slice_val["chapter"].as_u64().unwrap_or(0) as usize;
                g.chapter_title = slice_val["chapter_title"].as_str().unwrap_or("").to_string();
                g.para = slice_val["para"].as_u64().unwrap_or(0) as usize;
                g.chapter_paras = ch_paras;
            }

            let Some(items) = slice_val["items"].as_array().filter(|a| !a.is_empty()) else {
                let done_eng = engine.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = done_eng.say_aloud(
                        &format!("[sfx:bell] Книга «{title}» дочитана до конца."),
                        Duration::from_secs(20),
                    );
                })
                .await;
                break;
            };

            for item in items {
                if self.cancelled(generation) || engine.speech_epoch_now() != epoch {
                    self.mark_stopped(generation);
                    return;
                }
                let ch = item["chapter"].as_u64().unwrap_or(0) as usize;
                let p = item["para"].as_u64().unwrap_or(0) as usize;
                let ch_title = item["chapter_title"].as_str().unwrap_or("");
                let raw_text = item["text"].as_str().unwrap_or("").trim();
                let is_dialogue = item["role"].as_str() == Some("dialogue");

                if last_chapter_announced != Some(ch) {
                    last_chapter_announced = Some(ch);
                    if !ch_title.is_empty() && p == 0 {
                        engine.journal.note("книга", &format!("{title} — {ch_title}"), "");
                        let e = engine.clone();
                        let intro = format!("[sfx:chime] {ch_title}.");
                        let _ = tokio::task::spawn_blocking(move || {
                            let _ = e.say_aloud(&intro, Duration::from_secs(20));
                        })
                        .await;
                    }
                }
                if raw_text.is_empty() {
                    cur_ch = ch as i64;
                    cur_para = (p + 1) as i64;
                    continue;
                }

                // Сохраняем закладку на текущем абзаце перед озвучкой
                {
                    let mut g = self.st.lock().unwrap();
                    if g.generation != generation {
                        return;
                    }
                    g.chapter = ch;
                    g.para = p;
                    g.chapter_title = ch_title.to_string();
                }
                let _ = save_bookmark(&srv, &book_id, ch, p).await;

                let multi = self.st.lock().unwrap().multi_voice;
                let prof_override = if multi && is_dialogue {
                    Some(dialogue_prof.clone())
                } else {
                    None
                };

                let e = engine.clone();
                let line = raw_text.to_string();
                let res = tokio::task::spawn_blocking(move || {
                    e.say_aloud_opts(&line, Duration::from_secs(75), prof_override.as_ref(), false)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));

                if let Err(err) = res {
                    if err != crate::engine::INTERRUPTED {
                        self.fail(generation, err);
                    }
                    self.mark_stopped(generation);
                    return;
                }

                // Продвигаем закладку на следующий абзац после успешного прочтения
                cur_ch = ch as i64;
                cur_para = (p + 1) as i64;
                let _ = save_bookmark(&srv, &book_id, ch, p + 1).await;
            }
        }

        self.mark_stopped(generation);
    }

    fn cancelled(&self, generation: u64) -> bool {
        let g = self.st.lock().unwrap();
        !g.running || g.generation != generation
    }

    fn mark_stopped(&self, generation: u64) {
        let mut g = self.st.lock().unwrap();
        if g.generation == generation {
            g.running = false;
        }
    }

    fn fail(&self, generation: u64, err: String) {
        log::warn!("reader: {err}");
        let mut g = self.st.lock().unwrap();
        if g.generation == generation {
            g.running = false;
            g.last_error = err;
        }
    }

    pub async fn step_chapter(self: &Arc<Self>, engine: Arc<Engine>, delta: i32) -> Result<String, String> {
        let (was_running, book_id, cur_ch, ch_cnt) = {
            let g = self.st.lock().unwrap();
            (g.running, g.book_id.clone(), g.chapter, g.chapters_count.max(1))
        };
        if book_id.is_empty() {
            return Err("Книга ещё не выбрана".into());
        }
        let target_ch = (cur_ch as i32 + delta).clamp(0, (ch_cnt - 1) as i32) as usize;
        let srv = crate::tools::local_voice_url(&engine.host);
        save_bookmark(&srv, &book_id, target_ch, 0).await?;
        {
            let mut g = self.st.lock().unwrap();
            g.chapter = target_ch;
            g.para = 0;
        }
        if was_running {
            self.start(engine, Some(book_id), None).await?;
        }
        Ok(format!("Перешёл к главе {}", target_ch + 1))
    }

    pub async fn recap(&self, engine: Arc<Engine>) -> Result<String, String> {
        let srv = crate::tools::local_voice_url(&engine.host);
        let mut book_id = self.st.lock().unwrap().book_id.clone();
        if book_id.is_empty() {
            let list_resp = crate::net::direct()
                .get(format!("{srv}/books"))
                .timeout(Duration::from_secs(6))
                .send()
                .await
                .map_err(|e| format!("Сервер книг недоступен: {e}"))?;
            let v: Value = list_resp.json().await.unwrap_or_default();
            book_id = v["books"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|b| b["id"].as_str())
                .unwrap_or("")
                .to_string();
        }
        if book_id.is_empty() {
            return Err("Нет открытой книги".into());
        }
        let slice_val: Value = crate::net::direct()
            .get(format!("{srv}/books/{book_id}/slice?count=2"))
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|e| format!("Сервер книг недоступен: {e}"))?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let title = slice_val["title"].as_str().unwrap_or("Книга");
        let ch_title = slice_val["chapter_title"].as_str().unwrap_or("");
        let recent = slice_val["recent_text"].as_str().unwrap_or("").trim();
        if recent.is_empty() {
            return Ok(format!("Ты в самом начале книги «{title}» ({ch_title})."));
        }

        let s = engine.store.get();
        let p = s.active_profile();
        let sys = "Коротко и атмосферно напомни слушателю по-русски в 2–3 предложениях, на чём именно закончилось чтение книги по этому отрывку.";
        let prompt = format!("Книга «{title}», {ch_title}.\nПоследний фрагмент:\n{recent}");
        crate::llm::ask_with_pool(
            &engine.host,
            &s,
            &p,
            sys,
            &prompt,
            &engine.tools,
            Vec::new(),
            Some(&engine.gemini_pool),
        )
        .await
    }

    pub fn fast_command(self: &Arc<Self>, engine: &Arc<Engine>, command: &str) -> Option<String> {
        let low = command.trim().to_lowercase();
        let mentions_book = low.contains("книг") || low.contains("раноб") || low.contains("глав");
        if !mentions_book {
            return None;
        }
        if low.contains("останови") || low.contains("стоп") || low.contains("пауз") || low.contains("хватит") {
            if self.is_running() {
                self.stop();
                return Some("Поставил книгу на паузу.".into());
            }
            return None;
        }
        if low.contains("следующая глава") || low.contains("следующую главу") {
            let me = self.clone();
            let eng = engine.clone();
            tokio::spawn(async move {
                let _ = me.step_chapter(eng, 1).await;
            });
            return Some("Переключаю на следующую главу.".into());
        }
        if low.contains("предыдущая глава") || low.contains("предыдущую главу") {
            let me = self.clone();
            let eng = engine.clone();
            tokio::spawn(async move {
                let _ = me.step_chapter(eng, -1).await;
            });
            return Some("Возвращаю предыдущую главу.".into());
        }
        if low.contains("продолжи") || low.contains("читай") || low.contains("почитай") {
            let me = self.clone();
            let eng = engine.clone();
            tokio::spawn(async move {
                if let Err(e) = me.start(eng.clone(), None, None).await {
                    eng.say_text(&e);
                }
            });
            return Some("Открываю книгу с твоей закладки.".into());
        }
        None
    }
}

async fn save_bookmark(srv: &str, book_id: &str, chapter: usize, para: usize) -> Result<(), String> {
    let resp = crate::net::direct()
        .post(format!("{srv}/books/{book_id}/bookmark"))
        .json(&json!({"chapter": chapter, "para": para}))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("Не удалось сохранить закладку: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Ошибка сохранения закладки HTTP {}", resp.status()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_initial_state_and_stop_are_safe() {
        let r = BookReader::new();
        assert!(!r.is_running());
        assert_eq!(r.status()["running"], false);
        r.stop();
        assert!(!r.is_running());
    }
}
