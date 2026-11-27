<script>
  import { onMount } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, fmtClock, refreshActivity, runActivityNext, runScenario, stopActivityItem, summarizeActivity } from "#state/activity.js";
  import { getPanel } from "#state/panelContext.js";
  import { S, speaker } from "#state/panelState.js";
  import HomeAmbient from "./HomeAmbient/HomeAmbient.svelte";
  import HomeChat from "./HomeChat/HomeChat.svelte";
  import { readOverview } from "./HomeOverview/overview.js";
  import HomePlayer from "./HomePlayer/HomePlayer.svelte";
  import HomeStrip from "./HomeStrip/HomeStrip.svelte";
  import HomeVolume from "./HomeVolume/HomeVolume.svelte";

  const BASE_SCENES = [
    { id: "morning", label: "Утро", hint: "Джаз · бодрый старт", ic: "sun" },
    { id: "work", label: "Работа", hint: "Фокус · тихий Lo-Fi", ic: "briefcase" },
    { id: "evening", label: "Вечер", hint: "Уют · мягкий свет", ic: "coffee" },
    { id: "sleep", label: "Сон", hint: "Океан · таймер 20м", ic: "moon" },
  ];

  const dateFmt = new Intl.DateTimeFormat("ru-RU", { weekday: "long", day: "numeric", month: "long" });
  const timeFmt = new Intl.DateTimeFormat("ru-RU", { hour: "2-digit", minute: "2-digit" });

  const { poll, onPoll } = getPanel();
  let overview = $state.raw(readOverview(S));
  let activeProfile = $state.raw(S.profiles.find((p) => p.id === S.active) || S.profiles[0]);
  let sys = $state.raw(S.sys || {});
  let chat = $state();
  let now = $state(new Date());

  let loungeTab = $state("ambient");
  let chatExpanded = $state(false);
  let busy = $state("");
  let pcBusy = $state("");
  let isMobile = $state(false);

  let presence = $derived($activity.presence || { state: "unknown" });
  let ambActive = $derived($activity.ambient?.active || "");
  let fireplaceOn = $derived(ambActive === "fireplace");
  let focusRunning = $derived(Boolean($activity.focus?.running));
  let focusRem = $derived(Number($activity.focus?.remaining_s || 0));
  let isTalking = $state(Boolean(S.speaker?.talking));
  let hasPc = $derived(Boolean($speaker?.settings?.pc_agent_url?.trim()));

  let liveItems = $derived(summarizeActivity($activity));
  let nonAmbientLive = $derived(liveItems.filter((i) => i.id !== "ambient"));
  let activeTimers = $derived(Array.isArray($activity.timers) ? $activity.timers : []);
  let anyActive = $derived(Boolean(liveItems.length || isTalking));

  let clockText = $derived(timeFmt.format(now));
  let dateText = $derived(dateFmt.format(now));
  let greeting = $derived.by(() => {
    const h = now.getHours();
    if (h >= 5 && h < 12) return "Доброе утро";
    if (h >= 12 && h < 18) return "Добрый день";
    if (h >= 18 && h < 23) return "Добрый вечер";
    return "Доброй ночи";
  });

  onMount(() => {
    const offPoll = onPoll(refresh);
    const clockTimer = setInterval(() => { now = new Date(); }, 15000);
    const mq = window.matchMedia("(max-width: 860px)");
    isMobile = mq.matches;
    const onMq = (e) => { isMobile = e.matches; };
    mq.addEventListener("change", onMq);
    return () => {
      offPoll();
      clearInterval(clockTimer);
      mq.removeEventListener("change", onMq);
    };
  });

  function refresh() {
    overview = readOverview(S);
    activeProfile = S.profiles.find((p) => p.id === S.active) || S.profiles[0];
    sys = S.sys || {};
    isTalking = Boolean(S.speaker?.talking);
    chat?.refresh();
  }

  async function toggleTalk() {
    if (busy) return;
    busy = "talk";
    const stopping = isTalking;
    try {
      await api("POST", stopping ? "/api/stop" : "/api/talk");
      if (!stopping) toast("Слушаю…");
      else await refreshActivity();
    } catch (e) {
      toast(e.message, true);
    } finally {
      busy = "";
    }
  }

  async function stopAll() {
    if (busy) return;
    busy = "stopall";
    try {
      await api("POST", "/api/stop", {});
      await refreshActivity();
      toast("Остановлено");
    } catch (e) {
      toast(e.message, true);
    } finally {
      busy = "";
    }
  }

  async function toggleFocus() {
    if (busy) return;
    busy = "focus";
    try {
      const res = await api("POST", "/api/focus", { minutes: focusRunning ? 0 : 25 });
      await refreshActivity();
      toast(res.running ? "Фокус 25 мин запущен" : "Фокус остановлен");
    } catch (e) {
      toast(e.message, true);
    } finally {
      busy = "";
    }
  }

  async function runBriefing() {
    if (busy) return;
    busy = "briefing";
    try {
      await api("POST", "/api/briefing", { speak: true });
      toast("Озвучиваю брифинг");
    } catch (e) {
      toast(e.message, true);
    } finally {
      busy = "";
    }
  }

  async function triggerScene(id) {
    if (busy) return;
    busy = "sc:" + id;
    try {
      await runScenario(id);
    } finally {
      busy = "";
    }
  }

  async function quickTimer(minutes) {
    if (busy) return;
    busy = "t:" + minutes;
    try {
      await api("POST", "/api/schedule/timer", { minutes, label: `Таймер ${minutes}м` });
      await refreshActivity();
      toast(`Таймер на ${minutes} мин`);
    } catch (e) {
      toast(e.message, true);
    } finally {
      busy = "";
    }
  }

  async function cancelTimer(id) {
    if (busy) return;
    busy = "ct:" + id;
    try {
      await api("DELETE", `/api/schedule/timer/${encodeURIComponent(id)}`);
      await refreshActivity();
      toast("Таймер снят");
    } catch (e) {
      toast(e.message, true);
    } finally {
      busy = "";
    }
  }

  async function pcCmd(action) {
    if (pcBusy) return;
    pcBusy = action;
    try {
      if (action === "screen") {
        const res = await api("POST", "/api/pc/screen", { question: "Кратко опиши, что сейчас на экране." });
        toast(res.text || "Экран проанализирован");
      } else if (action === "clip_speak") {
        await api("POST", "/api/pc/clipboard", { speak: true });
        toast("Озвучиваю буфер ПК");
      } else {
        const res = await api("POST", "/api/pc/action", { action });
        toast(res.message || "Выполнено на ПК");
      }
    } catch (e) {
      toast(e.message, true);
    } finally {
      pcBusy = "";
    }
  }
