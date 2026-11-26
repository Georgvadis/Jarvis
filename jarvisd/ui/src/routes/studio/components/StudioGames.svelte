<script>
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";

  const PARTY_STORAGE = "jarvis.partyPlayers";
  const melodyThemes = ["Аниме-опенинги", "Хиты 2000-х", "Рок", "Игры"];
  const GAME_CARDS = [
    { id: "melody", title: "Угадай мелодию", hint: "8 сек · счёт", ic: "music" },
    { id: "detective", title: "Нуарный детектив", hint: "Улики и допрос", ic: "eye" },
    { id: "party", title: "Мафия / Бункер", hint: "Компания 3+", ic: "users" },
    { id: "modes", title: "RPG и Викторина", hint: "Голосовые режимы", ic: "gamepad" },
  ];

  function loadPlayers() {
    try {
      const raw = JSON.parse(localStorage.getItem(PARTY_STORAGE) || "null");
      if (Array.isArray(raw) && raw.length >= 3) return raw;
    } catch { /* ignore */ }
    return [
      { name: "Игрок 1", chat_id: "" },
      { name: "Игрок 2", chat_id: "" },
      { name: "Игрок 3", chat_id: "" },
      { name: "Игрок 4", chat_id: "" },
    ];
  }

  let selectedGame = $state("melody");
  let melodyTheme = $state("Аниме-опенинги");
  let detectiveTheme = $state("");
  let partyGame = $state("mafia");
  let partyDiscussion = $state(60);
  let partyPlayers = $state(loadPlayers());
  let revealedRoles = $state({});
  let busy = $state(false);

  $effect(() => {
    try {
      localStorage.setItem(PARTY_STORAGE, JSON.stringify(partyPlayers));
    } catch { /* ignore */ }
  });

  let games = $derived($activity.games || { active_game: "", melody: {}, detective: {}, party: {} });
  let mode = $derived($activity.modes?.mode || "normal");

  function isLive(id) {
    if (id === "modes") return mode === "rpg" || mode === "quiz";
    return games.active_game === id;
  }

  async function startMelody(theme = melodyTheme) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/games/melody", { theme });
      await refreshActivity();
      toast("Угадай мелодию запущена");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function skipMelody() {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/games/melody/skip", {});
      await refreshActivity();
      toast("Трек пропущен");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function startDetective(event) {
    event?.preventDefault();
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/games/detective", { theme: detectiveTheme.trim() });
      detectiveTheme = "";
      await refreshActivity();
      toast("Нуарный детектив начат");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function startParty(event) {
    event.preventDefault();
    if (busy) return;
    const players = partyPlayers.map((p) => ({ name: p.name.trim(), chat_id: p.chat_id.trim() })).filter((p) => p.name);
    if (players.length < 3) return toast("Нужно минимум 3 игрока", true);
    busy = true;
    try {
      revealedRoles = {};
      await api("POST", "/api/games/party/start", { game: partyGame, discussion_s: partyDiscussion, players });
      await refreshActivity();
      toast("Роли розданы");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function nextPartyPhase() {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", "/api/games/party/next", {});
      await refreshActivity();
      toast(res.message || "Следующая фаза");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function toggleMode(target) {
    if (busy) return;
    busy = true;
    const next = mode === target ? "off" : target;
    try {
      await api("POST", "/api/modes", { mode: next });
      await refreshActivity();
      toast(next === "off" ? "Режим выключен" : "Режим включён");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function stopGames() {
    await api("POST", "/api/games/stop", {});
    await refreshActivity();
    toast("Игра остановлена");
  }
</script>

<div class="studio-stack">
  <div class="scene-ribbon" role="tablist" aria-label="Выбор игры">
    {#each GAME_CARDS as card (card.id)}
      <button
        type="button"
        role="tab"
        aria-selected={selectedGame === card.id}
        class="scene-tile game-pick"
        class:on={selectedGame === card.id}
        onclick={() => { selectedGame = card.id; }}
      >
        <span class="scene-badge">{@html icon(card.ic)}</span>
        <span class="scene-copy">
          <b>{card.title}</b>
          <small>{isLive(card.id) ? "Идёт игра" : card.hint}</small>
        </span>
        {#if isLive(card.id)}<span class="sdot"></span>{/if}
      </button>
    {/each}
  </div>

  {#if selectedGame === "melody"}
    <section class="card">
      <div class="card-head">
        <h2>{@html icon("music")} Угадай мелодию</h2>
        {#if games.active_game === "melody"}
          <span class="badge acc">Раунд {games.melody.round}/{games.melody.total} · Счёт {games.melody.score}</span>
        {/if}
      </div>
      {#if games.active_game === "melody"}
        <div class="dj-bar">
          <div class="dj-info">
            <b>{games.melody.theme}</b>
            <small>Раунд {games.melody.round}/{games.melody.total} · Счёт {games.melody.score}</small>
          </div>
          <div class="dj-actions">
            <button type="button" class="btn sm ghost" disabled={busy} onclick={skipMelody}>{@html icon("skip")} Пропустить</button>
            <button type="button" class="ib danger" onclick={stopGames} aria-label="Завершить">{@html icon("stop", "solid")}</button>
          </div>
        </div>
      {/if}
      <form class="studio-stack" onsubmit={(e) => { e.preventDefault(); startMelody(melodyTheme); }}>
        <div class="chip-row">
          {#each melodyThemes as theme (theme)}
            <button type="button" class="btn sm" onclick={() => { melodyTheme = theme; startMelody(theme); }}>{theme}</button>
          {/each}
        </div>
        <div class="mode-row">
          <input type="text" maxlength="100" placeholder="Любая тема…" bind:value={melodyTheme} disabled={busy}>
          <button type="submit" class="btn primary" disabled={busy}>{@html icon("play", "solid")} Старт</button>
        </div>
      </form>
    </section>
  {:else if selectedGame === "detective"}
    <section class="card">
      <div class="card-head">
        <h2>{@html icon("eye")} Нуарный детектив</h2>
        {#if games.active_game === "detective"}
          <span class="badge acc">Улики {games.detective.clues_shown}/{games.detective.clues_total}</span>
        {/if}
      </div>
      {#if games.active_game === "detective"}
        <div class="dj-bar">
          <div class="dj-info">
            <b>{games.detective.title}</b>
            <small>Улик открыто: {games.detective.clues_shown}/{games.detective.clues_total}</small>
          </div>
          <button type="button" class="ib danger" onclick={stopGames} aria-label="Завершить">{@html icon("stop", "solid")}</button>
        </div>
        {#if games.detective.suspects?.length}
          <div class="suspect-pills">
            {#each games.detective.suspects as s (s.name)}
              <span class="suspect-pill">{s.name} · {s.role}</span>
            {/each}
          </div>
        {/if}
      {/if}
      <form class="studio-stack" onsubmit={startDetective}>
        <div class="mode-row">
          <input type="text" maxlength="100" placeholder="Сеттинг (например: особняк 1940-х)" bind:value={detectiveTheme} disabled={busy}>
          <button type="submit" class="btn primary" disabled={busy}>{@html icon("play", "solid")} Новое дело</button>
        </div>
      </form>
    </section>
  {:else if selectedGame === "party"}
    <section class="card">
      <div class="card-head">
        <h2>{@html icon("users")} Мафия / Бункер</h2>
        {#if games.active_game === "party"}
          <div class="actions">
            <span class="badge acc">{games.party.game === "bunker" ? "Бункер" : "Мафия"} · Раунд {games.party.round} · {games.party.phase}</span>
            <button type="button" class="btn sm primary" disabled={busy} onclick={nextPartyPhase}>{@html icon("skip")} Следующая фаза</button>
            <button type="button" class="btn sm ghost danger" onclick={stopGames}>{@html icon("stop", "solid")} Завершить</button>
          </div>
        {/if}
      </div>

      {#if games.active_game === "party" && games.party.game === "bunker" && games.party.scenario}
        <div class="bunker-banner">
          <b>Катастрофа:</b> {games.party.scenario}
        </div>
      {/if}

      {#if games.active_game === "party" && games.party.roles?.length}
        <div class="role-grid">
          {#each games.party.roles as r (r.name)}
            <button type="button" class="role-card-full" onclick={() => { revealedRoles = { ...revealedRoles, [r.name]: !revealedRoles[r.name] }; }}>
              <b>{r.name}</b>
              {#if r.dm_sent}
                <small>Роль отправлена в Telegram</small>
              {:else if revealedRoles[r.name]}
                <small>{r.role}</small>
                {#if r.detail}<div class="role-detail">{r.detail}</div>{/if}
              {:else}
                <small>Нажмите, чтобы открыть карту</small>
              {/if}
            </button>
          {/each}
        </div>
        <div class="nav-sep"></div>
      {/if}

      <form class="studio-stack" onsubmit={startParty}>
        <div class="mode-foot">
          <div class="seg sm" role="group" aria-label="Игра">
            <button type="button" class:on={partyGame === "mafia"} onclick={() => { partyGame = "mafia"; }}>Мафия</button>
            <button type="button" class:on={partyGame === "bunker"} onclick={() => { partyGame = "bunker"; }}>Бункер</button>
          </div>
          <div class="seg sm" role="group" aria-label="Таймер обсуждения">
            {#each [60, 90, 120] as sec (sec)}
              <button type="button" class:on={partyDiscussion === sec} onclick={() => { partyDiscussion = sec; }}>{sec}с</button>
            {/each}
          </div>
        </div>

        <div class="party-list">
          {#each partyPlayers as pl, idx (idx)}
            <div class="mode-row">
              <input type="text" maxlength="24" placeholder="Имя игрока" bind:value={pl.name}>
              <input type="text" maxlength="24" placeholder="Telegram Chat ID (необяз.)" bind:value={pl.chat_id}>
              {#if partyPlayers.length > 3}
                <button type="button" class="ib danger" aria-label="Удалить игрока" onclick={() => { partyPlayers = partyPlayers.filter((_, i) => i !== idx); }}>
                  {@html icon("x")}
                </button>
              {/if}
            </div>
          {/each}
        </div>

        <div class="mode-foot">
          <button type="button" class="btn sm ghost" onclick={() => { partyPlayers = [...partyPlayers, { name: `Игрок ${partyPlayers.length + 1}`, chat_id: "" }]; }}>
            {@html icon("plus")} Добавить игрока
          </button>
          <button type="submit" class="btn primary" disabled={busy}>
            {@html icon("play", "solid")} Раздать роли
          </button>
        </div>
      </form>
    </section>
  {:else}
    <section class="card">
      <h2>{@html icon("gamepad")} Интерактивные голосовые режимы</h2>
      <div class="quick-grid">
        <button type="button" class="scen-btn" class:on={mode === "rpg"} disabled={busy} onclick={() => toggleMode("rpg")}>
          {@html icon("shield")}
          <span>{mode === "rpg" ? "RPG-спектакль · идёт" : "RPG-спектакль"}</span>
        </button>
        <button type="button" class="scen-btn" class:on={mode === "quiz"} disabled={busy} onclick={() => toggleMode("quiz")}>
          {@html icon("cpu")}
          <span>{mode === "quiz" ? "Викторина · идёт" : "Викторина"}</span>
        </button>
      </div>
    </section>
  {/if}
</div>
