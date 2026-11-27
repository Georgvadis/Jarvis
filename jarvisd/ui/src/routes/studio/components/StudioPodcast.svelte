<script>
  import { onMount } from "svelte";
  import { api, icon, toast, toastProgress, upload } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";
  import { S, speaker } from "#state/panelState.js";

  const controller = new AbortController();
  const { signal } = controller;
  const epDateFmt = new Intl.DateTimeFormat("ru-RU", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

  const MODES = [
    { id: "notebook", label: "NotebookLM", ic: "podcast" },
    { id: "debate", label: "Спор", ic: "sparkles" },
    { id: "bedtime", label: "Сказка", ic: "moon" },
    { id: "translator", label: "Переводчик", ic: "globe" },
  ];

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

  let modeTab = $state("notebook");
  let profiles = $derived($speaker?.profiles || S?.profiles || []);
  let nbUrl = $state("");
  let nbText = $state("");
  let nbFile = $state(null);
  let nbTurns = $state(6);
  let p1 = $state(S.profiles?.[0]?.id || "");
  let p2 = $state(S.profiles?.[1]?.id || "");
  let dragOver = $state(false);

  let debateTopic = $state("");
  let debateRounds = $state(2);
  let bedtimeTopic = $state("");
  let transLang = $state("английский");
  let busy = $state(false);
  let episodes = $state([]);
  let epLoading = $state(true);
  let epBusy = $state("");
  let expandedEpId = $state("");
  let expandedEpData = $state(null);
  let confirmEpId = $state("");

  let activeMode = $derived($activity.modes?.mode || "normal");
  let activeLang = $derived($activity.modes?.lang || "");
  let sameTransLang = $derived(activeMode === "translator" && activeLang === transLang);
  let debateActive = $derived(Boolean($activity.debate_active));

  async function loadEpisodes() {
    epLoading = true;
    try {
      const res = await api("GET", "/api/podcasts", undefined, { signal });
      if (!signal.aborted && Array.isArray(res?.episodes)) episodes = res.episodes;
    } catch {
      /* ignore if jarvis-voice temporarily offline */
    } finally {
      if (!signal.aborted) epLoading = false;
    }
  }

  onMount(() => {
    void loadEpisodes();
    return () => controller.abort();
  });

  async function toggleEpTranscript(id) {
    if (expandedEpId === id) {
      expandedEpId = "";
      expandedEpData = null;
      return;
    }
    epBusy = id;
    try {
      const res = await api("GET", `/api/podcasts/${encodeURIComponent(id)}`, undefined, { signal });
      if (!signal.aborted) {
        expandedEpId = id;
        expandedEpData = res;
      }
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      epBusy = "";
    }
  }

  async function playEpisode(id) {
    if (epBusy) return;
    epBusy = id;
    try {
      await api("POST", `/api/podcasts/${encodeURIComponent(id)}/play`, {}, { signal });
      await refreshActivity();
      toast("Воспроизвожу выпуск на колонке");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      epBusy = "";
    }
  }

  async function sendEpisodeTelegram(ep) {
    if (epBusy) return;
    epBusy = ep.id;
    const prog = toastProgress("Рендерю MP3 и отправляю в Telegram…");
    try {
      await api("POST", `/api/podcasts/${encodeURIComponent(ep.id)}/telegram`, {}, { signal });
      prog.finish("MP3 отправлен в Telegram");
      void loadEpisodes();
    } catch (error) {
      prog.finish(error.message || "Ошибка отправки MP3", true);
    } finally {
      epBusy = "";
    }
  }

  async function saveEpisodeNote(ep) {
    if (epBusy) return;
    epBusy = ep.id;
    try {
      const data = expandedEpData?.id === ep.id
        ? expandedEpData
        : await api("GET", `/api/podcasts/${encodeURIComponent(ep.id)}`, undefined, { signal });
      const lines = (data.turns || []).map((t) => `**${t.speaker || "Ведущий"}:** ${t.text}`);
      await api("POST", "/api/notes/save", {
        title: ep.title,
        content: lines.join("\n\n"),
        tags: ["подкаст"],
        folder: "Подкасты",
      }, { signal });
      toast("Сохранено в CyberNotes");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      epBusy = "";
    }
  }

  async function deleteEpisode(id) {
    confirmEpId = "";
    if (epBusy) return;
    epBusy = id;
    try {
      await api("DELETE", `/api/podcasts/${encodeURIComponent(id)}`, undefined, { signal });
      episodes = episodes.filter((e) => e.id !== id);
      if (expandedEpId === id) {
        expandedEpId = "";
        expandedEpData = null;
      }
      toast("Выпуск удалён");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      epBusy = "";
    }
  }

  function onDropFile(event) {
    event.preventDefault();
    dragOver = false;
    const f = event.dataTransfer?.files?.[0];
    if (f) nbFile = f;
  }

  async function submitNotebook(event) {
    event.preventDefault();
    if (busy) return;
    busy = true;
    try {
      if (nbFile) {
        const qs = new URLSearchParams({
          filename: nbFile.name || "doc.txt",
          turns: String(nbTurns),
          ...(p1 ? { p1_id: p1 } : {}),
          ...(p2 && p2 !== p1 ? { p2_id: p2 } : {}),
        }).toString();
        await upload("POST", "/api/notebook/upload?" + qs, nbFile);
        nbFile = null;
      } else {
        const url = nbUrl.trim();
        const text = nbText.trim();
        if (!url && !text) {
          busy = false;
          return toast("Укажите ссылку, текст или загрузите файл", true);
        }
        await api("POST", "/api/notebook", {
          url,
          text,
          ...(p1 ? { p1_id: p1 } : {}),
          ...(p2 && p2 !== p1 ? { p2_id: p2 } : {}),
          turns: nbTurns,
        });
      }
      nbUrl = "";
      nbText = "";
      await refreshActivity();
      toast("Подкаст запущен");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function submitDebate(event) {
    event.preventDefault();
    if (busy) return;
    const topic = debateTopic.trim() || "Что лучше — кофе или чай?";
    busy = true;
    try {
      await api("POST", "/api/debate", {
        topic,
        rounds: debateRounds,
        ...(p1 ? { p1_id: p1 } : {}),
        ...(p2 && p2 !== p1 ? { p2_id: p2 } : {}),
      });
      debateTopic = "";
      await refreshActivity();
      toast("Мини-спор запущен");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function submitBedtime(event) {
    event.preventDefault();
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/modes", { mode: "bedtime", topic: bedtimeTopic.trim() || "уютный маяк у ночного океана" });
      bedtimeTopic = "";
      await refreshActivity();
      toast("История на ночь запущена");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function toggleTranslator() {
    if (busy) return;
    busy = true;
    const next = activeMode === "translator" && activeLang === transLang ? "off" : "translator";
    try {
      await api("POST", "/api/modes", { mode: next, lang: transLang });
      await refreshActivity();
      toast(next === "off" ? "Переводчик выключен" : "Переводчик включён");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }
</script>

<div class="studio-split">
  <section class="card">
    <div class="card-head">
      <h2>{@html icon("podcast")} Эфирная студия</h2>
      {#if debateActive}
        <span class="badge acc">В эфире</span>
      {:else if activeMode === "translator"}
        <span class="badge ok">Переводчик · {activeLang}</span>
      {/if}
    </div>

    <div class="seg studio-mode-seg" role="tablist" aria-label="Режим эфира">
      {#each MODES as m (m.id)}
        <button type="button" role="tab" aria-selected={modeTab === m.id} class:on={modeTab === m.id} onclick={() => { modeTab = m.id; }}>
          {@html icon(m.ic)}<span>{m.label}</span>
        </button>
      {/each}
    </div>

    {#if modeTab === "notebook"}
      <form class="studio-stack" onsubmit={submitNotebook}>
        <label
          class="dropzone"
          class:over={dragOver}
          ondragover={(e) => { e.preventDefault(); dragOver = true; }}
          ondragleave={() => { dragOver = false; }}
          ondrop={onDropFile}
        >
          {@html icon("upload")}
          <b>{nbFile ? nbFile.name : "PDF, EPUB, FB2 или TXT"}</b>
          <small>{nbFile ? "Нажмите, чтобы заменить" : "Перетащите файл или кликните"}</small>
          <input type="file" accept=".pdf,.epub,.fb2,.txt,.md" hidden onchange={(e) => { nbFile = e.currentTarget.files?.[0] || null; }}>
        </label>

        {#if nbFile}
          <button type="button" class="btn sm ghost" onclick={() => { nbFile = null; }}>{@html icon("x")} Убрать файл</button>
        {/if}

        <div class="f">
          <label for="nb-url">YouTube или ссылка на статью</label>
          <input id="nb-url" type="text" maxlength="500" placeholder="https://…" bind:value={nbUrl} disabled={busy}>
        </div>

        <div class="f">
          <label for="nb-text">Текст или тезисы</label>
          <textarea id="nb-text" rows="3" maxlength="10000" placeholder="О чём поговорить двум ведущим…" bind:value={nbText} disabled={busy}></textarea>
        </div>

        {#if profiles.length >= 2}
          <div class="g2 grid">
            <div class="f">
              <label for="nb-p1">Ведущий 1</label>
              <select id="nb-p1" bind:value={p1} disabled={busy}>
                {#each profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
              </select>
            </div>
            <div class="f">
              <label for="nb-p2">Ведущий 2</label>
              <select id="nb-p2" bind:value={p2} disabled={busy}>
                {#each profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
              </select>
            </div>
          </div>
        {/if}

        <div class="mode-foot">
          <div class="seg sm" role="group" aria-label="Число реплик">
            {#each [4, 6, 8] as t (t)}
              <button type="button" class:on={nbTurns === t} onclick={() => { nbTurns = t; }}>{t} реплик</button>
            {/each}
          </div>
          <button type="submit" class="btn primary" disabled={busy || debateActive}>
            {@html icon("play", "solid")} Обсудить
          </button>
        </div>
      </form>
    {:else if modeTab === "debate"}
      <form class="studio-stack" onsubmit={submitDebate}>
        <div class="f">
          <label for="deb-topic">Тема спора двух ассистентов</label>
          <input id="deb-topic" type="text" maxlength="300" placeholder="Например: Кофе или чай?" bind:value={debateTopic} disabled={busy}>
        </div>
        {#if profiles.length >= 2}
          <div class="g2 grid">
            <div class="f">
              <label for="deb-p1">Участник 1</label>
              <select id="deb-p1" bind:value={p1} disabled={busy}>
                {#each profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
              </select>
            </div>
            <div class="f">
              <label for="deb-p2">Участник 2</label>
              <select id="deb-p2" bind:value={p2} disabled={busy}>
                {#each profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
              </select>
            </div>
          </div>
        {/if}
        <div class="mode-foot">
          <div class="seg sm" role="group" aria-label="Раунды">
            {#each [2, 3, 4] as r (r)}
              <button type="button" class:on={debateRounds === r} onclick={() => { debateRounds = r; }}>{r} раунда</button>
            {/each}
          </div>
          <button type="submit" class="btn primary" disabled={busy || debateActive}>
            {@html icon("play", "solid")} Запустить
          </button>
        </div>
      </form>
    {:else if modeTab === "bedtime"}
      <form class="studio-stack" onsubmit={submitBedtime}>
        <div class="f">
          <label for="bed-topic">Сюжет сказки на ночь</label>
          <input id="bed-topic" type="text" maxlength="200" placeholder="Уютный маяк у ночного океана" bind:value={bedtimeTopic} disabled={busy}>
        </div>
        <div class="mode-foot">
          <span class="meta">Спокойный голос, мягкий темп</span>
          <button type="submit" class="btn primary" disabled={busy}>{@html icon("moon")} Слушать</button>
        </div>
      </form>
    {:else}
      <div class="studio-stack">
        <div class="f">
          <label for="tr-lang">Язык синхронного перевода</label>
          <select id="tr-lang" bind:value={transLang} disabled={busy}>
            {#each languages as [val, label] (val)}<option value={val}>{label}</option>{/each}
          </select>
        </div>
        <div class="mode-foot">
          <span class="meta">{activeMode === "translator" ? `Активен: ${activeLang}` : "Говорите фразы — колонка переведёт"}</span>
          <button type="button" class="btn" class:primary={!sameTransLang} disabled={busy} onclick={toggleTranslator}>
            {@html icon(sameTransLang ? "stop" : "play", "solid")}
            {sameTransLang ? "Выключить" : activeMode === "translator" ? "Переключить язык" : "Включить"}
          </button>
        </div>
      </div>
    {/if}
  </section>

  <section class="card">
    <div class="card-head">
      <h2>{@html icon("archive")} Архив выпусков и MP3</h2>
      <button type="button" class="btn sm ghost" disabled={epLoading} onclick={loadEpisodes}>
        {@html icon("refresh")} Обновить
      </button>
    </div>
    {#if epLoading}
      <p class="empty">Загружаю архив…</p>
    {:else if !episodes.length}
      <p class="empty">Готовые подкасты и споры автоматически сохраняются здесь.</p>
    {:else}
      <div class="pod-list">
        {#each episodes as ep (ep.id)}
          <article class="pod-row">
            <div class="pod-head">
              <div class="pod-meta">
                <b>{ep.title}</b>
                <small>
                  {ep.kind === "debate" ? "Спор" : "Подкаст"} · {ep.turns_count} репл.
                  {#if ep.created_at} · {epDateFmt.format(new Date(ep.created_at * 1000))}{/if}
                  {#if ep.has_mp3} · <span class="badge ok">MP3</span>{/if}
                </small>
              </div>
              <div class="dj-actions">
                <button type="button" class="ib" title="Прослушать на колонке" aria-label="Прослушать на колонке" disabled={debateActive || epBusy === ep.id} onclick={() => playEpisode(ep.id)}>
                  {@html icon("play", "solid")}
                </button>
                <button type="button" class="ib" class:on={expandedEpId === ep.id} title="Текст выпуска" aria-label="Текст выпуска" disabled={epBusy === ep.id} onclick={() => toggleEpTranscript(ep.id)}>
                  {@html icon("eye")}
                </button>
                <a class="ib" href={`/api/podcasts/${encodeURIComponent(ep.id)}/mp3`} download={`podcast-${ep.id}.mp3`} title="Скачать MP3" aria-label="Скачать MP3">
                  {@html icon("download")}
                </a>
                <button type="button" class="ib" title="Отправить MP3 в Telegram" aria-label="Отправить MP3 в Telegram" disabled={epBusy === ep.id} onclick={() => sendEpisodeTelegram(ep)}>
                  {@html icon("send")}
                </button>
                <button type="button" class="ib" title="Сохранить в CyberNotes" aria-label="Сохранить в CyberNotes" disabled={epBusy === ep.id} onclick={() => saveEpisodeNote(ep)}>
                  {@html icon("note")}
                </button>
                {#if confirmEpId === ep.id}
                  <button type="button" class="ib danger" aria-label="Подтвердить удаление" onclick={() => deleteEpisode(ep.id)}>{@html icon("check")}</button>
                  <button type="button" class="ib" aria-label="Отмена" onclick={() => { confirmEpId = ""; }}>{@html icon("x")}</button>
                {:else}
                  <button type="button" class="ib danger" title="Удалить выпуск" aria-label="Удалить выпуск" onclick={() => { confirmEpId = ep.id; }}>{@html icon("trash")}</button>
                {/if}
              </div>
            </div>
            {#if expandedEpId === ep.id && expandedEpData?.turns?.length}
              <div class="pod-turns">
                {#each expandedEpData.turns as t, idx (idx)}
                  <p><b>{t.speaker || "Ведущий"}:</b> {t.text}</p>
                {/each}
              </div>
            {/if}
          </article>
        {/each}
      </div>
    {/if}
  </section>
</div>
