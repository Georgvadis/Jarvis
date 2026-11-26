<script>
  import { onMount } from "svelte";
  import { api, copyText, icon, toast } from "#helpers/core.js";

  const controller = new AbortController();
  const { signal } = controller;
  const dateFmt = new Intl.DateTimeFormat("ru-RU", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

  let notes = $state([]);
  let total = $state(0);
  let available = $state(true);
  let query = $state("");
  let loading = $state(true);
  let busy = $state(false);
  let expandedPath = $state("");
  let speakingPath = $state("");

  let title = $state("");
  let tagsInput = $state("");
  let content = $state("");

  async function loadList() {
    loading = true;
    try {
      const res = await api("GET", "/api/notes/list?limit=24", undefined, { signal });
      if (signal.aborted) return;
      available = res.available !== false;
      total = Number(res.total_notes || 0);
      notes = res.recent || [];
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      if (!signal.aborted) loading = false;
    }
  }

  async function search(event) {
    event?.preventDefault();
    const q = query.trim();
    if (!q) return void loadList();
    loading = true;
    try {
      const res = await api("GET", "/api/notes/search?q=" + encodeURIComponent(q) + "&limit=18", undefined, { signal });
      if (signal.aborted) return;
      total = Number(res.total_notes || 0);
      notes = res.hits || [];
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      if (!signal.aborted) loading = false;
    }
  }

  async function saveNote(event) {
    event.preventDefault();
    if (!content.trim() || busy) return;
    busy = true;
    const tags = tagsInput.split(/[,\s]+/).map((t) => t.replace(/^#/, "").trim()).filter(Boolean).slice(0, 6);
    try {
      await api("POST", "/api/notes/save", {
        title: title.trim(),
        content: content.trim(),
        tags,
        folder: "Голосовые заметки",
      }, { signal });
      title = "";
      tagsInput = "";
      content = "";
      toast("Заметка сохранена в CyberNotes");
      await loadList();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function copyNote(item) {
    try {
      await copyText(item.snippet || item.title);
      toast("Скопировано");
    } catch {
      toast("Не удалось скопировать", true);
    }
  }

  async function speakNote(item) {
    if (speakingPath) return;
    const text = (item.snippet || item.title || "").trim();
    if (!text) return;
    speakingPath = item.path;
    try {
      await api("POST", "/api/say", { text, direct: true }, { signal });
      toast("Озвучиваю на колонке");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      speakingPath = "";
    }
  }

  function toggleExpand(path) {
    expandedPath = expandedPath === path ? "" : path;
  }

  onMount(() => {
    void loadList();
    return () => controller.abort();
  });
</script>

<div class="studio-grid">
  <section class="card">
    <h2>{@html icon("plus")} Новая заметка CyberNotes</h2>
    <form class="studio-stack" onsubmit={saveNote}>
      <div class="f">
        <label for="note-title">Заголовок (необяз.)</label>
        <input id="note-title" type="text" maxlength="100" placeholder="Короткий заголовок" bind:value={title} disabled={busy}>
      </div>
      <div class="f">
        <label for="note-tags">Теги через запятую</label>
        <input id="note-tags" type="text" maxlength="120" placeholder="идея, дом, покупка" bind:value={tagsInput} disabled={busy}>
      </div>
      <div class="f">
        <label for="note-body">Текст заметки</label>
        <textarea id="note-body" rows="5" maxlength="12000" placeholder="Что нужно записать…" bind:value={content} disabled={busy}></textarea>
      </div>
      <div class="mode-foot">
        <span class="meta">Всего в базе: {total}</span>
        <button type="submit" class="btn primary" disabled={busy || !content.trim()}>
          {@html icon("save")} Сохранить
        </button>
      </div>
    </form>
  </section>

  <section class="card">
    <div class="card-head">
      <h2>{@html icon("note")} База знаний</h2>
      <button type="button" class="btn sm ghost" disabled={loading} onclick={() => { query = ""; void loadList(); }}>
        {@html icon("refresh")} Обновить
      </button>
    </div>

    <form class="mode-row" onsubmit={search}>
      <input type="text" maxlength="120" placeholder="Поиск по заметкам…" bind:value={query}>
      <button type="submit" class="btn sm primary">{@html icon("search")} Найти</button>
    </form>

    {#if !available}
      <p class="empty">Каталог CyberNotes сейчас недоступен на сервере jarvis-voice.</p>
    {:else if loading}
      <p class="empty">Загружаю заметки…</p>
    {:else if !notes.length}
      <p class="empty">{query.trim() ? "Ничего не найдено." : "Заметок пока нет."}</p>
    {:else}
      <div class="notes-grid">
        {#each notes as item (item.path)}
          <article class="note-card" class:expanded={expandedPath === item.path}>
            <div class="note-card-head">
              <b>{item.title}</b>
              <div class="note-acts">
                <button
                  type="button"
                  class="ib"
                  title="Копировать"
                  aria-label="Копировать заметку"
                  onclick={() => void copyNote(item)}
                >{@html icon("copy")}</button>
                <button
                  type="button"
                  class="ib"
                  disabled={speakingPath === item.path}
                  title="Озвучить на колонке"
                  aria-label="Озвучить на колонке"
                  onclick={() => void speakNote(item)}
                >{@html icon("voice")}</button>
              </div>
            </div>
            <small>
              {item.path}
              {#if item.mtime} · {dateFmt.format(new Date(item.mtime * 1000))}{/if}
            </small>
            {#if item.snippet}
              <button type="button" class="note-toggle" onclick={() => toggleExpand(item.path)}>
                <p>{item.snippet}</p>
              </button>
            {/if}
          </article>
        {/each}
      </div>
    {/if}
  </section>
</div>
