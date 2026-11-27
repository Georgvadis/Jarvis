<script>
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";

  const PARTY_STORAGE = "jarvis.partyPlayers";
  const melodyThemes = ["Аниме-опенинги", "Хиты 2000-х", "Рок", "Игры", "Саундтреки кино", "Ретро 80–90-х"];
  const melodyRoundsList = [3, 5, 8];
  const melodySnippetList = [5, 8, 12, 15];

  const rpgPresets = ["Тёмное фэнтези", "Киберпанк 2099", "Космоопера", "Выживание в бункере", "Мистика Японии"];
  const quizPresets = ["Обо всём", "Аниме и манга", "Кино и сериалы", "Игры", "Наука и космос", "История"];
  const detectivePresets = ["Особняк 1940-х", "Поезд в метель", "Неоновый Токио", "Космическая станция"];

  const customTemplates = [
    {
      title: "Данетки",
      rules: "Загадай короткую загадочную ситуацию (данетку). Игрок задаёт вопросы, на которые ты отвечаешь только «да», «нет» или «не важно», пока он не разгадает всю историю.",
    },
    {
      title: "Акинатор наоборот",
      rules: "Игрок загадывает персонажа из аниме, кино или игр, а ты задаёшь по одному наводящему вопросу (да/нет) и пытаешься угадать за 10 вопросов.",
    },
    {
      title: "Суд присяжных",
      rules: "Придумай абсурдное и смешное дело (например, кот украл сосиску и нанял адвоката). Игрок — судья или адвокат, ты ведёшь процесс, опрашиваешь свидетелей и реагируешь на его аргументы.",
    },
    {
      title: "Интервью с пришельцем",
      rules: "Ты пришелец с далёкой планеты, который впервые прилетел на Землю и ничего не понимает в обычных человеческих вещах. Игрок пытается объяснить тебе, как устроен мир.",
    },
  ];

  const GAME_CARDS = [
    { id: "melody", title: "Угадай мелодию", hint: "Отрывки · счёт", ic: "music" },
    { id: "rpg", title: "RPG-квест", hint: "Любой сеттинг", ic: "shield" },
    { id: "quiz", title: "Викторина", hint: "Вопросы по теме", ic: "sparkles" },
    { id: "detective", title: "Нуар-детектив", hint: "Улики и допрос", ic: "eye" },
    { id: "custom", title: "Своя игра", hint: "Конструктор + ИИ", ic: "wand" },
    { id: "party", title: "Мафия / Бункер", hint: "Компания 3+", ic: "users" },
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
  let melodyRounds = $state(5);
  let melodySnippet = $state(8);
  let melodyFromStart = $state(false);
  let rpgTopic = $state("Тёмное фэнтези");
  let quizTopic = $state("Обо всём");
  let detectiveTheme = $state("");
  let customTitle = $state("");
  let customRules = $state("");
  let partyGame = $state("mafia");
  let partyDiscussion = $state(60);
  let partyPlayers = $state(loadPlayers());
  let revealedRoles = $state({});
  let busy = $state(false);
  let ideaBusy = $state(false);

  $effect(() => {
    try {
      localStorage.setItem(PARTY_STORAGE, JSON.stringify(partyPlayers));
    } catch { /* ignore */ }
  });

  let games = $derived($activity.games || { active_game: "", melody: {}, detective: {}, party: {} });
  let mode = $derived($activity.modes?.mode || "normal");
  let modeTopic = $derived($activity.modes?.topic || "");

  function isLive(id) {
    if (id === "rpg") return mode === "rpg";
    if (id === "quiz") return mode === "quiz";
    if (id === "custom") return mode === "custom";
    return games.active_game === id;
  }

  async function generateIdea(kind) {
    if (ideaBusy || busy) return;
    ideaBusy = true;
    try {
      const res = await api("POST", "/api/games/idea", { kind });
      if (kind === "melody" && res.theme) melodyTheme = res.theme;
      else if (kind === "rpg" && res.theme) rpgTopic = res.theme;
      else if (kind === "quiz" && res.theme) quizTopic = res.theme;
      else if (kind === "detective" && res.theme) detectiveTheme = res.theme;
      else if (kind === "custom") {
        if (res.title) customTitle = res.title;
        if (res.rules) customRules = res.rules;
      }
      toast("Идея придумана");
    } catch (error) {
      toast(error.message, true);
    } finally {
      ideaBusy = false;
    }
  }

  async function startMelody(theme = melodyTheme) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/games/melody", {
        theme,
        rounds: melodyRounds,
        snippet_s: melodySnippet,
        from_start: melodyFromStart,
      });
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

  async function startMode(target, payload = {}) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/modes", { mode: target, ...payload });
      await refreshActivity();
      toast(target === "off" ? "Режим выключен" : "Игра запущена на колонке");
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
      await refreshActivity();
      toast("Нуарный детектив начат");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function startCustomGame(event) {
    event?.preventDefault();
    if (busy) return;
    const prompt = customRules.trim();
    if (!prompt) return toast("Опишите правила или сгенерируйте идею", true);
    await startMode("custom", { title: customTitle.trim() || "Своя игра", prompt });
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

  async function stopGames() {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/games/stop", {});
      await api("POST", "/api/modes", { mode: "off" }).catch(() => {});
      await refreshActivity();
      toast("Игра остановлена");
    } catch (error) {
      toast(error.message || "Не удалось остановить игру", true);
    } finally {
      busy = false;
    }
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
          <small>{isLive(card.id) ? "Идёт сейчас" : card.hint}</small>
        </span>
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
            <button type="button" class="btn sm" class:primary={melodyTheme === theme} disabled={busy}
              onclick={() => { melodyTheme = theme; }}>{theme}</button>
          {/each}
        </div>
        <div class="grid g2">
          <div class="f">
            <span class="lbl">Раундов</span>
            <div class="seg" role="group" aria-label="Количество раундов">
              {#each melodyRoundsList as n (n)}
                <button type="button" class:on={melodyRounds === n} disabled={busy} onclick={() => { melodyRounds = n; }}>{n}</button>
              {/each}
            </div>
          </div>
          <div class="f">
            <span class="lbl">Длина отрывка</span>
            <div class="seg" role="group" aria-label="Длина отрывка">
              {#each melodySnippetList as s (s)}
                <button type="button" class:on={melodySnippet === s} disabled={busy} onclick={() => { melodySnippet = s; }}>{s} с</button>
              {/each}
            </div>
          </div>
        </div>
        <div class="f">
          <span class="lbl">Откуда включать</span>
          <div class="seg" role="group" aria-label="Позиция старта трека">
            <button type="button" class:on={!melodyFromStart} disabled={busy} onclick={() => { melodyFromStart = false; }}>С середины (30 с)</button>
            <button type="button" class:on={melodyFromStart} disabled={busy} onclick={() => { melodyFromStart = true; }}>С начала</button>
          </div>
        </div>
        <div class="mode-row">
          <input type="text" maxlength="100" placeholder="Любая тема…" bind:value={melodyTheme} disabled={busy}>
          <button type="button" class="btn" disabled={busy || ideaBusy} onclick={() => generateIdea("melody")} data-tip="Придумать случайную тему">
            {@html icon("sparkles")} {ideaBusy ? "…" : "Идея"}
          </button>
          <button type="submit" class="btn primary" disabled={busy}>{@html icon("play", "solid")} Старт</button>
        </div>
      </form>
    </section>

  {:else if selectedGame === "rpg"}
    <section class="card">
      <div class="card-head">
        <h2>{@html icon("shield")} RPG-квест</h2>
        {#if mode === "rpg"}
          <div class="actions">
            <span class="badge acc">{modeTopic || "Идёт квест"}</span>
            <button type="button" class="btn sm ghost danger" onclick={stopGames}>{@html icon("stop", "solid")} Завершить</button>
          </div>
        {/if}
      </div>
      <form class="studio-stack" onsubmit={(e) => { e.preventDefault(); startMode("rpg", { topic: rpgTopic.trim() }); }}>
        <div class="chip-row">
          {#each rpgPresets as preset (preset)}
            <button type="button" class="btn sm" class:primary={rpgTopic === preset} disabled={busy}
              onclick={() => { rpgTopic = preset; }}>{preset}</button>
          {/each}
        </div>
        <div class="mode-row">
          <input type="text" maxlength="120" placeholder="Свой сеттинг (например: нуарный Токио 2080)…" bind:value={rpgTopic} disabled={busy}>
          <button type="button" class="btn" disabled={busy || ideaBusy} onclick={() => generateIdea("rpg")} data-tip="Придумать сеттинг">
            {@html icon("sparkles")} {ideaBusy ? "…" : "Идея"}
          </button>
          <button type="submit" class="btn primary" disabled={busy}>{@html icon("play", "solid")} Старт</button>
        </div>
      </form>
    </section>

  {:else if selectedGame === "quiz"}
    <section class="card">
      <div class="card-head">
        <h2>{@html icon("sparkles")} Викторина</h2>
        {#if mode === "quiz"}
          <div class="actions">
            <span class="badge acc">{modeTopic || "Идёт викторина"}</span>
            <button type="button" class="btn sm ghost danger" onclick={stopGames}>{@html icon("stop", "solid")} Завершить</button>
          </div>
        {/if}
      </div>
      <form class="studio-stack" onsubmit={(e) => { e.preventDefault(); startMode("quiz", { topic: quizTopic.trim() === "Обо всём" ? "" : quizTopic.trim() }); }}>
        <div class="chip-row">
          {#each quizPresets as preset (preset)}
            <button type="button" class="btn sm" class:primary={quizTopic === preset} disabled={busy}
              onclick={() => { quizTopic = preset; }}>{preset}</button>
          {/each}
        </div>
        <div class="mode-row">
          <input type="text" maxlength="120" placeholder="Любая тема викторины…" bind:value={quizTopic} disabled={busy}>
          <button type="button" class="btn" disabled={busy || ideaBusy} onclick={() => generateIdea("quiz")} data-tip="Придумать тему">
            {@html icon("sparkles")} {ideaBusy ? "…" : "Идея"}
          </button>
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
        <div class="chip-row">
          {#each detectivePresets as preset (preset)}
            <button type="button" class="btn sm" class:primary={detectiveTheme === preset} disabled={busy}
              onclick={() => { detectiveTheme = preset; }}>{preset}</button>
          {/each}
        </div>
        <div class="mode-row">
          <input type="text" maxlength="120" placeholder="Сеттинг (например: особняк 1940-х)" bind:value={detectiveTheme} disabled={busy}>
          <button type="button" class="btn" disabled={busy || ideaBusy} onclick={() => generateIdea("detective")} data-tip="Придумать дело">
            {@html icon("sparkles")} {ideaBusy ? "…" : "Идея"}
          </button>
          <button type="submit" class="btn primary" disabled={busy}>{@html icon("play", "solid")} Новое дело</button>
        </div>
      </form>
    </section>

  {:else if selectedGame === "custom"}
    <section class="card">
      <div class="card-head">
        <h2>{@html icon("wand")} Конструктор своей игры</h2>
        {#if mode === "custom"}
          <div class="actions">
            <span class="badge acc">{modeTopic || "Идёт игра"}</span>
            <button type="button" class="btn sm ghost danger" onclick={stopGames}>{@html icon("stop", "solid")} Завершить</button>
          </div>
        {/if}
      </div>
      <form class="studio-stack" onsubmit={startCustomGame}>
        <div class="chip-row">
          {#each customTemplates as tpl (tpl.title)}
            <button type="button" class="btn sm" class:primary={customTitle === tpl.title} disabled={busy}
              onclick={() => { customTitle = tpl.title; customRules = tpl.rules; }}>{tpl.title}</button>
          {/each}
        </div>
        <div class="mode-row">
          <input type="text" maxlength="80" placeholder="Название игры…" bind:value={customTitle} disabled={busy}>
          <button type="button" class="btn" disabled={busy || ideaBusy} onclick={() => generateIdea("custom")}>
            {@html icon("sparkles")} {ideaBusy ? "Придумываю…" : "Придумать идею"}
          </button>
        </div>
        <div class="f">
          <textarea rows="4" maxlength="1000" placeholder="Правила, роль ассистента и сеттинг…" bind:value={customRules} disabled={busy}></textarea>
        </div>
        <div class="row">
          <span class="spacer"></span>
          <button type="submit" class="btn primary" disabled={busy || !customRules.trim()}>
            {@html icon("play", "solid")} Запустить на колонке
          </button>
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
  {/if}
</div>
