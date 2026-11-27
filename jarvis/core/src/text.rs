use super::*;
use crate::modes::{ModeManager, SpecialMode};

impl Engine {
    pub async fn text_turn(self: Arc<Self>, text: String, speak: bool) {
        let _ = self.ask_text(text, speak).await;
    }

    pub async fn ask_text(self: Arc<Self>, text: String, speak: bool) -> Result<String, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Пустой запрос".into());
        }
        let phrase = self.listener.phrase();
        let command = if phrase == crate::wake_profiles::DEFAULT_PHRASE {
            text.to_owned()
        } else {
            fastcmd::command_for_phrase(text, &phrase)
        };
        let command = command.as_str();
        if fastcmd::is_stop(command) {
            self.tools.stop_all_activities(Some(&self), true);
            let e = self.clone();
            let all = !self.talking() && fastcmd::is_stop_all(command);
            let _ = tokio::task::spawn_blocking(move || {
                if all {
                    e.stop("написали стоп")
                } else {
                    e.end_chat("написали стоп")
                }
            })
            .await;
            self.journal.note("стоп", text, "");
            return Ok("Остановил.".into());
        }
        let profile_names: Vec<(String, String)> = self
            .store
            .with(|s| s.profiles.iter().map(|p| (p.id.clone(), p.name.clone())).collect());
        let mut command_owned: Option<String> = None;
        if profile_names.len() > 1
            && let Some((target_id, tail)) = fastcmd::addressed_profile(command, &profile_names)
        {
            let current_id = self.store.with(|s| s.active.clone());
            if target_id != current_id {
                let _ = self.switch_active_profile_silent(&target_id);
            }
            if !tail.is_empty() {
                command_owned = Some(tail);
            }
        }
        let command = command_owned.as_deref().unwrap_or(command);
        let s = self.store.get();
        let p = s.active_profile();
        let llm_engine = p.effective_llm_engine();
        let llm_model = p.effective_llm_model();
        let model = if !llm_model.is_empty() {
            llm_model
        } else if !llm_engine.is_empty() {
            llm_engine
        } else {
            &p.backend_model
        };
        let id = self.journal.new_turn("текст", model);
        self.journal.update(id, |t| {
            t.text = text.into();
            t.live = true;
        });
        let epoch = self.speech_epoch.load(Ordering::SeqCst);
        let is_bedtime = ModeManager::is_bedtime_command(command);
        let active_mode = self.tools.modes.as_ref().map(|m| m.current()).unwrap_or_default();
        let work = async {
            if let Some(modes) = &self.tools.modes
                && let Some(ans) = modes.try_command(command, self.tools.ambient.as_deref())
            {
                return Ok(ans);
            }
            if active_mode.is_game()
                && let Some(games) = &self.tools.games
                && let Some(ans) = games.handle_turn(&self, command).await
            {
                return Ok(ans);
            }
            if let Some(question) = fastcmd::clarification(command) {
                return Ok(question.into());
            }
            if !is_bedtime && active_mode == SpecialMode::Normal {
                if let Some(answer) = self.tools.fast_local(command) {
                    return Ok(answer);
                }
                if let Some(answer) = self.host.command(command).await {
                    return Ok(answer);
                }
                if let Some(answer) = self.music.control(&self.host, command).await {
                    return Ok(answer);
                }
                if let Some(direct) = crate::music::parse_direct_music_request(command) {
                    let has_yt = self.host.provider("youtube").is_ok();
                    let use_ym = s.use_home("yandexmusic");
                    let res = match direct {
                        crate::music::PlayDirect::Liked { shuffle } if has_yt => {
                            self.tools.stop_audio_for_standalone_music();
                            Some(self.music.play_liked(&self.host, shuffle, false).await)
                        }
                        crate::music::PlayDirect::Playlist { query, shuffle } if has_yt => {
                            self.tools.stop_audio_for_standalone_music();
                            Some(self.music.play_playlist(&self.host, &query, shuffle, false).await)
                        }
                        crate::music::PlayDirect::Yandex { query } if use_ym => {
                            self.tools.stop_audio_for_standalone_music();
                            let _ = self.music.call(&self.host, "stop", serde_json::Value::Null).await;
                            let cmd = crate::tools::format_yandex_music_cmd(&query);
                            Some(match tokio::task::spawn_blocking(move || alice::text(&cmd, true)).await {
                                Ok(Ok(())) => format!("Включил в Яндекс Музыке: {query}."),
                                Ok(Err(e)) => format!("Не получилось: {e}"),
                                Err(e) => format!("Не получилось: {e}"),
                            })
                        }
                        crate::music::PlayDirect::Queue { query } if has_yt => {
                            Some(self.music.play(&self.host, &query, true).await)
                        }
                        crate::music::PlayDirect::Track { query } if has_yt => {
                            self.tools.stop_audio_for_standalone_music();
                            let mut r = self.music.play(&self.host, &query, false).await;
                            if r.starts_with("Не получилось:") && !r.contains("операция отменена") && use_ym {
                                let cmd = crate::tools::format_yandex_music_cmd(&query);
                                if let Ok(Ok(())) = tokio::task::spawn_blocking(move || alice::text(&cmd, true)).await {
                                    r = format!("В YouTube Music не нашлось, поэтому включил в Яндекс Музыке: {query}.");
                                }
                            }
                            Some(r)
                        }
                        _ => None,
                    };
                    if let Some(answer) = res {
                        return Ok(answer);
                    }
                }
                if let Some(answer) = self.tools.ha_vacuum(command).await {
                    return Ok(answer);
                }
                if let Some(answer) = self.tools.smartlife_fast(command).await {
                    return Ok(answer);
                }
                let has_youtube = self.host.provider("youtube").is_ok();
                if s.use_home("alice")
                    && let Some(command) = fastcmd::alice_skill_for_ex(command, s.use_home("yandexmusic"), has_youtube)
                {
                    let answer = format!("Передал Алисе: {command}");
                    tokio::task::spawn_blocking(move || alice::text(&command, false))
                        .await
                        .map_err(|e| e.to_string())??;
                    return Ok(answer);
                }
                if let Some(answer) = self.tools.fast(command).await {
                    return Ok(answer);
                }
            }
            if is_bedtime && let Some(amb) = &self.tools.ambient {
                let _ = amb.start("ocean");
                amb.set_sleep(20);
            }
            let (context, defs) = if matches!(active_mode, SpecialMode::Normal | SpecialMode::Chat) {
                self.tools.prepare().await
            } else {
                (String::new(), Vec::new())
            };
            let whisper_hint = if is_bedtime {
                "\n[Режим «История на ночь»: расскажи тёплую, уютную, атмосферную и законченную историю перед сном. Если хозяин просит большую или длинную сказку — не торопись, раскрой сюжет подробно и образно (15–25 плавных предложений), без Markdown и без вопросов в конце.]"
            } else if s.expressive_mode {
                "\n[При уместности можешь начинать фразу тегом эмоции [happy], [sarcastic], [whisper], [excited], [sad] или вставить короткий эффект [sfx:chime], [sfx:bell], [sfx:sigh], но не злоупотребляй.]"
            } else {
                ""
            };
            let base_rules = format!("{}{}", rules::TEXT_RULES, rules::BACKEND_RULES);
            let instructions = ModeManager::build_prompt(
                &active_mode,
                &self.instructions(&s, &p),
                &base_rules,
                &context,
                whisper_hint,
            );
            if active_mode == SpecialMode::Normal {
                crate::llm::ask_with_pool(
                    &self.host,
                    &s,
                    &p,
                    &instructions,
                    text,
                    &self.tools,
                    defs,
                    Some(&self.gemini_pool),
                )
                .await
            } else {
                let history = self
                    .tools
                    .modes
                    .as_ref()
                    .map(|m| m.history())
                    .unwrap_or_default();
                let reply = crate::llm::ask_with_history_detail_pool(
                    &self.host,
                    &s,
                    &p,
                    &instructions,
                    &history,
                    text,
                    &self.tools,
                    defs,
                    Some(&self.gemini_pool),
                )
                .await?;
                if let Some(modes) = &self.tools.modes {
                    modes.push_turn(text, &reply.text);
                }
                Ok(reply.text)
            }
        };
        let result = tokio::select! {
            value=tokio::time::timeout(Duration::from_secs(150),work)=>value.unwrap_or_else(|_|Err("Запрос не завершился за 150 с".into())),
            _=async{
                loop {
                    let wait = self.speech_notify.notified();
                    if self.speech_epoch.load(Ordering::SeqCst) != epoch { break; }
                    wait.await;
                }
            }=>Err("Запрос отменён".into()),
        };
        let result = result.map(|raw| raw);
        let display_result = result.as_ref().map(|ans| crate::sfx::strip_tags(ans)).map_err(|e| e.clone());
        self.journal.update(id, |t| {
            t.live = false;
            match &display_result {
                Ok(answer) => t.answer = answer.clone(),
                Err(error) => t.error = error.clone(),
            }
        });
        self.journal.remember(id);
        if speak && self.speech_epoch.load(Ordering::SeqCst) == epoch {
            if let Ok(ref answer) = result {
                let ans = answer.clone();
                let eng = self.clone();
                let _ = tokio::task::spawn_blocking(move || eng.say_text_whisper(&ans, is_bedtime)).await;
            }
        }
        display_result
    }
}