</script>

<div class="home-living">
  <!-- Уровень 1: Каминный очаг (Fireside Hearth Hero) -->
  <section class="card hearth-stage" class:lit={fireplaceOn} aria-label="Домашний очаг">
    <div class="hearth-embers" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></div>

    <div class="hearth-top">
      <span class="hearth-status-row">
        <span class="dot" class:on={overview.micOnline} class:off={!overview.micOnline} id="home-dot"></span>
        <span id="home-sub">{overview.micOnline ? "На связи" : "Микрофон выкл"}</span>
        {#if presence.configured && presence.state !== "unknown"}
          <span class="sep">·</span>
          <span>{presence.state === "home" ? "Дома" : "Нет дома"}</span>
        {/if}
        {#if activeProfile}
          <span class="sep">·</span>
          <span>{activeProfile.name}</span>
        {/if}
      </span>

      <HomeStrip {activeProfile} {sys} />
    </div>

    <div class="hearth-main hearth-main-compact">
      <div class="hearth-title-row">
        <span class="hearth-flame-orb" class:talking={isTalking} aria-hidden="true">
          {@html icon("flame")}
        </span>
        <div class="hearth-copy">
          <div class="hearth-clock-line">
            <span class="hearth-clock">{clockText}</span>
            <h1>{greeting}</h1>
          </div>
          <span class="hearth-date">{dateText}</span>
        </div>
      </div>

      <div class="hearth-actions">
        <button type="button" class="btn primary hearth-cta" disabled={busy === "talk"} onclick={toggleTalk}>
          {@html icon(isTalking ? "stop" : "mic", isTalking ? "solid" : "")}
          <span>{isTalking ? "Остановить" : "Поговорить"}</span>
        </button>
        <button type="button" class="btn hearth-pill-btn" class:on={focusRunning} disabled={busy === "focus"} onclick={toggleFocus}>
          {@html icon("bolt")}
          <span>{focusRunning ? `Фокус ${fmtClock(focusRem)}` : "Фокус 25м"}</span>
        </button>
        <button type="button" class="btn hearth-pill-btn" disabled={busy === "briefing"} onclick={runBriefing}>
          {@html icon("sparkles")}
          <span>Брифинг</span>
        </button>
        {#if anyActive}
          <button type="button" class="btn hearth-pill-btn danger-soft" disabled={busy === "stopall"} onclick={stopAll} title="Остановить все активные режимы">
            {@html icon("stop", "solid")}
            <span>Стоп всё</span>
          </button>
        {/if}
      </div>
    </div>
  </section>

  <!-- Уровень 2: Лента сценариев дома -->
  <section class="scene-ribbon" aria-label="Сценарии дома">
    {#each BASE_SCENES as sc (sc.id)}
      <button type="button" class="scene-tile" data-sc={sc.id} disabled={Boolean(busy)} onclick={() => triggerScene(sc.id)}>
        <span class="scene-badge">{@html icon(sc.ic)}</span>
        <span class="scene-copy">
          <b>{sc.label}</b>
          <small>{sc.hint}</small>
        </span>
      </button>
    {/each}
    {#each $activity.custom_scenarios || [] as sc (sc.id)}
      <button type="button" class="scene-tile" data-sc="custom" disabled={Boolean(busy)} onclick={() => triggerScene(sc.id)}>
        <span class="scene-badge">{@html icon(sc.icon || "sparkles")}</span>
        <span class="scene-copy">
          <b>{sc.name}</b>
          <small>{sc.phrase ? `«${sc.phrase}»` : `Шагов: ${sc.steps_count || 1}`}</small>
        </span>
      </button>
    {/each}
  </section>

  <!-- Уровень 3: Сбалансированная гостиная (Беседа + Уют и управление) -->
  <div class="home" class:chat-expanded={chatExpanded}>
    <section class="tg" id="home-chat" aria-label="Переписка с ассистентом">
      <HomeChat {poll} expanded={chatExpanded} onToggleExpand={() => { chatExpanded = !chatExpanded; }} bind:this={chat} />
    </section>

    {#if !chatExpanded}
      <aside class="rail">
        <section class="card lounge-card" aria-label="Уют и управление">
          <div class="lounge-head">
            <div class="lounge-tabs" role="tablist" aria-label="Панель уюта">
              <button type="button" role="tab" aria-selected={loungeTab === "ambient"} class="lounge-tab" class:on={loungeTab === "ambient"} onclick={() => { loungeTab = "ambient"; }}>
                {@html icon("flame")}<span>Атмосфера</span>
              </button>
              <button type="button" role="tab" aria-selected={loungeTab === "tools"} class="lounge-tab" class:on={loungeTab === "tools"} onclick={() => { loungeTab = "tools"; }}>
                {@html icon("clock")}<span>Таймеры и дом</span>
                {#if activeTimers.length || nonAmbientLive.length}<span class="sdot"></span>{/if}
              </button>
            </div>
            <a class="quick-link" href="/studio">Студия {@html icon("right")}</a>
          </div>

          <div class="lounge-body">
            {#if loungeTab === "ambient"}
              <HomeAmbient embedded={true} />
            {:else}
              <div class="lounge-section-lbl">
                <span>Быстрый таймер</span>
                <a class="quick-link" href="/studio?tab=schedule">Расписание {@html icon("right")}</a>
              </div>
              <div class="timer-quick-row">
                {#each [5, 15, 30] as m (m)}
                  <button type="button" class="scen-btn" disabled={Boolean(busy)} onclick={() => quickTimer(m)}>
                    {@html icon("clock")}
                    <span>{m} мин</span>
                  </button>
                {/each}
              </div>

              {#if activeTimers.length}
                <div class="lounge-section-lbl"><span>Идут сейчас</span></div>
                <div class="sched-list">
                  {#each activeTimers as t (t.id)}
                    <div class="timer-row">
                      <span class="timer-ic">{@html icon("clock")}</span>
                      <div class="timer-main"><strong>{t.label}</strong></div>
                      <span class="badge ok timer-clock">{fmtClock(t.remaining_s)}</span>
                      <button type="button" class="ib" aria-label="Снять таймер" onclick={() => cancelTimer(t.id)}>{@html icon("x")}</button>
                    </div>
                  {/each}
                </div>
              {/if}

              {#if nonAmbientLive.length}
                <div class="lounge-section-lbl"><span>Активные режимы</span></div>
                <div class="live-deck-list">
                  {#each nonAmbientLive as item (item.id)}
                    <div class="live-row">
                      <a class="live-link" href={item.href}>
                        <b>{item.title}</b>
                        {#if item.subtitle}<small>{item.subtitle}</small>{/if}
                      </a>
                      <div class="live-acts">
                        {#if item.canNext}
                          <button type="button" class="ib" title={item.nextLabel || "Далее"} aria-label={item.nextLabel || "Далее"} onclick={() => void runActivityNext(item)}>
                            {@html icon("skip")}
                          </button>
                        {/if}
                        <button type="button" class="ib danger" title="Остановить" aria-label="Остановить" onclick={() => void stopActivityItem(item)}>
                          {@html icon("stop", "solid")}
                        </button>
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}

              {#if hasPc}
                <div class="lounge-section-lbl"><span>ПК-компаньон</span></div>
                <div class="pc-bar" aria-label="Управление ПК">
                  <span class="pc-bar-lbl">{@html icon("monitor")} ПК</span>
                  <div class="pc-bar-btns">
                    <button type="button" class="ib" disabled={!!pcBusy} onclick={() => pcCmd("screen")} title="Посмотреть экран" aria-label="Посмотреть экран">
                      {@html icon("eye")}
                    </button>
                    <button type="button" class="ib" disabled={!!pcBusy} onclick={() => pcCmd("clip_speak")} title="Озвучить буфер ПК" aria-label="Озвучить буфер ПК">
                      {@html icon("clipboard")}
                    </button>
                    <button type="button" class="ib" disabled={!!pcBusy} onclick={() => pcCmd("media_play_pause")} title="Медиа плей/пауза" aria-label="Медиа плей/пауза">
                      {@html icon("play", "solid")}
                    </button>
                    <button type="button" class="ib" disabled={!!pcBusy} onclick={() => pcCmd("screenshot_save")} title="Сохранить скриншот" aria-label="Сохранить скриншот">
                      {@html icon("camera")}
                    </button>
                    <button type="button" class="ib" disabled={!!pcBusy} onclick={() => pcCmd("lock_screen")} title="Заблокировать экран ПК" aria-label="Заблокировать экран ПК">
                      {@html icon("lock")}
                    </button>
                  </div>
                </div>
              {:else}
                <a class="lounge-pc-cta" href="/speaker?tab=integrations">
                  {@html icon("monitor")}
                  <span>Подключить ПК-компаньон</span>
                  {@html icon("right")}
                </a>
              {/if}
            {/if}
          </div>
        </section>

        {#if isMobile}
          <div class="mobile-only-cards">
            <HomePlayer />
            <HomeVolume />
          </div>
        {/if}
      </aside>
    {/if}
  </div>
</div>
