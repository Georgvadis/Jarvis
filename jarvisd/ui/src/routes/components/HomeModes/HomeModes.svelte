<script>
  import { onMount } from "svelte";
  import { api, icon, toast, upload } from "#helpers/core.js";
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
  const djPresets = ["Ночной Токио", "Синтвейв", "Аниме OST", "Лоу-фай"];
  const melodyThemes = ["Аниме-опенинги", "Хиты 2000-х", "Рок", "Игры"];

  let tab = $state("modes"); // "modes" | "games" | "books"
  let focusRunning = $state(false);
  let focusRem = $state(0);
  let activeMode = $state("normal");
  let activeLang = $state("");
  let drawer = $state(""); // "" | "debate" | "notebook" | "dj" | "translator" | "bedtime" | "melody" | "party" | "book_url"
  let busy = $state(false);

  // Debate & NotebookLM
  let debateTopic = $state("");
  let debateP1 = $state("");
  let debateP2 = $state("");
  let debateRounds = $state(2);
  let nbUrl = $state("");
  let nbText = $state("");
  let nbFile = $state(null);

  // Translator / Bedtime
  let transLang = $state("английский");
  let bedtimeTopic = $state("");

  // AI-DJ
  let djMood = $state("ночной Токио синтвейв");
  let djState = $state({ active: false, station: "", current_title: "", mood: "" });

  // Games
  let gamesState = $state({ active_game: "", melody: {}, detective: {}, party: {} });
  let melodyTheme = $state("Аниме-опенинги");
  let partyGame = $state("mafia");
  let partyDiscussion = $state(60);
  let partyPlayers = $state([
    { name: "Игрок 1", chat_id: "" },
    { name: "Игрок 2", chat_id: "" },
    { name: "Игрок 3", chat_id: "" },
    { name: "Игрок 4", chat_id: "" },
  ]);
  let revealedRoles = $state({});

  // Books Reader
  let books = $state([]);
  let readerState = $state({ running: false, book_id: "", title: "", chapter: 0, chapter_title: "", chapters_count: 0, multi_voice: false });
  let multiVoice = $state(false);
  let bookUrl = $state("");
  let bookUrlTitle = $state("");
  let confirmBookId = $state("");

  let tickTimer;
  let syncTimer;
  let profiles = $derived(S.profiles || []);
  let anyActive = $derived(
    focusRunning || activeMode !== "normal" || djState.active || readerState.running || !!gamesState.active_game
  );

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
      const [f, m, g, d, b] = await Promise.all([
        api("GET", "/api/focus", undefined, { signal }),
        api("GET", "/api/modes", undefined, { signal }),
        api("GET", "/api/games", undefined, { signal }),
        api("GET", "/api/dj", undefined, { signal }),
        api("GET", "/api/books", undefined, { signal }),
      ]);
      if (signal.aborted) return;
      focusRunning = !!f.running;
      focusRem = Number(f.remaining_s || 0);
      activeMode = m.mode || "normal";
      activeLang = m.lang || "";
      if (activeLang) transLang = activeLang;
      gamesState = g || gamesState;
      djState = d || djState;
      if (Array.isArray(b?.books)) books = b.books;
      if (b?.reader) {
        readerState = b.reader;
        if (b.reader.running) multiVoice = !!b.reader.multi_voice;
      }
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

  async function stopAll() {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/stop", {}, { signal });
      if (signal.aborted) return;
      await loadStatus();
      toast("Остановлено");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  function toggleDrawer(name) {
    if (drawer === name) {
      drawer = "";
      return;
    }
    if ((name === "debate" || name === "notebook") && !debateP1 && profiles[0]) debateP1 = profiles[0].id;
    if ((name === "debate" || name === "notebook") && !debateP2 && profiles[1]) debateP2 = profiles[1].id;
    drawer = name;
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
      toast("Мини-спор запущен");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function submitNotebook(event) {
    event.preventDefault();
    if (busy) return;
    busy = true;
    try {
      if (nbFile) {
        const qs = new URLSearchParams({
          filename: nbFile.name || "doc.txt",
          ...(debateP1 ? { p1_id: debateP1 } : {}),
          ...(debateP2 && debateP2 !== debateP1 ? { p2_id: debateP2 } : {}),
        }).toString();
        await upload("POST", "/api/notebook/upload?" + qs, nbFile, { signal });
        nbFile = null;
      } else {
        const url = nbUrl.trim();
        const text = nbText.trim();
        if (!url && !text) {
          busy = false;
          return toast("Укажите URL, текст или файл", true);
        }
        await api("POST", "/api/notebook", {
          url,
          text,
          ...(debateP1 ? { p1_id: debateP1 } : {}),
          ...(debateP2 && debateP2 !== debateP1 ? { p2_id: debateP2 } : {}),
          turns: 6,
        }, { signal });
      }
      if (signal.aborted) return;
      drawer = "";
      nbUrl = "";
      nbText = "";
      toast("Подкаст запущен");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function startDj(mood = djMood) {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", "/api/dj/start", { mood: mood.trim() || "ночной Токио синтвейв" }, { signal });
      if (signal.aborted) return;
      djState = res;
      drawer = "";
      toast("AI-DJ запущен");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function djCmd(action) {
    try {
      const res = await api("POST", `/api/dj/${action}`, {}, { signal });
      if (signal.aborted) return;
      djState = res.dj || res;
      if (res.message) toast(res.message);
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
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

  async function startMelody(theme = melodyTheme) {
    if (busy) return;
    busy = true;
    try {
      gamesState = await api("POST", "/api/games/melody", { theme }, { signal });
      drawer = "";
      toast("Угадай мелодию запущена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function startDetective() {
    if (busy) return;
    busy = true;
    try {
      gamesState = await api("POST", "/api/games/detective", {}, { signal });
      drawer = "";
      toast("Нуарный детектив начат");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function startParty(event) {
    event.preventDefault();
    if (busy) return;
    const players = partyPlayers
      .map((p) => ({ name: p.name.trim(), chat_id: p.chat_id.trim() }))
      .filter((p) => p.name);
    if (players.length < 3) return toast("Нужно минимум 3 игрока", true);
    busy = true;
    try {
      revealedRoles = {};
      gamesState = await api("POST", "/api/games/party/start", {
        game: partyGame,
        discussion_s: partyDiscussion,
        players,
      }, { signal });
      drawer = "";
      toast("Роли розданы");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function nextPartyPhase() {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", "/api/games/party/next", {}, { signal });
      if (res.games) gamesState = res.games;
      toast("Следующая фаза");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function handleBookUpload(event) {
    const file = event.currentTarget.files?.[0];
    if (!file || busy) return;
    busy = true;
    try {
      const qs = new URLSearchParams({ filename: file.name || "book.txt" }).toString();
      await upload("POST", "/api/books/upload?" + qs, file, { signal });
      event.currentTarget.value = "";
      await loadStatus();
      toast("Книга добавлена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function submitBookUrl(event) {
    event.preventDefault();
    if (!bookUrl.trim() || busy) return;
    busy = true;
    try {
      await api("POST", "/api/books/url", { url: bookUrl.trim(), title: bookUrlTitle.trim() }, { signal });
      bookUrl = "";
      bookUrlTitle = "";
      drawer = "";
      await loadStatus();
      toast("Глава добавлена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function playBook(bookId) {
    if (busy) return;
    busy = true;
    try {
      readerState = await api("POST", "/api/books/play", { book_id: bookId, multi_voice: multiVoice }, { signal });
      toast("Читаю с закладки");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function pauseBook() {
    readerState = await api("POST", "/api/books/stop", {}, { signal });
    toast("Книга на паузе");
  }

  async function navBook(delta) {
    try {
      const res = await api("POST", "/api/books/nav", { delta }, { signal });
      if (res.reader) readerState = res.reader;
      toast(res.message || "Глава переключена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    }
  }

  async function recapBook() {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", "/api/books/recap", {}, { signal });
      toast(res.text || "Пересказ готов");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function deleteBook(id) {
    confirmBookId = "";
    try {
      await api("DELETE", "/api/books/" + encodeURIComponent(id), undefined, { signal });
      books = books.filter((b) => b.id !== id);
      toast("Книга удалена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
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
    <div class="seg sm mode-tabs" role="tablist" aria-label="Раздел">
      <button type="button" class:on={tab === "modes"} onclick={() => { tab = "modes"; drawer = ""; }}>
        {@html icon("sparkles")} Режимы
      </button>
      <button type="button" class:on={tab === "games"} onclick={() => { tab = "games"; drawer = ""; }}>
        {@html icon("gamepad")} Игры
      </button>
      <button type="button" class:on={tab === "books"} onclick={() => { tab = "books"; drawer = ""; }}>
        {@html icon("book")} Книги
      </button>
    </div>
    {#if anyActive}
      <button class="btn sm ghost amb-stop" type="button" disabled={busy} onclick={stopAll}>
        {@html icon("stop", "solid")} Стоп
      </button>
    {/if}
  </h2>

  {#if tab === "modes"}
    {#if djState.active}
      <div class="dj-bar">
        <div class="dj-info">
          <b>{@html icon("disc")} {djState.station || "AI-DJ"}</b>
          <small>{djState.current_title || djState.mood}</small>
        </div>
        <div class="dj-actions">
          <button type="button" class="ib" title="Лайк" aria-label="Лайк треку" onclick={() => djCmd("like")}>{@html icon("heart")}</button>
          <button type="button" class="ib" title="Следующий" aria-label="Следующий трек" onclick={() => djCmd("next")}>{@html icon("skip")}</button>
          <button type="button" class="ib danger" title="Стоп DJ" aria-label="Остановить DJ" onclick={() => djCmd("stop")}>{@html icon("stop", "solid")}</button>
        </div>
      </div>
    {/if}
    <div class="amb-grid">
      <button type="button" class="amb-btn" class:on={focusRunning} disabled={busy} onclick={toggleFocus}>
        {@html icon("bolt")}
        <span>{focusRunning ? `Фокус ${fmt(focusRem)}` : "Фокус 25м"}</span>
      </button>
      <button type="button" class="amb-btn" class:on={drawer === "notebook"} disabled={busy} onclick={() => toggleDrawer("notebook")}>
        {@html icon("podcast")}
        <span>Подкаст URL</span>
      </button>
      <button type="button" class="amb-btn" class:on={djState.active || drawer === "dj"} disabled={busy} onclick={() => toggleDrawer("dj")}>
        {@html icon("disc")}
        <span>AI-DJ</span>
      </button>
      <button type="button" class="amb-btn" class:on={drawer === "debate"} disabled={busy} onclick={() => toggleDrawer("debate")}>
        {@html icon("sparkles")}
        <span>Мини-спор</span>
      </button>
      <button type="button" class="amb-btn" class:on={activeMode === "translator" || drawer === "translator"} disabled={busy} onclick={() => toggleDrawer("translator")}>
        {@html icon("globe")}
        <span>{activeMode === "translator" && activeLang ? `Перевод (${activeLang})` : "Переводчик"}</span>
      </button>
      <button type="button" class="amb-btn" class:on={drawer === "bedtime"} disabled={busy} onclick={() => toggleDrawer("bedtime")}>
        {@html icon("moon")}
        <span>На ночь</span>
      </button>
    </div>

    {#if drawer === "notebook"}
      <form class="mode-drawer" onsubmit={submitNotebook}>
        <input type="text" maxlength="500" placeholder="Ссылка на YouTube или статью…" bind:value={nbUrl} disabled={busy}>
        <textarea rows="2" maxlength="6000" placeholder="Или вставьте текст / загрузите PDF, EPUB, FB2…" bind:value={nbText} disabled={busy}></textarea>
        {#if profiles.length >= 2}
          <div class="mode-row">
            <select aria-label="Ведущий 1" bind:value={debateP1} disabled={busy}>
              {#each profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
            </select>
            <select aria-label="Ведущий 2" bind:value={debateP2} disabled={busy}>
              {#each profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
            </select>
          </div>
        {/if}
        <div class="mode-foot">
          <label class="btn sm ghost file-btn">
            {@html icon("upload")} {nbFile ? nbFile.name.slice(0, 14) : "PDF/EPUB"}
            <input type="file" accept=".pdf,.epub,.fb2,.txt,.md" hidden onchange={(e) => { nbFile = e.currentTarget.files?.[0] || null; }}>
          </label>
          <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("play", "solid")}Обсудить</button>
        </div>
      </form>
    {:else if drawer === "dj"}
      <form class="mode-drawer" onsubmit={(e) => { e.preventDefault(); startDj(djMood); }}>
        <div class="chip-row">
          {#each djPresets as preset (preset)}
            <button type="button" class="btn sm ghost" onclick={() => { djMood = preset; startDj(preset); }}>{preset}</button>
          {/each}
        </div>
        <div class="mode-row">
          <input type="text" maxlength="140" placeholder="Опишите вайб…" bind:value={djMood} disabled={busy}>
          <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("play", "solid")}Эфир</button>
        </div>
      </form>
    {:else if drawer === "debate"}
      <form class="mode-drawer" onsubmit={submitDebate}>
        <input type="text" maxlength="300" placeholder="Тема спора, например: Кофе или чай?" bind:value={debateTopic} disabled={busy}>
        {#if profiles.length >= 2}
          <div class="mode-row">
            <select aria-label="Участник 1" bind:value={debateP1} disabled={busy}>
              {#each profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
            </select>
            <select aria-label="Участник 2" bind:value={debateP2} disabled={busy}>
              {#each profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
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
            {#each languages as [val, label] (val)}<option value={val}>{label}</option>{/each}
          </select>
          <button type="button" class="btn sm primary" disabled={busy} onclick={() => toggleMode("translator", transLang)}>
            {@html icon(activeMode === "translator" && activeLang === transLang ? "stop" : "play", "solid")}
            {activeMode === "translator" && activeLang === transLang ? "Выкл" : "Вкл"}
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

  {:else if tab === "games"}
    {#if gamesState.active_game === "melody"}
      <div class="dj-bar">
        <div class="dj-info">
          <b>Угадай мелодию: {gamesState.melody.theme}</b>
          <small>Раунд {gamesState.melody.round}/{gamesState.melody.total} · Счёт {gamesState.melody.score}</small>
        </div>
      </div>
    {:else if gamesState.active_game === "detective"}
      <div class="dj-bar">
        <div class="dj-info">
          <b>Дело: {gamesState.detective.title}</b>
          <small>Улики: {gamesState.detective.clues_shown}/{gamesState.detective.clues_total}</small>
        </div>
      </div>
    {:else if gamesState.active_game === "party"}
      <div class="dj-bar">
        <div class="dj-info">
          <b>{gamesState.party.game === "bunker" ? "Бункер" : "Мафия"} · Раунд {gamesState.party.round}</b>
          <small>{gamesState.party.phase}</small>
        </div>
        <button type="button" class="btn sm primary" disabled={busy} onclick={nextPartyPhase}>
          {@html icon("skip")} Фаза
        </button>
      </div>
    {/if}

    <div class="amb-grid">
      <button type="button" class="amb-btn" class:on={gamesState.active_game === "melody" || drawer === "melody"} disabled={busy} onclick={() => toggleDrawer("melody")}>
        {@html icon("music")}
        <span>Угадай мелодию</span>
      </button>
      <button type="button" class="amb-btn" class:on={gamesState.active_game === "detective"} disabled={busy} onclick={startDetective}>
        {@html icon("eye")}
        <span>Нуар-детектив</span>
      </button>
      <button type="button" class="amb-btn" class:on={gamesState.active_game === "party" || drawer === "party"} disabled={busy} onclick={() => toggleDrawer("party")}>
        {@html icon("users")}
        <span>Мафия / Бункер</span>
      </button>
      <button type="button" class="amb-btn" class:on={activeMode === "rpg"} disabled={busy} onclick={() => toggleMode("rpg")}>
        {@html icon("shield")}
        <span>RPG-спектакль</span>
      </button>
      <button type="button" class="amb-btn" class:on={activeMode === "quiz"} disabled={busy} onclick={() => toggleMode("quiz")}>
        {@html icon("cpu")}
        <span>Викторина</span>
      </button>
    </div>

    {#if gamesState.active_game === "party" && gamesState.party.roles?.length}
      <div class="party-roles">
        {#each gamesState.party.roles as r (r.name)}
          <button type="button" class="role-card" onclick={() => { revealedRoles = { ...revealedRoles, [r.name]: !revealedRoles[r.name] }; }}>
            <b>{r.name}</b>
            {#if r.dm_sent}
              <small>В Telegram</small>
            {:else if revealedRoles[r.name]}
              <small>{r.role}</small>
            {:else}
              <small>Показать</small>
            {/if}
          </button>
        {/each}
      </div>
    {/if}

    {#if drawer === "melody"}
      <form class="mode-drawer" onsubmit={(e) => { e.preventDefault(); startMelody(melodyTheme); }}>
        <div class="chip-row">
          {#each melodyThemes as theme (theme)}
            <button type="button" class="btn sm ghost" onclick={() => { melodyTheme = theme; startMelody(theme); }}>{theme}</button>
          {/each}
        </div>
        <div class="mode-row">
          <input type="text" maxlength="100" placeholder="Любая тема…" bind:value={melodyTheme} disabled={busy}>
          <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("play", "solid")}Старт</button>
        </div>
      </form>
    {:else if drawer === "party"}
      <form class="mode-drawer" onsubmit={startParty}>
        <div class="mode-foot">
          <div class="seg sm" role="group">
            <button type="button" class:on={partyGame === "mafia"} onclick={() => { partyGame = "mafia"; }}>Мафия</button>
            <button type="button" class:on={partyGame === "bunker"} onclick={() => { partyGame = "bunker"; }}>Бункер</button>
          </div>
          <div class="seg sm" role="group">
            {#each [60, 90, 120] as sec (sec)}
              <button type="button" class:on={partyDiscussion === sec} onclick={() => { partyDiscussion = sec; }}>{sec}с</button>
            {/each}
          </div>
        </div>
        <div class="party-list">
          {#each partyPlayers as pl, idx (idx)}
            <div class="mode-row">
              <input type="text" maxlength="24" placeholder="Имя" bind:value={pl.name}>
              <input type="text" maxlength="24" placeholder="TG ID (опц.)" bind:value={pl.chat_id}>
              {#if partyPlayers.length > 3}
                <button type="button" class="ib danger" aria-label="Удалить игрока" onclick={() => { partyPlayers = partyPlayers.filter((_, i) => i !== idx); }}>{@html icon("x")}</button>
              {/if}
            </div>
          {/each}
        </div>
        <div class="mode-foot">
          <button type="button" class="btn sm ghost" onclick={() => { partyPlayers = [...partyPlayers, { name: `Игрок ${partyPlayers.length + 1}`, chat_id: "" }]; }}>
            {@html icon("plus")} Игрок
          </button>
          <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("play", "solid")}Раздать</button>
        </div>
      </form>
    {/if}

  {:else if tab === "books"}
    <div class="mode-foot">
      <div class="chip-row">
        <label class="btn sm primary file-btn">
          {@html icon("upload")} Файл
          <input type="file" accept=".epub,.fb2,.zip,.txt,.md,.pdf" hidden onchange={handleBookUpload}>
        </label>
        <button type="button" class="btn sm ghost" onclick={() => toggleDrawer("book_url")}>{@html icon("link")} URL</button>
        <button type="button" class="btn sm" class:primary={multiVoice} onclick={() => { multiVoice = !multiVoice; }}>
          {@html icon("users")} Голоса
        </button>
      </div>
      {#if books.length}
        <button type="button" class="btn sm ghost" disabled={busy} onclick={recapBook}>{@html icon("refresh")} Суть</button>
      {/if}
    </div>

    {#if drawer === "book_url"}
      <form class="mode-drawer" onsubmit={submitBookUrl}>
        <input type="text" maxlength="400" placeholder="Ссылка на главу или текст…" bind:value={bookUrl} disabled={busy}>
        <div class="mode-row">
          <input type="text" maxlength="80" placeholder="Название (необяз.)" bind:value={bookUrlTitle} disabled={busy}>
          <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("plus")} Добавить</button>
        </div>
      </form>
    {/if}

    {#if readerState.running}
      <div class="dj-bar">
        <div class="dj-info">
          <b>{readerState.title}</b>
          <small>{readerState.chapter_title || `Глава ${readerState.chapter + 1}`}</small>
        </div>
        <div class="dj-actions">
          <button type="button" class="ib" title="Пред. глава" aria-label="Предыдущая глава" onclick={() => navBook(-1)}>{@html icon("prev")}</button>
          <button type="button" class="ib" title="Пауза" aria-label="Пауза книги" onclick={pauseBook}>{@html icon("pause")}</button>
          <button type="button" class="ib" title="След. глава" aria-label="Следующая глава" onclick={() => navBook(1)}>{@html icon("skip")}</button>
        </div>
      </div>
    {/if}

    <div class="book-list">
      {#if !books.length}
        <p class="note">Загрузите EPUB, FB2, TXT, PDF или вставьте ссылку на главу.</p>
      {:else}
        {#each books as b (b.id)}
          <div class="book-row" class:on={readerState.running && readerState.book_id === b.id}>
            <div class="book-meta">
              <b>{b.title}</b>
              <small>Гл. {b.chapter + 1}/{b.chapters_count} · {b.progress_pct}%</small>
              <div class="book-prog"><i style:width={`${b.progress_pct}%`}></i></div>
            </div>
            <div class="dj-actions">
              {#if readerState.running && readerState.book_id === b.id}
                <button type="button" class="ib" title="Пауза" aria-label="Пауза" onclick={pauseBook}>{@html icon("pause")}</button>
              {:else}
                <button type="button" class="ib" title="Слушать" aria-label="Слушать книгу" onclick={() => playBook(b.id)}>{@html icon("play", "solid")}</button>
              {/if}
              {#if confirmBookId === b.id}
                <button type="button" class="ib danger" aria-label="Подтвердить удаление" onclick={() => deleteBook(b.id)}>{@html icon("check")}</button>
                <button type="button" class="ib" aria-label="Отмена" onclick={() => { confirmBookId = ""; }}>{@html icon("x")}</button>
              {:else}
                <button type="button" class="ib danger" title="Удалить" aria-label="Удалить книгу" onclick={() => { confirmBookId = b.id; }}>{@html icon("trash")}</button>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
    </div>
  {/if}
</section>
