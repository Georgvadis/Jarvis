<script>
  import { onMount, tick } from "svelte";
  import { api, icon, ring, toast, upload } from "#helpers/core.js";
  import { profilePath } from "#helpers/paths.js";
  import { activity, refreshActivity } from "#state/activity.js";
  import { avatar, journal, refreshRoute, reload, S, speakerState } from "#state/panelState.js";
  import { chatMessages } from "./messages.js";
  import Message from "./Message/Message.svelte";

  let { poll } = $props();
  const controller = new AbortController();
  const { signal } = controller;
  let snapshot = $state.raw(readChat());
  let draft = $state("");
  let speak = $state(readVoicePreference());
  let directTts = $state(false);
  let sending = $state(false);
  let chipBusy = $state("");
  let confirmClear = $state(false);
  const speakerActions = $state({ talk: false, stop: false });
  let feed = $state();
  let sayInput = $state();
  let animate = $state(false);
  let scrolledUp = $state(false);
  let searchOpen = $state(false);
  let searchQuery = $state("");
  let searchInput = $state();
  let pttRecording = $state(false);
  let pttBusy = $state(false);
  let pttSec = $state(0);
  let stickToBottom = true;
  let pollTimer;
  let confirmTimer;
  let pttTickTimer;
  let pttMaxTimer;
  let mediaRecorder = null;
  let mediaStream = null;
  let audioChunks = [];
  let pttExt = "webm";

  let rows = $derived(chatMessages(snapshot.turns, snapshot.phase));
  let visibleRows = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter((row) =>
      (row.kind === "message" || row.kind === "answer" || row.kind === "error")
      && (row.text || row.answer || "").toLowerCase().includes(q)
    );
  });
  let status = $derived(snapshot.talking ? "говорит и слушает…"
    : snapshot.state.title.replace(/^Жду/, "ждёт").replace(/^Микрофон/, "микрофон"));
  let typing = $derived(draft.trim() !== "");
  let focusRunning = $derived(Boolean($activity.focus?.running));

  onMount(() => {
    animate = true;
    return () => {
      controller.abort();
      clearTimeout(pollTimer);
      clearTimeout(confirmTimer);
      stopPttCleanup();
    };
  });

  function stopPttCleanup() {
    clearInterval(pttTickTimer);
    clearTimeout(pttMaxTimer);
    try {
      if (mediaRecorder && mediaRecorder.state !== "inactive") mediaRecorder.stop();
    } catch { /* ignore */ }
    if (mediaStream) {
      mediaStream.getTracks().forEach((t) => { try { t.stop(); } catch { /* ignore */ } });
      mediaStream = null;
    }
  }

  function pickAudioMime() {
    if (typeof MediaRecorder === "undefined") return ["", "webm"];
    const candidates = [
      ["audio/webm;codecs=opus", "webm"],
      ["audio/webm", "webm"],
      ["audio/ogg;codecs=opus", "ogg"],
      ["audio/mp4", "mp4"],
    ];
    if (typeof MediaRecorder.isTypeSupported !== "function") return ["", "webm"];
    for (const [mime, ext] of candidates) {
      if (MediaRecorder.isTypeSupported(mime)) return [mime, ext];
    }
    return ["", "webm"];
  }

  async function togglePtt() {
    if (pttBusy) return;
    if (pttRecording) {
      clearInterval(pttTickTimer);
      clearTimeout(pttMaxTimer);
      try {
        mediaRecorder?.stop();
      } catch {
        stopPttCleanup();
        pttRecording = false;
      }
      return;
    }

    if (!window.isSecureContext || !navigator.mediaDevices?.getUserMedia) {
      toast("Микрофон браузера требует HTTPS или localhost (Chrome: flags → insecure-origins)", true);
      return;
    }
    if (typeof MediaRecorder === "undefined") {
      toast("Браузер не поддерживает запись аудио (MediaRecorder)", true);
      return;
    }

    try {
      mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch {
      toast("Нет доступа к микрофону браузера", true);
      return;
    }

    const [mime, ext] = pickAudioMime();
    pttExt = ext;
    audioChunks = [];
    try {
      mediaRecorder = mime ? new MediaRecorder(mediaStream, { mimeType: mime }) : new MediaRecorder(mediaStream);
    } catch {
      stopPttCleanup();
      toast("Не удалось запустить запись с микрофона", true);
      return;
    }

    mediaRecorder.ondataavailable = (ev) => {
      if (ev.data && ev.data.size > 0) audioChunks.push(ev.data);
    };
    mediaRecorder.onstop = () => {
      clearInterval(pttTickTimer);
      clearTimeout(pttMaxTimer);
      const chunks = audioChunks;
      const recordedExt = pttExt;
      audioChunks = [];
      if (mediaStream) {
        mediaStream.getTracks().forEach((t) => { try { t.stop(); } catch { /* ignore */ } });
        mediaStream = null;
      }
      pttRecording = false;
      const blob = new Blob(chunks, { type: mime || "audio/webm" });
      if (blob.size < 256) {
        toast("Запись слишком короткая", true);
        return;
      }
      void sendPttAudio(blob, recordedExt);
    };

    pttSec = 0;
    pttRecording = true;
    mediaRecorder.start(250);
    pttTickTimer = setInterval(() => { pttSec += 1; }, 1000);
    pttMaxTimer = setTimeout(() => {
      if (pttRecording) void togglePtt();
    }, 45000);
  }

  async function sendPttAudio(blob, ext) {
    if (pttBusy || signal.aborted) return;
    pttBusy = true;
    try {
      const qs = `ext=${encodeURIComponent(ext)}&speak=${speak ? "1" : "0"}&direct=${directTts ? "1" : "0"}`;
      const res = await upload("POST", `/api/ptt?${qs}`, blob, { signal });
      if (signal.aborted) return;
      if (res.ok && res.text) {
        toast(`«${res.text}»`);
        stickToBottom = true;
        followFeed();
        void poll();
        clearTimeout(pollTimer);
        pollTimer = setTimeout(poll, 700);
      } else {
        toast(res.error || "Речь не распознана", true);
      }
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      pttBusy = false;
    }
  }

  $effect(() => {
    if (rows.length) followFeed();
  });

  function readChat() {
    return {
      profile: S.profiles.find((profile) => profile.id === S.active) || S.profiles[0],
      profiles: S.profiles || [],
      state: speakerState(),
      wakeWord: S.settings.wake_word || "Джарвис",
      talking: S.speaker.talking,
      phase: S.speaker.phase,
      turns: journal,
    };
  }

  async function switchProfile(event) {
    const id = event.currentTarget.value;
    if (!id || id === snapshot.profile?.id) return;
    try {
      await api("POST", "/api/active", { id }, { signal });
      await reload();
      if (signal.aborted) return;
      const next = S.profiles.find((p) => p.id === id);
      if (next) toast(`Активен: ${next.name}`);
      refreshRoute();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    }
  }

  export function refresh() {
    snapshot = readChat();
  }

  function readVoicePreference() {
    try { return localStorage.getItem("jarvis.speak") !== "0"; }
    catch { return true; }
  }

  function toggleVoice() {
    speak = !speak;
    try { localStorage.setItem("jarvis.speak", speak ? "1" : "0"); }
    catch { /* ignore */ }
  }

  function onScroll(event) {
    const node = event.currentTarget;
    const dist = node.scrollHeight - node.scrollTop - node.clientHeight;
    stickToBottom = dist < 80;
    scrolledUp = dist > 140;
  }

  function jumpToBottom() {
    stickToBottom = true;
    scrolledUp = false;
    if (feed?.isConnected) feed.scrollTo({ top: feed.scrollHeight, behavior: "smooth" });
  }

  function followFeed() {
    if (!searchQuery.trim() && stickToBottom && feed?.isConnected) feed.scrollTop = feed.scrollHeight;
  }

  async function toggleSearch() {
    searchOpen = !searchOpen;
    if (!searchOpen) {
      searchQuery = "";
      return;
    }
    await tick();
    searchInput?.focus();
  }

  async function send(event) {
    event.preventDefault();
    const submitted = draft;
    const text = submitted.trim();
    if (!text || sending) return;
    sending = true;
    try {
      await api("POST", "/api/say", directTts ? { text, direct: true } : { text, speak }, { signal });
      if (draft === submitted) draft = "";
      stickToBottom = true;
      followFeed();
      void poll();
      clearTimeout(pollTimer);
      pollTimer = setTimeout(poll, 700);
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      sending = false;
    }
  }

  async function controlSpeaker(action) {
    if (speakerActions[action]) return;
    speakerActions[action] = true;
    try {
      await api("POST", "/api/" + action, undefined, { signal });
      if (action === "talk") toast("Слушаю…");
      if (action === "stop") void refreshActivity();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      speakerActions[action] = false;
    }
  }

  function askClearFeed() {
    confirmClear = true;
    clearTimeout(confirmTimer);
    confirmTimer = setTimeout(() => { confirmClear = false; }, 4000);
  }

  async function clearFeed() {
    confirmClear = false;
    clearTimeout(confirmTimer);
    try {
      await api("POST", "/api/journal/clear", undefined, { signal });
      void poll();
      toast("Лента чата очищена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    }
  }

  async function chipScreen() {
    if (chipBusy) return;
    chipBusy = "screen";
    try {
      await api("POST", "/api/pc/screen", { question: "Кратко опиши, что сейчас на экране." }, { signal });
      stickToBottom = true;
      void poll();
      toast("Экран проанализирован");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      chipBusy = "";
    }
  }

  async function chipFocus() {
    if (chipBusy) return;
    chipBusy = "focus";
    try {
      const res = await api("POST", "/api/focus", { minutes: focusRunning ? 0 : 25 }, { signal });
      await refreshActivity();
      toast(res.running ? "Фокус 25 мин запущен" : "Фокус остановлен");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      chipBusy = "";
    }
  }

  async function chipRecap() {
    if (chipBusy) return;
    chipBusy = "recap";
    try {
      const res = await api("POST", "/api/books/recap", {}, { signal });
      stickToBottom = true;
      void poll();
      toast(res.text || "Пересказ готов");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      chipBusy = "";
    }
  }

  function chipNote() {
    draft = "Запиши заметку: ";
    sayInput?.focus();
  }

  async function chipBriefing() {
    if (chipBusy) return;
    chipBusy = "briefing";
    try {
      await api("POST", "/api/briefing", { speak }, { signal });
      stickToBottom = true;
      void poll();
      toast("Брифинг готов");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      chipBusy = "";
    }
  }
</script>

<header class="tg-h">
  <span class="tg-av">
    <span id="now-ring" class="ringbox">{@html ring(snapshot.state.ring, "av")}</span>
    {#if snapshot.profile}{@html avatar(snapshot.profile)}{/if}
  </span>
  <div class="tg-who">
    <b>{snapshot.profile?.name || "Джарвис"}</b>
    <span id="tg-status" class="tg-st" class:live={snapshot.talking} class:warn={!snapshot.talking && snapshot.state.ring === "off"}>
      {status}
    </span>
  </div>
  {#if snapshot.profiles.length > 1 && snapshot.profile}
    <select class="tg-switch mobile-only" value={snapshot.profile.id} aria-label="Выбрать активного ассистента" onchange={switchProfile}>
      {#each snapshot.profiles as p (p.id)}
        <option value={p.id}>{p.name}</option>
      {/each}
    </select>
  {/if}
  <button class="ib lg" class:on={searchOpen} type="button" onclick={toggleSearch} title="Поиск по переписке" aria-label="Поиск по переписке">{@html icon("search")}</button>
  {#if rows.length}
    {#if confirmClear}
      <div class="tg-confirm" role="group" aria-label="Очистить переписку">
        <span>Очистить?</span>
        <button class="ib danger" type="button" onclick={clearFeed} aria-label="Подтвердить очистку">{@html icon("check")}</button>
        <button class="ib" type="button" onclick={() => { confirmClear = false; }} aria-label="Отменить">{@html icon("x")}</button>
      </div>
    {:else}
      <button class="ib lg" type="button" onclick={askClearFeed} title="Очистить ленту" aria-label="Очистить ленту чата">{@html icon("trash")}</button>
    {/if}
  {/if}
  {#if snapshot.profile}
    <a class="ib lg" href={profilePath(snapshot.profile.id)} aria-label="Настроить ассистента">{@html icon("tune")}</a>
  {/if}
</header>

{#if searchOpen}
  <div class="tg-search">
    {@html icon("search")}
    <input
      type="search"
      maxlength="120"
      placeholder="Поиск по переписке…"
      aria-label="Поиск по переписке"
      bind:this={searchInput}
      bind:value={searchQuery}
    >
    {#if searchQuery.trim()}
      <span class="meta">{visibleRows.length}</span>
    {/if}
    <button class="ib" type="button" onclick={toggleSearch} aria-label="Закрыть поиск">
      {@html icon("x")}
    </button>
  </div>
{/if}

<div class="tg-wrap">
  <div class="tg-body" id="feed" bind:this={feed} onscroll={onScroll}>
    {#each visibleRows as row (row.key)}
      <Message {row} {animate} follow={followFeed} />
    {:else}
      <div class="tg-empty">
        {#if searchQuery.trim()}
          <b>Ничего не найдено</b>
          <span>Попробуйте другой запрос</span>
        {:else}
          <b>Здесь будет переписка</b>
          <span>Скажите «{snapshot.wakeWord}, какая погода?» или напишите сообщение ниже</span>
        {/if}
      </div>
    {/each}
  </div>
  {#if scrolledUp}
    <button type="button" class="tg-jump" onclick={jumpToBottom} title="К новым сообщениям" aria-label="Вниз">
      {@html icon("arrowDown")}
    </button>
  {/if}
</div>

<div class="chat-chips" aria-label="Быстрые команды">
  <button type="button" class="chat-chip" disabled={chipBusy === "screen"} onclick={chipScreen}>
    {@html icon("monitor")} Экран ПК
  </button>
  <button type="button" class="chat-chip" class:on={focusRunning} disabled={chipBusy === "focus"} onclick={chipFocus}>
    {@html icon("bolt")} {focusRunning ? "Стоп фокус" : "Фокус 25м"}
  </button>
  <button type="button" class="chat-chip" disabled={chipBusy === "recap"} onclick={chipRecap}>
    {@html icon("book")} Суть книги
  </button>
  <button type="button" class="chat-chip" onclick={chipNote}>
    {@html icon("note")} Заметка
  </button>
  <button type="button" class="chat-chip" disabled={chipBusy === "briefing"} onclick={chipBriefing}>
    {@html icon("sparkles")} Брифинг
  </button>
</div>

<form class="tg-f" class:direct={directTts} id="say-form" onsubmit={send}>
  {#if directTts}
    <span class="tg-direct-tag">TTS</span>
  {/if}
  <input type="text" id="say" maxlength="600" bind:this={sayInput} bind:value={draft} autocomplete="off"
    aria-label={directTts ? "Озвучить текст на колонке" : "Сообщение ассистенту"}
    placeholder={directTts ? "Текст прозвучит голосом колонки без ИИ…" : snapshot.talking ? "Идёт разговор — можно и написать" : "Сообщение"}>
  <button class="ib lg" class:on={directTts} id="say-direct" type="button" aria-pressed={directTts} onclick={() => { directTts = !directTts; }}
    data-tip={directTts ? "Режим TTS: колонка скажет именно этот текст без ИИ" : "Переключить на прямую озвучку текста (TTS) без ИИ"}
    aria-label="Озвучить текст напрямую">{@html icon("broadcast")}</button>
  {#if !directTts}
    <button class="ib lg" class:on={speak} id="say-voice" type="button" aria-pressed={speak} onclick={toggleVoice}
      data-tip={speak ? "Ответ прозвучит на колонке. Нажмите, чтобы отвечал только текстом" : "Ответ только текстом. Нажмите, чтобы читал вслух на колонке"}
      aria-label="Читать ответ вслух на колонке">{@html icon(speak ? "voice" : "mute")}</button>
  {/if}
  <button
    class="ib lg"
    class:ptt-rec={pttRecording}
    id="ptt-btn"
    type="button"
    disabled={pttBusy || sending}
    aria-pressed={pttRecording}
    onclick={togglePtt}
    data-tip={pttRecording ? `Остановить запись (${pttSec}с) и отправить` : "Голос с этого устройства (браузер → Whisper)"}
    aria-label={pttRecording ? "Остановить запись" : "Сказать с микрофона устройства"}
  >{@html icon(pttRecording ? "stop" : "headset", pttRecording ? "solid" : "")}</button>
  {#if snapshot.talking}
    <button class="rb" id="stop" type="button" disabled={speakerActions.stop} onclick={() => controlSpeaker("stop")}
      data-tip="Оборвать разговор" aria-label="Стоп">{@html icon("stop", "solid")}</button>
  {/if}
  {#if typing}
    <button class="rb primary" id="send" type="submit" disabled={sending} aria-label="Отправить">{@html icon("send")}</button>
  {:else}
    <button class="rb primary" id="talk" type="button" disabled={speakerActions.talk} onclick={() => controlSpeaker("talk")}
      data-tip="Колонка сразу начнёт слушать — как при голосовой активации" aria-label="Поговорить">{@html icon("mic")}</button>
  {/if}
</form>
