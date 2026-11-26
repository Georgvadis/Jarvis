<script>
  import { onMount } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { S } from "#state/panelState.js";

  const controller = new AbortController();
  const { signal } = controller;
  const languages = [
    ["английский", "Английский"],
    ["немецкий", "Немецкий"],
    ["испанский", "Испанский"],
    ["французский", "Французский"],
    ["итальянский", "Итальянский"],
    ["японский", "Японский"],
    ["китайский", "Китайский"],
    ["корейский", "Корейский"],
    ["турецкий", "Турецкий"],
  ];

  let focusRunning = $state(false);
  let focusRem = $state(0);
  let activeMode = $state("normal");
  let activeLang = $state("");
  let drawer = $state(""); // "" | "debate" | "translator" | "bedtime"
  let debateTopic = $state("");
  let debateP1 = $state("");
  let debateP2 = $state("");
  let debateRounds = $state(2);
  let transLang = $state("английский");
  let bedtimeTopic = $state("");
  let busy = $state(false);
  let tickTimer;
  let syncTimer;

  let profiles = $derived(S.profiles || []);

  onMount(() => {
    void loadStatus();
    tickTimer = setInterval(() => {
      if (focusRunning && focusRem > 0) focusRem = Math.max(0, focusRem - 1);
      if (focusRunning && focusRem === 0) focusRunning = false;
    }, 1000);
    syncTimer = setInterval(() => void loadStatus(), 4000);
    return () => {
      clearInterval(tickTimer);
      clearInterval(syncTimer);
      controller.abort();
    };
  });

  async function loadStatus() {
    try {
      const [f, m] = await Promise.all([
        api("GET", "/api/focus", undefined, { signal }),
        api("GET", "/api/modes", undefined, { signal }),
      ]);
      if (signal.aborted) return;
      focusRunning = !!f.running;
      focusRem = Number(f.remaining_s || 0);
      activeMode = m.mode || "normal";
      activeLang = m.lang || "";
      if (activeLang) transLang = activeLang;
    } catch { /* ignore */ }
  }

  async function toggleFocus() {
    if (busy) return;
    busy = true;
    try {
      const data = await api("POST", "/api/focus", { minutes: focusRunning ? 0 : 25 }, { signal });
      if (signal.aborted) return;
      focusRunning = !!data.running;
      focusRem = Number(data.remaining_s || 0);
      toast(focusRunning ? "Фокус 25 мин запущен" : "Фокус остановлен");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function toggleMode(mode, lang = "") {
    if (busy) return;
    busy = true;
    const next = activeMode === mode && (mode !== "translator" || activeLang === lang) ? "off" : mode;
    try {
      const res = await api("POST", "/api/modes", { mode: next, lang }, { signal });
      if (signal.aborted) return;
      activeMode = res.mode || "normal";
      activeLang = res.lang || "";
      drawer = "";
      toast(activeMode === "normal" ? "Режим выключен" : "Режим включён");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function stopActiveModes() {
    if (busy) return;
    busy = true;
    try {
      const tasks = [];
      if (focusRunning) tasks.push(api("POST", "/api/focus", { minutes: 0 }, { signal }));
      if (activeMode !== "normal") tasks.push(api("POST", "/api/modes", { mode: "off" }, { signal }));
      await Promise.all(tasks);
      if (signal.aborted) return;
      focusRunning = false;
      focusRem = 0;
      activeMode = "normal";
      activeLang = "";
      toast("Режимы остановлены");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  function openDebateDrawer() {
    if (drawer === "debate") {
      drawer = "";
      return;
    }
    if (!debateP1 && profiles[0]) debateP1 = profiles[0].id;
    if (!debateP2 && profiles[1]) debateP2 = profiles[1].id;
    drawer = "debate";
  }

  async function submitDebate(event) {
    event.preventDefault();
    if (busy) return;
    const topic = debateTopic.trim() || "Что лучше — кофе или чай?";
    busy = true;
    try {
      const payload = { topic, rounds: debateRounds };
      if (debateP1) payload.p1_id = debateP1;
      if (debateP2 && debateP2 !== debateP1) payload.p2_id = debateP2;
      await api("POST", "/api/debate", payload, { signal });
      if (signal.aborted) return;
      drawer = "";
      debateTopic = "";
      toast("Мини-подкаст запущен");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function submitBedtime(event) {
    event.preventDefault();
    if (busy) return;
    const topic = bedtimeTopic.trim() || "уютный маяк у ночного океана";
    busy = true;
    try {
      await api("POST", "/api/modes", { mode: "bedtime", topic }, { signal });
      if (signal.aborted) return;
      drawer = "";
      bedtimeTopic = "";
      toast("История на ночь запущена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  function fmt(sec) {
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
</script>

<section class="card" aria-label="Интерактивные режимы">
  <h2>
    {@html icon("sparkles")} Режимы
    {#if focusRunning || activeMode !== "normal"}
      <button class="btn sm ghost amb-stop" type="button" disabled={busy} onclick={stopActiveModes}>
        {@html icon("stop", "solid")} Стоп
      </button>
    {/if}
  </h2>
  <div class="amb-grid">
    <button type="button" class="amb-btn" class:on={focusRunning} disabled={busy} onclick={toggleFocus}>
      {@html icon("bolt")}
      <span>{focusRunning ? `Фокус ${fmt(focusRem)}` : "Фокус 25м"}</span>
    </button>
    <button type="button" class="amb-btn" class:on={drawer === "debate"} disabled={busy} onclick={openDebateDrawer}>
      {@html icon("sparkles")}
      <span>Мини-подкаст</span>
    </button>
    <button type="button" class="amb-btn" class:on={activeMode === "rpg"} disabled={busy} onclick={() => toggleMode("rpg")}>
      {@html icon("shield")}
      <span>RPG-квест</span>
    </button>
    <button type="button" class="amb-btn" class:on={activeMode === "quiz"} disabled={busy} onclick={() => toggleMode("quiz")}>
      {@html icon("cpu")}
      <span>Викторина</span>
    </button>
    <button type="button" class="amb-btn" class:on={activeMode === "translator" || drawer === "translator"} disabled={busy}
      onclick={() => { drawer = drawer === "translator" ? "" : "translator"; }}>
      {@html icon("globe")}
      <span>{activeMode === "translator" && activeLang ? `Перевод (${activeLang})` : "Переводчик"}</span>
    </button>
    <button type="button" class="amb-btn" class:on={drawer === "bedtime"} disabled={busy}
      onclick={() => { drawer = drawer === "bedtime" ? "" : "bedtime"; }}>
      {@html icon("moon")}
      <span>История на ночь</span>
    </button>
  </div>

  {#if drawer === "debate"}
    <form class="mode-drawer" onsubmit={submitDebate}>
      <input type="text" maxlength="300" placeholder="Тема спора, например: Кофе или чай?" bind:value={debateTopic} disabled={busy}>
      {#if profiles.length >= 2}
        <div class="mode-row">
          <select aria-label="Участник 1" bind:value={debateP1} disabled={busy}>
            {#each profiles as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
          </select>
          <select aria-label="Участник 2" bind:value={debateP2} disabled={busy}>
            {#each profiles as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
          </select>
        </div>
      {/if}
      <div class="mode-foot">
        <div class="seg sm" role="group" aria-label="Раунды">
          {#each [2, 3, 4] as r (r)}
            <button type="button" class:on={debateRounds === r} onclick={() => { debateRounds = r; }}>{r}р</button>
          {/each}
        </div>
        <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("play", "solid")}Запустить</button>
      </div>
    </form>
  {:else if drawer === "translator"}
    <div class="mode-drawer">
      <div class="mode-row">
        <select aria-label="Язык перевода" bind:value={transLang} disabled={busy}>
          {#each languages as [val, label] (val)}
            <option value={val}>{label}</option>
          {/each}
        </select>
        <button type="button" class="btn sm primary" disabled={busy} onclick={() => toggleMode("translator", transLang)}>
          {@html icon(activeMode === "translator" && activeLang === transLang ? "stop" : "play", "solid")}
          {activeMode === "translator" && activeLang === transLang ? "Выключить" : "Включить"}
        </button>
      </div>
    </div>
  {:else if drawer === "bedtime"}
    <form class="mode-drawer" onsubmit={submitBedtime}>
      <div class="mode-row">
        <input type="text" maxlength="200" placeholder="Тема: уютный маяк у океана" bind:value={bedtimeTopic} disabled={busy}>
        <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("moon")}Слушать</button>
      </div>
    </form>
  {/if}
</section>
