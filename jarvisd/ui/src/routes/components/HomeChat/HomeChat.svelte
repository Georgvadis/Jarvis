<script>
  import { onMount } from "svelte";
  import { api, icon, ring, toast } from "#helpers/core.js";
  import { avatar, journal, refreshRoute, reload, S, speakerState } from "#state/panelState.js";
  import { profilePath } from "#helpers/paths.js";
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
  let showDebate = $state(false);
  let debateTopic = $state("");
  let debateRounds = $state(2);
  const speakerActions = $state({ talk: false, stop: false });
  let feed = $state();
  let animate = $state(false);
  let stickToBottom = true;
  let pollTimer;

  let rows = $derived(chatMessages(snapshot.turns, snapshot.phase));
  let status = $derived(snapshot.talking ? "говорит и слушает…"
    : snapshot.state.title.replace(/^Жду/, "ждёт").replace(/^Микрофон/, "микрофон"));
  let typing = $derived(draft.trim() !== "");

  onMount(() => {
    animate = true;
    return () => {
      controller.abort();
      clearTimeout(pollTimer);
    };
  });

  $effect(() => {
    // После обновления строк прокручиваем ленту, если пользователь уже был внизу.
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
    catch { /* Настройка всё равно действует до ухода со страницы. */ }
  }

  function onScroll(event) {
    const node = event.currentTarget;
    stickToBottom = node.scrollHeight - node.scrollTop - node.clientHeight < 80;
  }

  function followFeed() {
    if (stickToBottom && feed?.isConnected) feed.scrollTop = feed.scrollHeight;
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
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      speakerActions[action] = false;
    }
  }

  async function clearFeed() {
    try {
      await api("POST", "/api/journal/clear", undefined, { signal });
      void poll();
      toast("Лента чата очищена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    }
  }

  async function submitDebate(event) {
    event.preventDefault();
    const topic = debateTopic.trim() || "Что лучше — кофе или чай?";
    try {
      await api("POST", "/api/debate", { topic, rounds: debateRounds }, { signal });
      if (signal.aborted) return;
      showDebate = false;
      debateTopic = "";
      toast("Мини-подкаст запущен!");
      setTimeout(() => void poll(), 1000);
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
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
    <select class="tg-switch" value={snapshot.profile.id} aria-label="Выбрать активного ассистента" onchange={switchProfile}>
      {#each snapshot.profiles as p (p.id)}
        <option value={p.id}>{p.name}</option>
      {/each}
    </select>
  {/if}
  <button class="ib lg" class:on={showDebate} type="button" onclick={() => { showDebate = !showDebate; }} title="Мини-подкаст / спор двух ассистентов" aria-label="Запустить спор двух ассистентов">{@html icon("sparkles")}</button>
  {#if rows.length}
    <button class="ib lg" type="button" onclick={clearFeed} title="Очистить ленту" aria-label="Очистить ленту чата">{@html icon("trash")}</button>
  {/if}
  {#if snapshot.profile}
    <a class="ib lg" href={profilePath(snapshot.profile.id)} aria-label="Настроить ассистента">{@html icon("tune")}</a>
  {/if}
</header>

{#if showDebate}
  <form class="debate-bar" onsubmit={submitDebate}>
    <input type="text" maxlength="300" placeholder="Тема мини-подкаста / спора…" bind:value={debateTopic}>
    <select aria-label="Раунды" bind:value={debateRounds}>
      <option value={2}>2р</option>
      <option value={3}>3р</option>
      <option value={4}>4р</option>
    </select>
    <button class="btn sm primary" type="submit">{@html icon("play", "solid")}Старт</button>
    <button class="ib" type="button" onclick={() => { showDebate = false; }} aria-label="Закрыть">{@html icon("x")}</button>
  </form>
{/if}

<div class="tg-body" id="feed" bind:this={feed} onscroll={onScroll}>
  {#each rows as row (row.key)}
    <Message {row} {animate} follow={followFeed} />
  {:else}
    <div class="tg-empty">
      <b>Здесь будет переписка</b>
      <span>Скажите «{snapshot.wakeWord}, какая погода?» — вопрос и ответ появятся тут</span>
    </div>
  {/each}
</div>

<form class="tg-f" id="say-form" onsubmit={send}>
  <input type="text" id="say" maxlength="600" bind:value={draft} autocomplete="off" aria-label={directTts ? "Озвучить текст на колонке" : "Сообщение ассистенту"}
    placeholder={directTts ? "Озвучить текст на колонке напрямую…" : snapshot.talking ? "Идёт разговор — можно и написать" : "Сообщение"}>
  <button class="ib lg" class:on={directTts} id="say-direct" type="button" aria-pressed={directTts} onclick={() => { directTts = !directTts; }}
    data-tip={directTts ? "Режим TTS: колонка скажет именно этот текст без ИИ" : "Переключить на прямую озвучку текста (TTS) без ИИ"}
    aria-label="Озвучить текст напрямую">{@html icon("broadcast")}</button>
  {#if !directTts}
    <button class="ib lg" class:on={speak} id="say-voice" type="button" aria-pressed={speak} onclick={toggleVoice}
      data-tip={speak ? "Ответ прозвучит на колонке. Нажмите, чтобы отвечал только текстом" : "Ответ только текстом. Нажмите, чтобы читал вслух на колонке"}
      aria-label="Читать ответ вслух на колонке">{@html icon(speak ? "voice" : "mute")}</button>
  {/if}
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
