//! Classic STT → tools/LLM → TTS, without a Go process.
use crate::{modes::{ModeManager, SpecialMode}, plugins::Host, settings::{Settings, Profile}, tools::Tools, journal::Journal, session::{Session, Dialog}};
use std::{sync::{Arc,Mutex,atomic::{AtomicBool,AtomicU64,Ordering},mpsc::{Receiver,SyncSender,sync_channel}},time::Duration};
use tokio::sync::mpsc;
use serde_json::json;
use base64::{Engine as _,engine::general_purpose::STANDARD as B64};

pub struct Classic {
    dialog:Dialog,frames:mpsc::Sender<Vec<i16>>,say:mpsc::Sender<String>,
    out:Mutex<Option<SyncSender<Vec<i16>>>>,audio:Mutex<Option<Receiver<Vec<i16>>>>,
    closed:AtomicBool,generation:AtomicU64,interrupted:AtomicBool,stop:tokio::sync::Notify,
    last_failure:Mutex<Option<crate::voice_error::VoiceFailure>>,
}

struct Context {
    host:Arc<Host>,tools:Arc<Tools>,settings:Settings,profile:Profile,instructions:String,phrase:String,
    history:Mutex<Vec<crate::llm::Turn>>,
    gemini_pool:Option<Arc<crate::gemini_pool::GeminiPool>>,
    proxy:Option<Arc<crate::vpn::Proxy>>,
}
impl Classic {
    pub fn open(rt:&tokio::runtime::Handle,host:Arc<Host>,tools:Arc<Tools>,journal:Arc<Journal>,settings:Settings,profile:Profile,instructions:String,phrase:String)->Arc<Self>{
        Self::open_with(rt,host,tools,journal,settings,profile,instructions,phrase,None,None)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn open_with(rt:&tokio::runtime::Handle,host:Arc<Host>,tools:Arc<Tools>,journal:Arc<Journal>,settings:Settings,profile:Profile,instructions:String,phrase:String,gemini_pool:Option<Arc<crate::gemini_pool::GeminiPool>>,proxy:Option<Arc<crate::vpn::Proxy>>)->Arc<Self>{
        let (out,audio)=sync_channel(512);let (frames,mut input)=mpsc::channel(64);let (say,mut messages)=mpsc::channel(8);
        let model={let m=profile.effective_llm_model();let e=profile.effective_llm_engine();if !m.is_empty(){m}else if !e.is_empty(){e}else{&profile.backend_model}};
        // Подтягиваем последние 6 реплик из постоянной памяти (переживает перезагрузку колонки),
        // а если память выключена или пуста — из оперативного журнала за 30 минут.
        let mut initial_history: Vec<crate::llm::Turn> = Vec::new();
        if let Some(mem) = &tools.memory {
            for t in mem.recent_turns(6) {
                let u = t.user.trim();
                let a = t.bot.trim();
                if !u.is_empty() && !a.is_empty() {
                    initial_history.push(crate::llm::Turn { role: "user", content: u.to_owned() });
                    initial_history.push(crate::llm::Turn { role: "assistant", content: a.to_owned() });
                }
            }
        }
        if initial_history.is_empty() {
            for t in journal.history().into_iter().rev().take(6).collect::<Vec<_>>().into_iter().rev() {
                let u = t.text.trim();
                let a = t.answer.trim();
                if !u.is_empty() && !a.is_empty() && t.error.is_empty() {
                    initial_history.push(crate::llm::Turn { role: "user", content: u.to_owned() });
                    initial_history.push(crate::llm::Turn { role: "assistant", content: a.to_owned() });
                }
            }
        }
        let me=Arc::new(Self{dialog:Dialog::new(journal,"классический",model),frames,say,out:Mutex::new(Some(out)),audio:Mutex::new(Some(audio)),closed:AtomicBool::new(false),generation:AtomicU64::new(0),interrupted:AtomicBool::new(false),stop:tokio::sync::Notify::new(),last_failure:Mutex::new(None)});
        let actor=me.clone();let context=Arc::new(Context{host,tools,settings,profile,instructions,phrase,history:Mutex::new(initial_history),gemini_pool,proxy});
        rt.spawn(async move {
            let mut vad=Capture::new(context.settings.vad_threshold, context.settings.vad_silence_ms);let mut tasks=tokio::task::JoinSet::new();
            loop {tokio::select! {
                biased;
                _=actor.stop.notified()=>break,
                Some(text)=messages.recv()=>{
                    tasks.abort_all();let generation=actor.generation.fetch_add(1,Ordering::SeqCst)+1;
                    let me=actor.clone();let context=context.clone();
                    actor.dialog.working(1);
                    tasks.spawn(async move{me.speak(&context,&text,generation,false).await});
                },
                result=tasks.join_next(),if !tasks.is_empty()=>{
                    actor.dialog.working(-1);
                    if let Some(Ok(Err(error)))=result {
                        actor.dialog.finish_turn(&error);
                        if actor.last_failure.lock().unwrap().is_some() {
                            break;
                        }
                    }
                },
                Some(pcm)=input.recv()=>{
                    let starting=!vad.speaking && vad.is_loud(&pcm);
                    if starting {tasks.abort_all();actor.generation.fetch_add(1,Ordering::SeqCst);actor.interrupted.store(true,Ordering::SeqCst);}
                    if let Some(pcm)=vad.feed(&pcm){
                        if context.settings.thinking_sound {
                            if let Some(tx)=actor.out.lock().unwrap().clone() {
                                let _=tx.try_send(thinking_chime_for(&context.settings.sound_theme));
                            }
                        }
                        let me=actor.clone();let context=context.clone();let generation=actor.generation.load(Ordering::SeqCst);
                        actor.dialog.working(1);tasks.spawn(async move{me.turn(&context,pcm,generation).await});
                    }
                },
            }}
            tasks.shutdown().await;actor.out.lock().unwrap().take();
        });me
    }
    fn record_failure(&self, context: &Context, provider: &str, stage: crate::voice_error::Stage, err: &str) {
        let vpn_blocked = context.proxy.as_ref().is_some_and(|p| matches!(p.route(provider), crate::vpn::Route::Blocked(_)));
        *self.last_failure.lock().unwrap() = Some(crate::voice_error::classify(provider, stage, err, None, vpn_blocked));
    }
    async fn turn(self: &Arc<Self>,context:&Arc<Context>,pcm:Vec<i16>,generation:u64)->Result<(),String>{
        let utt_peak = crate::engine::peak(&pcm);
        let whisper_ceiling = (context.settings.vad_threshold.clamp(100, 3000) as f64) * 2.3;
        let is_whisper = context.settings.whisper_mode && utt_peak > 0.0 && utt_peak < whisper_ceiling;
        let stt=context.profile.effective_stt_engine();
        let wav_b64=B64.encode(wav(&pcm));
        let t_stt0 = std::time::Instant::now();
        let response=match stt {
            "openai"|"elevenlabs"=>{
                let key=if stt=="openai"{&context.settings.openai_key}else{&context.settings.elevenlabs_key};
                let model=if context.profile.stt_model.is_empty(){if stt=="openai"{"gpt-4o-mini-transcribe"}else{"scribe_v2"}}else{&context.profile.stt_model};
                match context.host.provider(stt)?.request(stt,json!({"op":"stt","key":key,"model":model,"language":context.settings.language,"audio":wav_b64}),Duration::from_secs(45)).await {
                    Ok(v) => v,
                    Err(e) => { self.record_failure(context, stt, crate::voice_error::Stage::Connect, &e); return Err(e); }
                }
            },
            id=>match context.host.plugin_stt(id,json!({"audio_b64":wav_b64,"format":"wav","rate":16000,"model":context.profile.stt_model,"language":context.settings.language})).await {
                Ok(v) => v,
                Err(e) => { self.record_failure(context, id, crate::voice_error::Stage::Connect, &e); return Err(e); }
            },
        };
        let stt_ms = t_stt0.elapsed().as_millis() as u64;
        let text=response.as_str().or_else(||response["text"].as_str()).unwrap_or("").trim();if text.is_empty(){return Ok(());}
        self.dialog.on_user(text);
        self.dialog.set_timings(Some(stt_ms), None, None);
        let command = if context.phrase == crate::wake_profiles::DEFAULT_PHRASE { text.to_owned() }
            else { crate::fastcmd::command_for_phrase(text, &context.phrase) };
        let command = command.as_str();
        if crate::fastcmd::is_stop(command){
            context.tools.stop_all_activities(context.tools.engine().as_deref(), true);
            self.dialog.annotate("стоп","");self.dialog.finish_turn("");self.close();return Ok(());
        }
        if let Some(modes) = &context.tools.modes
            && let Some(mode_reply) = modes.try_command(command, context.tools.ambient.as_deref())
        {
            self.dialog.on_bot(&crate::sfx::strip_tags(&mode_reply));
            self.speak(context, &mode_reply, generation, is_whisper).await?;
            self.dialog.finish_turn("");
            return Ok(());
        }
        if let Some(question)=crate::fastcmd::clarification(command) {
            self.dialog.annotate("уточнение",question);self.speak(context,question,generation,is_whisper).await?;self.dialog.finish_turn("");return Ok(());
        }
        let is_bedtime = ModeManager::is_bedtime_command(command);
        let active_mode = context.tools.modes.as_ref().map(|m| m.current()).unwrap_or_default();
        if active_mode.is_game()
            && let Some(games) = &context.tools.games
            && let Some(eng) = context.tools.engine()
            && let Some(game_reply) = games.handle_turn(&eng, command).await
        {
            self.dialog.on_bot(&crate::sfx::strip_tags(&game_reply));
            self.speak(context, &game_reply, generation, is_whisper).await?;
            self.dialog.finish_turn("");
            return Ok(());
        }
        if !is_bedtime && active_mode == SpecialMode::Normal {
            if let Some(answer) = context.tools.fast_local(command) {
                self.dialog.on_bot(&answer);
                self.speak(context, &answer, generation, is_whisper).await?;
                self.dialog.finish_turn("");
                return Ok(());
            }
            if let Some(control)=&context.tools.music_fast {
                let answer=control(command.to_string()).await;
                if let Some(answer)=answer{self.dialog.on_bot(&answer);self.speak(context,&answer,generation,is_whisper).await?;self.dialog.finish_turn("");return Ok(());}
            }
            let has_youtube = context.host.provider("youtube").is_ok();
            if context.settings.use_home("alice")
                && let Some(command)=crate::fastcmd::alice_skill_for_ex(command,context.settings.use_home("yandexmusic"),has_youtube)
            {
                tokio::task::spawn_blocking(move||crate::alice::text(&command,false)).await.map_err(|e|e.to_string())??;
                self.dialog.on_bot("Передал Алисе");self.dialog.finish_turn("");return Ok(());
            }
        }
        if is_bedtime && let Some(amb) = &context.tools.ambient {
            let _ = amb.start("ocean");
            amb.set_sleep(15);
        }
        let is_whisper = is_whisper || is_bedtime;
        let mut close_after = false;
        let mut already_spoken = false;
        let answer=if !is_bedtime && active_mode == SpecialMode::Normal {
            if let Some(answer)=context.host.command(command).await{Some(answer)}
            else if let Some(answer)=context.tools.ha_vacuum(command).await{Some(answer)}
            else if let Some(answer)=context.tools.smartlife_fast(command).await{Some(answer)}
            else { context.tools.fast(command).await }
        } else { None };
        let answer = if let Some(a) = answer { a } else {
            let history = if active_mode == SpecialMode::Normal {
                context.history.lock().unwrap().clone()
            } else {
                context.tools.modes.as_ref().map(|m| m.history()).unwrap_or_default()
            };
            let (extra,defs)=if matches!(active_mode, SpecialMode::Normal | SpecialMode::Chat) { context.tools.prepare().await } else { (String::new(), Vec::new()) };
            let llm_provider = {
                let e = context.profile.effective_llm_engine();
                if e.is_empty() { "classic" } else { e }
            };
            let whisper_hint = if is_bedtime {
                "\n[Режим «История на ночь»: расскажи тёплую, уютную, атмосферную и законченную историю перед сном. Если хозяин просит большую или длинную сказку — не торопись, раскрой сюжет подробно и образно (15–25 плавных предложений), без Markdown и без вопросов в конце.]"
            } else if is_whisper {
                "\n[Хозяин говорит шёпотом — ответь максимально коротко, тихо и спокойно.]"
            } else if context.settings.expressive_mode {
                "\n[При уместности можешь начинать фразу тегом эмоции [happy], [sarcastic], [whisper], [excited], [sad] или вставить короткий эффект [sfx:chime], [sfx:bell], [sfx:sigh], [sfx:thunder], [sfx:magic], но не злоупотребляй.]"
            } else {
                ""
            };
            let sys_prompt = ModeManager::build_prompt(
                &active_mode,
                &context.instructions,
                crate::rules::BACKEND_RULES,
                &extra,
                whisper_hint,
            );
            let t_llm0 = std::time::Instant::now();
            let can_stream_chatgpt = llm_provider == "chatgpt"
                && crate::chatgpt::signed_in(&context.settings.chatgpt)
                && matches!(active_mode, SpecialMode::Normal | SpecialMode::Chat);
            let streamed_reply = if can_stream_chatgpt {
                if let Ok(prov) = context.host.provider("chatgpt")
                    && let Ok(auth) = prov.request("auth.token", json!(null), Duration::from_secs(20)).await
                {
                    let (delta_tx, delta_rx) = mpsc::unbounded_channel::<String>();
                    let me_s = self.clone();
                    let ctx_s = context.clone();
                    let stream_task = tokio::spawn(async move {
                        me_s.speak_stream_deltas(&ctx_s, delta_rx, generation, is_whisper).await
                    });
                    match crate::chatgpt::ask_codex_with_history_detail_stream(
                        &context.host,
                        auth,
                        &context.profile,
                        &sys_prompt,
                        &history,
                        text,
                        Some(&context.tools),
                        defs.clone(),
                        Some(delta_tx),
                    )
                    .await
                    {
                        Ok(r) => {
                            let spoken = stream_task.await.ok().and_then(|res| res.ok()).unwrap_or(0);
                            if spoken > 0 {
                                already_spoken = true;
                            }
                            Some(crate::llm::AskReply { text: r.text, close_dialog: r.close_dialog })
                        }
                        Err(e) => {
                            stream_task.abort();
                            log::warn!("ChatGPT stream: {e}, пробую стандартный путь/фолбэк");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let reply = if let Some(r) = streamed_reply {
                r
            } else {
                match crate::llm::ask_with_history_detail_pool(&context.host,&context.settings,&context.profile,&sys_prompt,&history,text,&context.tools,defs,context.gemini_pool.as_deref()).await {
                    Ok(r) => r,
                    Err(e) => { self.record_failure(context, llm_provider, crate::voice_error::Stage::Connect, &e); return Err(e); }
                }
            };
            self.dialog.set_timings(None, Some(t_llm0.elapsed().as_millis() as u64), None);
            close_after = active_mode == SpecialMode::Normal && reply.close_dialog && !crate::music::wants_keep_talking(text);
            let display_reply = strip_web_citations(&crate::sfx::strip_tags(&reply.text));
            if self.generation.load(Ordering::SeqCst)==generation{
                if active_mode == SpecialMode::Normal {
                    let mut h=context.history.lock().unwrap();
                    h.push(crate::llm::Turn{role:"user",content:text.to_owned()});
                    h.push(crate::llm::Turn{role:"assistant",content:display_reply.clone()});
                    if h.len()>12{let drop_n=h.len()-12;h.drain(..drop_n);}
                } else if let Some(modes) = &context.tools.modes {
                    modes.push_turn(text, &display_reply);
                }
            }
            reply.text
        };
        if self.generation.load(Ordering::SeqCst)!=generation{return Ok(());}
        self.dialog.set_bot(&strip_web_citations(&crate::sfx::strip_tags(&answer)));
        if close_after {
            self.dialog.finish_turn("");
            self.dialog.request_close();
            return Ok(());
        }
        if !already_spoken {
            self.speak(context,&answer,generation,is_whisper).await?;
        }
        self.dialog.finish_turn("");Ok(())
    }
    async fn synthesize_provider(&self,context:&Context,provider:&str,voice:&str,model:&str,text:&str,emotion:Option<&str>)->Result<Vec<i16>,String>{
        let result=match provider {
            "openai"=>{
                let model=if model.is_empty(){"gpt-4o-mini-tts"}else{model};
                let voice=if voice.is_empty(){"alloy"}else{voice};
                let body=json!({"model":model,"voice":voice,"input":text});
                context.host.provider("openai")?.request("openai",json!({"op":"tts","key":&context.settings.openai_key,"voice":voice,"body":body}),Duration::from_secs(60)).await?
            },
            "elevenlabs"=>{
                let model=if model.is_empty(){crate::settings::ELEVEN_DEFAULT_MODEL}else{model};
                let body=json!({"text":text,"model_id":model});
                context.host.provider("elevenlabs")?.request("elevenlabs",json!({"op":"tts","key":&context.settings.elevenlabs_key,"voice":voice,"body":body}),Duration::from_secs(60)).await?
            },
            id=>context.host.plugin_tts(id,json!({"text":text,"voice":voice,"model":model,"language":context.settings.language,"emotion":emotion.unwrap_or("")})).await?,
        };
        let b64=result["audio"].as_str().or_else(||result["audio_b64"].as_str()).ok_or("Нет голоса TTS")?;
        let bytes=B64.decode(b64).map_err(|_|"Неверный голос TTS")?;
        let rate=result["rate"].as_u64().unwrap_or(24000) as usize;
        if !matches!(rate,16000|24000|48000) || bytes.len()%2!=0{return Err("Неверный формат TTS".into());}
        let samples:Vec<i16>=bytes.chunks_exact(2).map(|b|i16::from_le_bytes([b[0],b[1]])).collect();
        Ok(resample_pcm(&samples,rate,24000))
    }
    async fn synthesize_one(&self,context:&Context,text:&str,emotion:Option<&str>)->Result<Vec<i16>,String>{
        let provider=context.profile.effective_tts_engine();
        let voice=if !context.profile.tts_voice.is_empty(){context.profile.tts_voice.as_str()}else if !context.profile.voice.is_empty(){context.profile.voice.as_str()}else if provider=="openai"{"alloy"}else{""};
        let cache_key = if text.chars().count() <= 60 {
            Some(tts_cache_key(provider, voice, &context.profile.tts_model, emotion.unwrap_or(""), text))
        } else {
            None
        };
        if let Some(ref key) = cache_key
            && let Some(cached) = tts_cache_get(key)
        {
            return Ok(cached);
        }
        let primary = match self.synthesize_provider(context, provider, voice, &context.profile.tts_model, text, emotion).await {
            Ok(pcm) => Ok(pcm),
            Err(first_err) => {
                // Один быстрый повтор при кратковременном сетевом/API сбое, чтобы длинная сказка не обрывалась на середине
                log::warn!("TTS «{provider}»: повтор после сбоя ({first_err})");
                tokio::time::sleep(Duration::from_millis(450)).await;
                self.synthesize_provider(context, provider, voice, &context.profile.tts_model, text, emotion).await
            }
        };
        match primary {
            Ok(pcm) => {
                if let Some(ref key) = cache_key {
                    tts_cache_put(key, &pcm);
                }
                Ok(pcm)
            }
            Err(primary_err) => {
                let fallback = context.profile.tts_fallback_engine.trim();
                if !fallback.is_empty() && fallback != provider {
                    log::warn!("TTS: основной провайдер «{provider}» не ответил ({primary_err}), пробую резервный «{fallback}»");
                    match self.synthesize_provider(context, fallback, "", "", text, emotion).await {
                        Ok(pcm) => return Ok(pcm),
                        Err(fb_err) => {
                            log::warn!("TTS: резервный «{fallback}» тоже не ответил: {fb_err}");
                            self.record_failure(context, fallback, crate::voice_error::Stage::Connect, &fb_err);
                            return Err(fb_err);
                        }
                    }
                }
                self.record_failure(context, provider, crate::voice_error::Stage::Connect, &primary_err);
                Err(primary_err)
            }
        }
    }
    async fn send_pcm_samples(&self, samples: &[i16], generation: u64) -> Result<(), String> {
        for chunk in samples.chunks(1200) {
            let mut pcm = chunk.to_vec();
            loop {
                if self.closed.load(Ordering::SeqCst) || self.generation.load(Ordering::SeqCst) != generation {
                    return Ok(());
                }
                let tx = self.out.lock().unwrap().clone().ok_or("Разговор закрыт")?;
                match tx.try_send(pcm) {
                    Ok(()) => break,
                    Err(std::sync::mpsc::TrySendError::Full(data)) => pcm = data,
                    Err(_) => return Ok(()),
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        Ok(())
    }
    /// Синтезирует один потоковый фрагмент (сцену/SFX/речь) в готовый список PCM-операций для конвейера.
    async fn synth_stream_chunk(
        self: &Arc<Self>,
        context: &Arc<Context>,
        text: &str,
        generation: u64,
        whisper: bool,
    ) -> Result<Vec<StreamAudioOp>, String> {
        let segments = crate::sfx::parse_segments(text);
        let mut ops = Vec::new();
        for seg in segments {
            if self.closed.load(Ordering::SeqCst) || self.generation.load(Ordering::SeqCst) != generation {
                return Ok(Vec::new());
            }
            match seg {
                crate::sfx::Segment::Scene(scene) => ops.push(StreamAudioOp::Scene(scene)),
                crate::sfx::Segment::Sfx(name) => {
                    if let Some(pcm) = crate::sfx::generate_sfx(&name) {
                        ops.push(StreamAudioOp::Pcm { samples: pcm, led_color: None });
                    }
                }
                crate::sfx::Segment::Speech { text: seg_text, emotion, speaker } => {
                    let cleaned = clean_for_tts(&seg_text);
                    if !cleaned.chars().any(char::is_alphanumeric) {
                        continue;
                    }
                    let emo_ref = emotion.as_deref();
                    let (emo_speed, emo_gain) = emo_ref.map(crate::sfx::emotion_speed_gain).unwrap_or((1.0, 1.0));
                    let (spk_speed, spk_color) = speaker.as_deref().map(crate::sfx::speaker_style).unwrap_or((1.0, ""));
                    let led_color = emo_ref
                        .and_then(crate::sfx::emotion_led_color)
                        .or_else(|| (!spk_color.is_empty()).then_some(spk_color));
                    let sentences = split_sentences(&cleaned);
                    for s in sentences {
                        if self.closed.load(Ordering::SeqCst) || self.generation.load(Ordering::SeqCst) != generation {
                            return Ok(Vec::new());
                        }
                        let mut samples = self.synthesize_one(context, &s, emo_ref).await?;
                        let speed = (context.profile.tts_speed * emo_speed * spk_speed).clamp(0.75, 1.35);
                        if (speed - 1.0).abs() > 0.02 {
                            samples = time_stretch_pcm(&samples, speed);
                        }
                        let mul = if whisper || emo_ref == Some("whisper") { 0.35_f32 } else { emo_gain };
                        if (mul - 1.0).abs() > 0.02 {
                            for v in &mut samples {
                                *v = ((*v as f32) * mul).clamp(-32767.0, 32767.0) as i16;
                            }
                        }
                        ops.push(StreamAudioOp::Pcm { samples, led_color });
                    }
                }
            }
        }
        Ok(ops)
    }

    async fn play_stream_ops(
        self: &Arc<Self>,
        context: &Arc<Context>,
        ops: Vec<StreamAudioOp>,
        generation: u64,
    ) -> Result<bool, String> {
        let mut played_pcm = false;
        for op in ops {
            if self.closed.load(Ordering::SeqCst) || self.generation.load(Ordering::SeqCst) != generation {
                return Ok(played_pcm);
            }
            match op {
                StreamAudioOp::Scene(scene) => {
                    if let Some(amb) = &context.tools.ambient {
                        let mapped = match scene.as_str() {
                            "tavern" | "fireplace" | "таверна" => Some("fireplace"),
                            "storm" | "rain" | "forest" | "шторм" | "лес" => Some("rain"),
                            "ocean" | "sea" | "океан" => Some("ocean"),
                            "noir" | "jazz" | "нуар" => Some("jazz"),
                            "chill" => Some("chill"),
                            _ => None,
                        };
                        if let Some(id) = mapped {
                            let _ = amb.start(id);
                        }
                    }
                }
                StreamAudioOp::Pcm { samples, led_color } => {
                    if let Some(c) = led_color && let Some(leds) = &context.tools.leds {
                        let bright = context.settings.led.brightness.max(35);
                        leds.show(crate::led::Mode::Speaking, c, bright, Duration::from_secs(4));
                    }
                    self.send_pcm_samples(&samples, generation).await?;
                    played_pcm = true;
                }
            }
        }
        Ok(played_pcm)
    }

    /// Потоковая озвучка по мере прихода предложений от LLM с конвейерным предсинтезом (до 2 фраз в параллель):
    /// первая фраза улетает в TTS максимально рано, а пока она синтезируется и звучит — следующая уже готова.
    async fn speak_stream_deltas(
        self: &Arc<Self>,
        context: &Arc<Context>,
        mut rx: mpsc::UnboundedReceiver<String>,
        generation: u64,
        whisper: bool,
    ) -> Result<usize, String> {
        let mut buf = String::new();
        let mut full = String::new();
        let mut pending: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        let mut in_flight: std::collections::VecDeque<tokio::task::JoinHandle<Result<Vec<StreamAudioOp>, String>>> =
            std::collections::VecDeque::new();
        let mut rx_done = false;
        let mut spoken = 0usize;
        let mut queued_any = false;
        let mut recorded_timing = false;
        let t_tts0 = std::time::Instant::now();

        let abort_all = |q: &mut std::collections::VecDeque<tokio::task::JoinHandle<Result<Vec<StreamAudioOp>, String>>>| {
            while let Some(h) = q.pop_front() {
                h.abort();
            }
        };

        loop {
            if self.closed.load(Ordering::SeqCst) || self.generation.load(Ordering::SeqCst) != generation {
                abort_all(&mut in_flight);
                return Ok(spoken);
            }
            while in_flight.len() < 2 {
                let Some(chunk) = pending.pop_front() else { break };
                let me = self.clone();
                let ctx = context.clone();
                in_flight.push_back(tokio::spawn(async move {
                    me.synth_stream_chunk(&ctx, &chunk, generation, whisper).await
                }));
            }
            if rx_done && in_flight.is_empty() && pending.is_empty() {
                break;
            }
            if !in_flight.is_empty() && !rx_done {
                tokio::select! {
                    biased;
                    maybe_delta = rx.recv() => {
                        match maybe_delta {
                            Some(delta) => {
                                buf.push_str(&delta);
                                full.push_str(&delta);
                                while let Some(chunk) = pop_ready_sentence(&mut buf, !queued_any) {
                                    let shown = strip_web_citations(&crate::sfx::strip_tags(&full));
                                    if !shown.is_empty() {
                                        self.dialog.set_bot(&shown);
                                    }
                                    if has_speakable_chunk(&chunk) {
                                        queued_any = true;
                                        pending.push_back(chunk);
                                    }
                                }
                            }
                            None => {
                                rx_done = true;
                                let shown = strip_web_citations(&crate::sfx::strip_tags(&full));
                                if !shown.is_empty() {
                                    self.dialog.set_bot(&shown);
                                }
                                let rest = buf.trim().to_string();
                                buf.clear();
                                if !rest.is_empty() && has_speakable_chunk(&rest) {
                                    queued_any = true;
                                    pending.push_back(rest);
                                }
                            }
                        }
                    }
                    res = in_flight.front_mut().unwrap() => {
                        let _ = in_flight.pop_front();
                        let ops = match res {
                            Ok(Ok(ops)) => ops,
                            Ok(Err(e)) => {
                                abort_all(&mut in_flight);
                                return Err(e);
                            }
                            Err(_) => {
                                abort_all(&mut in_flight);
                                return Ok(spoken);
                            }
                        };
                        if !ops.is_empty() {
                            if !recorded_timing {
                                recorded_timing = true;
                                self.dialog.set_timings(None, None, Some(t_tts0.elapsed().as_millis() as u64));
                            }
                            if self.play_stream_ops(context, ops, generation).await? {
                                spoken += 1;
                                let pause = vec![0i16; 2400]; // 100 мс пауза между фразами
                                let _ = self.send_pcm_samples(&pause, generation).await;
                            }
                        }
                    }
                }
            } else if !rx_done {
                match rx.recv().await {
                    Some(delta) => {
                        buf.push_str(&delta);
                        full.push_str(&delta);
                        while let Some(chunk) = pop_ready_sentence(&mut buf, !queued_any) {
                            let shown = strip_web_citations(&crate::sfx::strip_tags(&full));
                            if !shown.is_empty() {
                                self.dialog.set_bot(&shown);
                            }
                            if has_speakable_chunk(&chunk) {
                                queued_any = true;
                                pending.push_back(chunk);
                            }
                        }
                    }
                    None => {
                        rx_done = true;
                        let shown = strip_web_citations(&crate::sfx::strip_tags(&full));
                        if !shown.is_empty() {
                            self.dialog.set_bot(&shown);
                        }
                        let rest = buf.trim().to_string();
                        buf.clear();
                        if !rest.is_empty() && has_speakable_chunk(&rest) {
                            queued_any = true;
                            pending.push_back(rest);
                        }
                    }
                }
            } else if let Some(h) = in_flight.pop_front() {
                let ops = match h.await {
                    Ok(Ok(ops)) => ops,
                    Ok(Err(e)) => {
                        abort_all(&mut in_flight);
                        return Err(e);
                    }
                    Err(_) => {
                        abort_all(&mut in_flight);
                        return Ok(spoken);
                    }
                };
                if !ops.is_empty() {
                    if !recorded_timing {
                        recorded_timing = true;
                        self.dialog.set_timings(None, None, Some(t_tts0.elapsed().as_millis() as u64));
                    }
                    if self.play_stream_ops(context, ops, generation).await? {
                        spoken += 1;
                        if !in_flight.is_empty() || !pending.is_empty() {
                            let pause = vec![0i16; 2400]; // 100 мс пауза между фразами
                            let _ = self.send_pcm_samples(&pause, generation).await;
                        }
                    }
                }
            }
        }
        if spoken > 0 {
            let tail = vec![0i16; 5760]; // 240 мс хвост тишины для ALSA
            let _ = self.send_pcm_samples(&tail, generation).await;
        }
        Ok(spoken)
    }

    async fn speak(self: &Arc<Self>,context:&Arc<Context>,text:&str,generation:u64,whisper:bool)->Result<(),String>{
        self.speak_inner(context, text, generation, whisper, true).await
    }

    async fn speak_inner(self: &Arc<Self>,context:&Arc<Context>,text:&str,generation:u64,whisper:bool,add_tail:bool)->Result<(),String>{
        let segments = crate::sfx::parse_segments(text);
        if segments.is_empty() { return Ok(()); }
        let t_tts0 = std::time::Instant::now();
        let mut recorded_timing = false;

        for seg in segments {
            if self.closed.load(Ordering::SeqCst) || self.generation.load(Ordering::SeqCst) != generation {
                return Ok(());
            }
            match seg {
                crate::sfx::Segment::Scene(scene) => {
                    if let Some(amb) = &context.tools.ambient {
                        let mapped = match scene.as_str() {
                            "tavern" | "fireplace" | "таверна" => Some("fireplace"),
                            "storm" | "rain" | "forest" | "шторм" | "лес" => Some("rain"),
                            "ocean" | "sea" | "океан" => Some("ocean"),
                            "noir" | "jazz" | "нуар" => Some("jazz"),
                            "chill" => Some("chill"),
                            _ => None,
                        };
                        if let Some(id) = mapped {
                            let _ = amb.start(id);
                        }
                    }
                }
                crate::sfx::Segment::Sfx(name) => {
                    if let Some(pcm) = crate::sfx::generate_sfx(&name) {
                        self.send_pcm_samples(&pcm, generation).await?;
                    }
                }
                crate::sfx::Segment::Speech { text: seg_text, emotion, speaker } => {
                    let cleaned = clean_for_tts(&seg_text);
                    if !cleaned.chars().any(char::is_alphanumeric) { continue; }
                    let sentences = split_sentences(&cleaned);
                    if sentences.is_empty() { continue; }

                    let emo_ref = emotion.as_deref();
                    let (emo_speed, emo_gain) = emo_ref.map(crate::sfx::emotion_speed_gain).unwrap_or((1.0, 1.0));
                    let (spk_speed, spk_color) = speaker.as_deref().map(crate::sfx::speaker_style).unwrap_or((1.0, ""));
                    if let Some(leds) = &context.tools.leds {
                        if let Some(c) = emo_ref.and_then(crate::sfx::emotion_led_color).or_else(|| (!spk_color.is_empty()).then_some(spk_color)) {
                            let bright = context.settings.led.brightness.max(35);
                            leds.show(crate::led::Mode::Speaking, c, bright, Duration::from_secs(4));
                        }
                    }

                    let emo_owned = emotion.clone();
                    let mut next_fut: Option<tokio::task::JoinHandle<Result<Vec<i16>, String>>> = {
                        let me = self.clone();
                        let ctx = context.clone();
                        let s0 = sentences[0].clone();
                        let em = emo_owned.clone();
                        Some(tokio::spawn(async move { me.synthesize_one(&ctx, &s0, em.as_deref()).await }))
                    };
                    for i in 0..sentences.len() {
                        if self.closed.load(Ordering::SeqCst) || self.generation.load(Ordering::SeqCst) != generation {
                            if let Some(h) = next_fut.take() { h.abort(); }
                            return Ok(());
                        }
                        let current_fut = next_fut.take().unwrap();
                        let mut samples = match current_fut.await {
                            Ok(Ok(s)) => s,
                            Ok(Err(e)) => return Err(e),
                            Err(_) => return Ok(()),
                        };
                        let speed = (context.profile.tts_speed * emo_speed * spk_speed).clamp(0.75, 1.35);
                        if (speed - 1.0).abs() > 0.02 {
                            samples = time_stretch_pcm(&samples, speed);
                        }
                        let mul = if whisper || emo_ref == Some("whisper") {
                            0.35_f32
                        } else {
                            emo_gain
                        };
                        if (mul - 1.0).abs() > 0.02 {
                            for s in &mut samples {
                                *s = ((*s as f32) * mul).clamp(-32767.0, 32767.0) as i16;
                            }
                        }
                        if !recorded_timing {
                            recorded_timing = true;
                            self.dialog.set_timings(None, None, Some(t_tts0.elapsed().as_millis() as u64));
                        }
                        if i + 1 < sentences.len() {
                            let me = self.clone();
                            let ctx = context.clone();
                            let s_next = sentences[i + 1].clone();
                            let em = emo_owned.clone();
                            next_fut = Some(tokio::spawn(async move { me.synthesize_one(&ctx, &s_next, em.as_deref()).await }));
                        }
                        if let Err(e) = self.send_pcm_samples(&samples, generation).await {
                            if let Some(h) = next_fut.take() { h.abort(); }
                            return Err(e);
                        }
                        if i + 1 < sentences.len() {
                            // Небольшая естественная пауза между предложениями (120 мс при 24 кГц)
                            let pause = vec![0i16; 2880];
                            let _ = self.send_pcm_samples(&pause, generation).await;
                        }
                    }
                }
            }
        }
        if add_tail {
            // Мягкий хвост тишины (240 мс при 24 кГц), чтобы ALSA не съедала последний слог
            let tail = vec![0i16; 5760];
            let _ = self.send_pcm_samples(&tail, generation).await;
        }
        Ok(())
    }
}
impl Session for Classic {
    fn handles_commands(&self)->bool{true}
    fn feed(&self,pcm:&[i16]){if !self.closed.load(Ordering::SeqCst){let _=self.frames.try_send(pcm.to_vec());}}
    fn take_audio(&self)->Option<Receiver<Vec<i16>>>{self.audio.lock().unwrap().take()}
    fn rate(&self)->usize{24000}
    fn ready(&self,_:Duration)->Result<(),String>{Ok(())}
    fn say(&self,text:&str)->Result<(),String>{self.say.try_send(text.to_string()).map_err(|e|e.to_string())}
    fn close(&self){self.closed.store(true,Ordering::SeqCst);self.stop.notify_one();}
    fn dialog(&self)->&Dialog{&self.dialog}
    fn interrupted(&self)->bool{self.interrupted.swap(false,Ordering::SeqCst)}
    fn last_failure(&self)->Option<crate::voice_error::VoiceFailure>{self.last_failure.lock().unwrap().clone()}
}
struct Capture{preroll:Vec<i16>,speech:Vec<i16>,silence:usize,speaking:bool,threshold:u16,silence_limit:usize}
impl Capture {
    fn new(threshold: i64, silence_ms: i64) -> Self {
        let th = threshold.clamp(100, 5000) as u16;
        let lim = (silence_ms.clamp(250, 3000) as usize) * 16; // 16 samples per ms at 16 kHz
        Self { preroll: Vec::new(), speech: Vec::new(), silence: 0, speaking: false, threshold: th, silence_limit: lim }
    }
    fn is_loud(&self, pcm: &[i16]) -> bool {
        pcm.iter().any(|&v| v.unsigned_abs() > self.threshold)
    }
    fn feed(&mut self,pcm:&[i16])->Option<Vec<i16>>{
        let voiced=self.is_loud(pcm);
        if !self.speaking && !voiced {self.preroll.extend(pcm);if self.preroll.len()>8000{self.preroll.drain(..self.preroll.len()-8000);}return None;}
        if !self.speaking {self.speaking=true;self.speech=std::mem::take(&mut self.preroll);}
        self.speech.extend(pcm);if voiced{self.silence=0;}else{self.silence+=pcm.len();}
        if self.silence>=self.silence_limit || self.speech.len()>=16000*45 {self.speaking=false;self.silence=0;let result=std::mem::take(&mut self.speech);if result.len()>=3200{return Some(result);}}
        None
    }
}
/// Звуковой отклик «услышал, думаю» по выбранной теме (24 кГц PCM16).
pub fn thinking_chime_for(theme: &str) -> Vec<i16> {
    let rate = 24000.0_f32;
    match theme {
        "scifi" => {
            // Футуристичный двухнотный гармонический аккорд в духе Джарвиса (~160 мс)
            let total = (rate * 0.16) as usize;
            let mut out = Vec::with_capacity(total);
            for i in 0..total {
                let t = i as f32 / rate;
                let p = i as f32 / total as f32;
                let env = (std::f32::consts::PI * p).sin().powf(0.8);
                let f1 = if p < 0.45 { 587.33 } else { 880.0 };
                let f2 = if p < 0.45 { 880.0 } else { 1174.66 };
                let s = ((2.0 * std::f32::consts::PI * f1 * t).sin() * 0.65
                    + (2.0 * std::f32::consts::PI * f2 * t).sin() * 0.35)
                    * env
                    * 2600.0;
                out.push(s as i16);
            }
            out
        }
        "retro" => {
            // Короткий 8-битный чиптюн-блип (~100 мс)
            let total = (rate * 0.10) as usize;
            let mut out = Vec::with_capacity(total);
            for i in 0..total {
                let t = i as f32 / rate;
                let p = i as f32 / total as f32;
                let freq = if p < 0.5 { 784.0 } else { 1046.5 };
                let sq = if (2.0 * std::f32::consts::PI * freq * t).sin() >= 0.0 { 1.0 } else { -1.0 };
                let env = (1.0 - p).max(0.0);
                out.push((sq * env * 1400.0) as i16);
            }
            out
        }
        "minimal" => {
            // Едва слышный мягкий акустический клик (~45 мс)
            let total = (rate * 0.045) as usize;
            let mut out = Vec::with_capacity(total);
            for i in 0..total {
                let t = i as f32 / rate;
                let p = i as f32 / total as f32;
                let env = (std::f32::consts::PI * p).sin() * (1.0 - p);
                let s = (2.0 * std::f32::consts::PI * 950.0 * t).sin() * env * 1800.0;
                out.push(s as i16);
            }
            out
        }
        _ => {
            // "soft" — тёплый плавный тон (~110 мс)
            let total = (rate * 0.11) as usize;
            let mut out = Vec::with_capacity(total);
            for i in 0..total {
                let t = i as f32 / rate;
                let env = (std::f32::consts::PI * i as f32 / total as f32).sin();
                let freq = 620.0 + 140.0 * (i as f32 / total as f32);
                let s = (2.0 * std::f32::consts::PI * freq * t).sin() * env * 2400.0;
                out.push(s as i16);
            }
            out
        }
    }
}

/// Изменение темпа речи без искажения высоты голоса (Overlap-Add с окном Хэннинга, 24 кГц).
fn time_stretch_pcm(samples: &[i16], speed: f64) -> Vec<i16> {
    let win = 600usize; // 25 мс при 24 кГц
    if samples.len() < win * 2 {
        return samples.to_vec();
    }
    let hop_out = win / 2;
    let hop_in = ((hop_out as f64) * speed).round().max(1.0) as usize;
    let frames = (samples.len().saturating_sub(win)) / hop_in + 1;
    let out_len = (frames.saturating_sub(1)) * hop_out + win;
    let mut acc = vec![0.0_f32; out_len];
    let mut norm = vec![0.0_f32; out_len];
    let window: Vec<f32> = (0..win)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / win as f32).cos())
        .collect();

    for f in 0..frames {
        let in_pos = f * hop_in;
        let out_pos = f * hop_out;
        if in_pos + win > samples.len() || out_pos + win > out_len {
            break;
        }
        for i in 0..win {
            let w = window[i];
            acc[out_pos + i] += samples[in_pos + i] as f32 * w;
            norm[out_pos + i] += w;
        }
    }
    acc.iter()
        .zip(norm.iter())
        .map(|(&v, &n)| {
            let s = if n > 1e-4 { v / n } else { v };
            s.clamp(-32767.0, 32767.0) as i16
        })
        .collect()
}
/// Проверяет, похожа ли строка на домен или URL (например `ghibli.jp`, `ru.wikipedia.org`, `https://...`).
fn looks_like_domain_or_url(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.chars().any(char::is_whitespace) {
        return false;
    }
    let low = t.to_ascii_lowercase();
    if low.starts_with("http://") || low.starts_with("https://") || low.starts_with("www.") {
        return true;
    }
    let host = t.split(['/', '?', '#']).next().unwrap_or(t);
    if !host.contains('.') {
        return false;
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return false;
    }
    let tld = parts.last().copied().unwrap_or("");
    let tld_low = tld.to_lowercase();
    let valid_cyrillic_tld = matches!(tld_low.as_str(), "рф" | "бел" | "укр" | "сайт" | "онлайн");
    if host.is_ascii() {
        let valid_tld = (2..=10).contains(&tld.len()) && tld.bytes().all(|b| b.is_ascii_alphabetic());
        let valid_labels = parts.iter().all(|p| {
            p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        });
        let has_lower = host.bytes().any(|b| b.is_ascii_lowercase());
        valid_tld && valid_labels && has_lower
    } else {
        valid_cyrillic_tld
            && parts.iter().all(|p| p.chars().all(|c| c.is_alphanumeric() || c == '-'))
    }
}

/// Проверяет, состоит ли содержимое `(...)` исключительно из ссылок-источников (например `([ghibli.jp](https://...))`).
fn is_citation_group(inner: &str) -> bool {
    let mut found_link = false;
    let mut i = 0;
    let bytes = inner.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(close_b) = inner[i + 1..].find(']') {
                let mid = i + 1 + close_b;
                if bytes.get(mid + 1) == Some(&b'(') {
                    if let Some(close_p) = inner[mid + 2..].find(')') {
                        let url = inner[mid + 2..mid + 2 + close_p].trim();
                        if url.starts_with("http://") || url.starts_with("https://") {
                            found_link = true;
                            i = mid + 2 + close_p + 1;
                            continue;
                        }
                    }
                }
            }
        }
        if inner[i..].starts_with("http://") || inner[i..].starts_with("https://") {
            found_link = true;
            while i < bytes.len() && !inner[i..].chars().next().unwrap().is_whitespace() && bytes[i] != b',' && bytes[i] != b';' {
                i += inner[i..].chars().next().unwrap().len_utf8();
            }
            continue;
        }
        let ch = inner[i..].chars().next().unwrap();
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | ',' | ';' | '/' | '|' | '•' | '·' | '-' | '–' | '—') {
            i += ch.len_utf8();
            continue;
        }
        return false;
    }
    found_link
}

/// Удаляет веб-цитаты OpenAI вроде `([ghibli.jp](https://...))` и маркеры `【4:0†source】`,
/// чтобы они не портили чат и не зачитывались вслух.
fn strip_web_citations(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Маркеры цитирования OpenAI вида 【4:0†source】
        if input[i..].starts_with('【') {
            if let Some(end_off) = input[i..].find('】') {
                let after = i + end_off + '】'.len_utf8();
                let next_ch = input[after..].chars().next();
                if out.ends_with(' ') && next_ch.is_none_or(|c| c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '…' | ')' | '»')) {
                    out.pop();
                }
                i = after;
                continue;
            }
        }
        // Группы ссылок-источников в скобках: ([ghibli.jp](https://...)) или ([a.ru](https://...), [b.ru](https://...))
        if bytes[i] == b'(' {
            let mut depth = 0usize;
            let mut j = i;
            let mut matched_close: Option<usize> = None;
            while j < bytes.len() {
                if bytes[j] == b'(' {
                    depth += 1;
                    j += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                    if depth == 0 {
                        matched_close = Some(j);
                        break;
                    }
                    j += 1;
                } else {
                    j += input[j..].chars().next().unwrap().len_utf8();
                }
            }
            if let Some(close_idx) = matched_close {
                let inner = &input[i + 1..close_idx];
                if is_citation_group(inner) {
                    let after = close_idx + 1;
                    let next_ch = input[after..].chars().next();
                    if out.ends_with(' ') && next_ch.is_none_or(|c| c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '…' | ')' | '»')) {
                        out.pop();
                    }
                    i = after;
                    continue;
                }
            }
        }
        // Markdown-ссылки [label](http...) без внешних скобок
        if bytes[i] == b'[' {
            if let Some(close_b) = input[i + 1..].find(']') {
                let mid = i + 1 + close_b;
                if bytes.get(mid + 1) == Some(&b'(') {
                    if let Some(close_p) = input[mid + 2..].find(')') {
                        let label = &input[i + 1..mid];
                        let url = input[mid + 2..mid + 2 + close_p].trim();
                        if url.starts_with("http://") || url.starts_with("https://") {
                            let after = mid + 2 + close_p + 1;
                            if looks_like_domain_or_url(label) {
                                let next_ch = input[after..].chars().next();
                                if out.ends_with(' ') && next_ch.is_none_or(|c| c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '…' | ')' | '»')) {
                                    out.pop();
                                }
                            } else {
                                // Обычную текстовую ссылку оставляем целиком, чтобы (url) не съелось отдельно
                                out.push_str(&input[i..after]);
                            }
                            i = after;
                            continue;
                        }
                    }
                }
            }
        }
        let ch = input[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out.trim().to_string()
}

/// Есть ли в фрагменте реальный произносимый текст или SFX/сцена (а не только цитата/пунктуация).
fn has_speakable_chunk(chunk: &str) -> bool {
    for seg in crate::sfx::parse_segments(chunk) {
        match seg {
            crate::sfx::Segment::Scene(_) | crate::sfx::Segment::Sfx(_) => return true,
            crate::sfx::Segment::Speech { text, .. } => {
                if clean_for_tts(&text).chars().any(char::is_alphanumeric) {
                    return true;
                }
            }
        }
    }
    false
}

/// Очистка ответа от Markdown, ссылок, цитат, кода и эмодзи перед отправкой в TTS.
fn clean_for_tts(input: &str) -> String {
    // 0. Убираем веб-цитаты OpenAI ([ghibli.jp](https://...)) и маркеры 【...】
    let cited = strip_web_citations(input);

    // 1. Убираем fenced code blocks ```...```
    let mut no_code = String::with_capacity(cited.len());
    let mut rest = cited.as_str();
    while let Some(start) = rest.find("```") {
        no_code.push_str(&rest[..start]);
        let after = &rest[start + 3..];
        if let Some(end) = after.find("```") {
            rest = &after[end + 3..];
        } else {
            rest = "";
            break;
        }
    }
    no_code.push_str(rest);

    // 2. Превращаем markdown-ссылки [текст](url) -> текст (а доменные ссылки вроде [ghibli.jp](...) удаляем)
    let mut no_links = String::with_capacity(no_code.len());
    let bytes = no_code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(close_b) = no_code[i + 1..].find(']') {
                let mid = i + 1 + close_b;
                if no_code.as_bytes().get(mid + 1) == Some(&b'(') {
                    if let Some(close_p) = no_code[mid + 2..].find(')') {
                        let label = no_code[i + 1..mid].trim();
                        if !label.is_empty() && !looks_like_domain_or_url(label) {
                            no_links.push_str(label);
                        }
                        i = mid + 2 + close_p + 1;
                        continue;
                    }
                }
            }
        }
        let ch = no_code[i..].chars().next().unwrap();
        no_links.push(ch);
        i += ch.len_utf8();
    }

    // 3. Убираем голые URL/домены и markdown-разметку по строкам
    let mut lines_out = Vec::new();
    for raw_line in no_links.lines() {
        let mut line = raw_line.trim();
        // Убираем заголовки ### и маркеры списков (- , * , • )
        line = line.trim_start_matches('#').trim_start();
        if let Some(stripped) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).or_else(|| line.strip_prefix("• ")) {
            line = stripped.trim_start();
        }
        let mut words = Vec::new();
        for w in line.split_whitespace() {
            let wl = w.trim_matches(|c: char| matches!(c, '(' | ')' | '<' | '>' | '[' | ']' | '"' | '\'' | ',' | '.' | ';' | ':'));
            if wl.is_empty() && w.chars().all(|c| matches!(c, '(' | ')' | '[' | ']')) {
                continue;
            }
            if looks_like_domain_or_url(wl) {
                continue;
            }
            words.push(w);
        }
        let joined = words.join(" ");
        if !joined.is_empty() {
            lines_out.push(joined);
        }
    }
    let joined = lines_out.join("\n").replace("()", "");

    // 4. Убираем символы форматирования и эмодзи/пиктограммы
    let mut out = String::with_capacity(joined.len());
    let mut prev_space = false;
    for ch in joined.chars() {
        if matches!(ch, '*' | '_' | '`' | '~' | '|') {
            continue;
        }
        let cp = ch as u32;
        let is_emoji = (0x1F300..=0x1FAFF).contains(&cp)
            || (0x2600..=0x27BF).contains(&cp)
            || (0xFE00..=0xFE0F).contains(&cp)
            || cp == 0x200D
            || cp == 0x20E3;
        if is_emoji {
            continue;
        }
        if ch == ' ' || ch == '\t' {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    let trimmed = out.trim().to_string();
    if !trimmed.chars().any(char::is_alphanumeric) {
        String::new()
    } else {
        trimmed
    }
}

fn tts_cache_key(provider: &str, voice: &str, model: &str, emotion: &str, text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(provider.as_bytes());
    h.update(b"|");
    h.update(voice.as_bytes());
    h.update(b"|");
    h.update(model.as_bytes());
    h.update(b"|");
    h.update(emotion.as_bytes());
    h.update(b"|");
    h.update(text.trim().to_lowercase().as_bytes());
    hex::encode(&h.finalize()[..12])
}

fn tts_cache_dir() -> std::path::PathBuf {
    std::path::PathBuf::from("/tmp/jarvisd/tts_cache")
}

fn tts_cache_get(key: &str) -> Option<Vec<i16>> {
    let bytes = std::fs::read(tts_cache_dir().join(format!("{key}.pcm"))).ok()?;
    if bytes.is_empty() || bytes.len() % 2 != 0 {
        return None;
    }
    Some(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect())
}

fn tts_cache_put(key: &str, samples: &[i16]) {
    if samples.is_empty() || samples.len() > 24000 * 6 {
        return;
    }
    let dir = tts_cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(rd) = std::fs::read_dir(&dir) && rd.count() >= 48 {
        return;
    }
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    let _ = std::fs::write(dir.join(format!("{key}.pcm")), bytes);
}

enum StreamAudioOp {
    Scene(String),
    Pcm { samples: Vec<i16>, led_color: Option<&'static str> },
}

/// Вырезает из потокового буфера первое законченное предложение или (для самого первого фрагмента)
/// первую естественную фразу, чтобы TTS начал говорить максимально быстро.
fn pop_ready_sentence(buf: &mut String, is_first: bool) -> Option<String> {
    let min_sentence = if is_first { 12 } else { 25 };
    let min_clause = if is_first { 45 } else { 180 };
    let mut chars = buf.char_indices().peekable();
    let mut char_count = 0usize;
    let mut cut_at: Option<usize> = None;
    while let Some((idx, c)) = chars.next() {
        char_count += 1;
        let next_ws = chars.peek().is_some_and(|&(_, n)| n.is_whitespace());
        if matches!(c, '.' | '!' | '?' | '…' | '\n') && next_ws && char_count >= min_sentence {
            let end = idx + c.len_utf8();
            let prefix = buf[..end].trim();
            let is_abbrev = c == '.' && (
                prefix.ends_with(" т.д.")
                    || prefix.ends_with(" т.п.")
                    || prefix.ends_with(" др.")
                    || prefix.ends_with(" ул.")
                    || prefix.ends_with(" г.")
            );
            // Не обрываем незакрытый тег [emotion]/[sfx:...] и незакрытую скобку (цитату/ссылку)
            let open_br = prefix.matches('[').count();
            let close_br = prefix.matches(']').count();
            let open_par = prefix.matches('(').count();
            let close_par = prefix.matches(')').count();
            if !is_abbrev && open_br == close_br && open_par == close_par {
                cut_at = Some(end);
                break;
            }
        }
        if char_count >= min_clause && matches!(c, ',' | ';' | '—' | ':') && next_ws {
            let end = idx + c.len_utf8();
            let prefix = buf[..end].trim();
            if prefix.matches('[').count() == prefix.matches(']').count()
                && prefix.matches('(').count() == prefix.matches(')').count()
            {
                cut_at = Some(end);
                break;
            }
        }
    }
    let end = cut_at?;
    let head = buf[..end].trim().to_string();
    let rest = buf[end..].trim_start().to_string();
    *buf = rest;
    if head.is_empty() { None } else { Some(head) }
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut raw = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        let is_end = matches!(c, '.' | '!' | '?' | '…' | '\n');
        let next_sep = chars.peek().is_none_or(|n| n.is_whitespace());
        if is_end && next_sep {
            let trimmed = cur.trim();
            // Не разрываем на частых сокращениях вроде «т.д.», «г.», «ул.»
            let is_abbrev = c == '.' && (
                trimmed.ends_with(" т.д.")
                    || trimmed.ends_with(" т.п.")
                    || trimmed.ends_with(" др.")
                    || trimmed.ends_with(" ул.")
                    || trimmed.ends_with(" г.")
            );
            if !is_abbrev && !trimmed.is_empty() {
                raw.push(trimmed.to_string());
                cur.clear();
            }
        }
    }
    let tail = cur.trim().to_string();
    if !tail.is_empty() {
        raw.push(tail);
    }
    // Аккуратно делим сверхдлинные предложения (> 260 симв.) по запятой/точке с запятой, чтобы TTS не задыхался
    let mut sized: Vec<String> = Vec::new();
    for s in raw {
        if s.chars().count() <= 260 {
            sized.push(s);
            continue;
        }
        let mut rest = s.as_str();
        while rest.chars().count() > 260 {
            let cut_byte = rest
                .char_indices()
                .take(260)
                .filter(|&(_, ch)| matches!(ch, ';' | ',' | '—' | ':'))
                .map(|(idx, ch)| idx + ch.len_utf8())
                .last()
                .filter(|&b| b > 80)
                .or_else(|| {
                    rest.char_indices()
                        .take(260)
                        .filter(|&(_, ch)| ch.is_whitespace())
                        .map(|(idx, _)| idx)
                        .last()
                })
                .unwrap_or_else(|| rest.char_indices().nth(260).map(|(i, _)| i).unwrap_or(rest.len()));
            let part = rest[..cut_byte].trim();
            if !part.is_empty() {
                sized.push(part.to_string());
            }
            rest = rest[cut_byte..].trim();
        }
        if !rest.is_empty() {
            sized.push(rest.to_string());
        }
    }
    // Merge very short fragments (< 35 chars) both forward and backward so short tails like «Спокойной ночи.» aren't isolated
    let mut out: Vec<String> = Vec::new();
    for s in sized {
        if let Some(last) = out.last_mut() {
            let last_len = last.chars().count();
            let s_len = s.chars().count();
            if last_len < 35 || (s_len < 35 && last_len + 1 + s_len <= 240) {
                last.push(' ');
                last.push_str(&s);
                continue;
            }
        }
        out.push(s);
    }
    out
}
fn resample_pcm(samples:&[i16],from_rate:usize,to_rate:usize)->Vec<i16>{
    if from_rate==to_rate || samples.is_empty(){return samples.to_vec();}
    let out_len=(samples.len() as u64 * to_rate as u64 / from_rate as u64) as usize;
    (0..out_len).map(|i|{
        let pos=i as u64 * from_rate as u64;
        let left=(pos / to_rate as u64) as usize;
        let frac=(pos % to_rate as u64) as f32 / to_rate as f32;
        let a=f32::from(samples[left]);
        let b=f32::from(*samples.get(left+1).unwrap_or(&samples[left]));
        (a+(b-a)*frac).round().clamp(-32768.0,32767.0) as i16
    }).collect()
}
fn wav(pcm:&[i16])->Vec<u8>{
    let size=(pcm.len()*2) as u32;let mut out=Vec::with_capacity(size as usize+44);out.extend(b"RIFF");out.extend((size+36).to_le_bytes());out.extend(b"WAVEfmt ");out.extend(16u32.to_le_bytes());out.extend(1u16.to_le_bytes());out.extend(1u16.to_le_bytes());out.extend(16000u32.to_le_bytes());out.extend(32000u32.to_le_bytes());out.extend(2u16.to_le_bytes());out.extend(16u16.to_le_bytes());out.extend(b"data");out.extend(size.to_le_bytes());out.extend(pcm.iter().flat_map(|p|p.to_le_bytes()));out
}
#[cfg(test)]mod tests{use super::*;#[test]fn speech_capture_is_bounded_and_wav_has_real_lengths(){let mut capture=Capture::new(600,700);for _ in 0..30{assert!(capture.feed(&[0;1600]).is_none());}assert_eq!(capture.preroll.len(),4800);assert!(capture.feed(&[2000;3200]).is_none());let data=capture.feed(&[0;11200]).unwrap();assert!(!capture.speaking);let encoded=wav(&data);assert_eq!(&encoded[..4],b"RIFF");assert_eq!(encoded.len(),44+data.len()*2);}
    #[test]fn resamples_16k_24k_and_48k_to_24k(){
        let s16:Vec<i16>=(0..1600).map(|i|(i*10) as i16).collect();
        let r16=resample_pcm(&s16,16000,24000);assert_eq!(r16.len(),2400);assert_eq!(r16[0],0);
        let s24=vec![123i16;2400];assert_eq!(resample_pcm(&s24,24000,24000),s24);
        let s48:Vec<i16>=(0..4800).map(|i|(i*5) as i16).collect();
        let r48=resample_pcm(&s48,48000,24000);assert_eq!(r48.len(),2400);assert_eq!(r48[10],100);
    }
    #[test]
    fn strips_web_citations_and_prevents_stream_gibberish() {
        let raw = "Может, «Ведьмину службу доставки» Миядзаки? Уютное аниме для тихого вечера. ([ghibli.jp](https://www.ghibli.jp/works/majo/?utm_source=openai))";
        let expected = "Может, «Ведьмину службу доставки» Миядзаки? Уютное аниме для тихого вечера.";
        assert_eq!(strip_web_citations(raw), expected);
        assert_eq!(clean_for_tts(raw), expected);
        assert_eq!(clean_for_tts("([ghibli.jp](https://www.ghibli.jp/works/majo/?utm_source=openai))"), "");
        assert!(!has_speakable_chunk("([ghibli.jp](https://www.ghibli.jp/works/majo/?utm_source=openai))"));
        assert_eq!(
            clean_for_tts("Посмотри [эту статью](https://example.com/a) ([site1.ru](https://site1.ru), [site2.org](https://site2.org))."),
            "Посмотри эту статью."
        );
        // Потоковая нарезка не отдаёт цитату в озвучку и не режет внутри скобок
        let mut buf = String::from(raw);
        let mut spoken = Vec::new();
        while let Some(chunk) = pop_ready_sentence(&mut buf, spoken.is_empty()) {
            if has_speakable_chunk(&chunk) {
                spoken.push(clean_for_tts(&chunk));
            }
        }
        if !buf.trim().is_empty() && has_speakable_chunk(buf.trim()) {
            spoken.push(clean_for_tts(buf.trim()));
        }
        assert_eq!(spoken, vec![
            "Может, «Ведьмину службу доставки» Миядзаки?",
            "Уютное аниме для тихого вечера.",
        ]);
        // Первый длинный список режется рано на двоеточии, чтобы TTS начал говорить сразу
        let mut list_buf = String::from("Хай… вот несколько отличных вариантов на выбор: первый фильм, второй фильм и третий фильм.");
        let first_part = pop_ready_sentence(&mut list_buf, true).unwrap();
        assert_eq!(first_part, "Хай… вот несколько отличных вариантов на выбор:");
        // Dialog::set_bot не дублирует ответ при многократном обновлении
        let journal = Arc::new(Journal::default());
        let dialog = Dialog::new(journal.clone(), "classic", "gpt");
        dialog.on_user("Посоветуй аниме");
        dialog.set_bot("Может, «Ведьмину службу доставки» Миядзаки? Уют");
        dialog.set_bot(expected);
        dialog.set_bot(expected);
        let items = journal.items(5);
        assert_eq!(items.last().unwrap().answer, expected);
    }
    #[cfg(feature="lua")]
    #[tokio::test]
    async fn classic_pipeline_runs_lua_stt_llm_with_history_and_tts(){
        let dir=std::env::temp_dir().join(format!("jarvis-classic-lua-{}",crate::settings::rand_hex(6)));
        for (id,provides,src) in [
            ("my-stt","stt",r#"local n=0;return {transcribe=function(req) n=n+1;assert(req.rate==16000 and req.model=='whisper-x');return {text='вопрос '..n} end}"#),
            ("my-llm","llm",r#"return {chat=function(req) assert(req.model=='custom-m');if #req.messages==2 then return 'ответ 1' end;assert(#req.messages==4 and req.messages[2].content=='вопрос 1' and req.messages[3].content=='ответ 1');return 'ответ 2' end}"#),
            ("my-tts","tts",r#"return {speak=function(req) return {audio='AQABAA==',rate=16000} end}"#),
        ]{
            let pdir=dir.join("plugins").join(id);std::fs::create_dir_all(&pdir).unwrap();
            std::fs::write(pdir.join("plugin.json"),json!({"id":id,"name":id,"version":"1","kind":"lua","category":"assistant","main":"main.lua","provides":[provides]}).to_string()).unwrap();
            std::fs::write(pdir.join("main.lua"),src).unwrap();
        }
        let host=Host::new(dir.clone(),crate::plugins::Hooks::default());host.load().await;
        let store=crate::settings::Store::open(dir.join("settings.json")).unwrap();
        let tools=Arc::new(Tools::new(host.clone(),store));
        let journal=Arc::new(Journal::default());
        let profile=Profile{mode:"classic".into(),engine:"classic".into(),stt_engine:"my-stt".into(),stt_model:"whisper-x".into(),llm_engine:"my-llm".into(),llm_model:"custom-m".into(),tts_engine:"my-tts".into(),tts_voice:"v1".into(),..Default::default()};
        let sess=Classic::open(&tokio::runtime::Handle::current(),host.clone(),tools.clone(),journal.clone(),Settings::default(),profile.clone(),"inst".into(),"Джарвис".into());
        let rx=sess.take_audio().unwrap();
        let ctx=Arc::new(Context{host:host.clone(),tools,settings:Settings::default(),profile,instructions:"inst".into(),phrase:"Джарвис".into(),history:Mutex::new(Vec::new()),gemini_pool:None,proxy:None});
        sess.turn(&ctx,vec![0;3200],0).await.unwrap();
        let pcm1=rx.recv_timeout(Duration::from_secs(2)).unwrap();assert_eq!(pcm1.len(),3);
        sess.turn(&ctx,vec![0;3200],0).await.unwrap();
        let pcm2=rx.recv_timeout(Duration::from_secs(2)).unwrap();assert_eq!(pcm2.len(),3);
        let items=journal.items(10);assert!(items.iter().any(|t|t.answer=="ответ 2"));
        sess.close();host.stop_all().await;std::fs::remove_dir_all(dir).unwrap();
    }
}
