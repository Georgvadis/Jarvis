<script>
  import { onMount } from "svelte";
  import { api, copyText, icon, toast } from "#helpers/core.js";
  import { renderMarkdownInto } from "#helpers/markdown.js";

  const controller = new AbortController();
  const { signal } = controller;
  const dateFmt = new Intl.DateTimeFormat("ru-RU", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

  let notes = $state([]);
  let folders = $state([]);
  let allTags = $state([]);
  let total = $state(0);
  let available = $state(true);
  let query = $state("");
  let selectedFolder = $state("");
  let selectedTag = $state("");
  let loading = $state(true);
  let busy = $state(false);
  let speakingPath = $state("");

  let title = $state("");
  let folder = $state("Голосовые заметки");
  let tagsInput = $state("");
  let content = $state("");
  let showComposer = $state(false);

  let openNote = $state(null);
  let modalLoading = $state(false);
  let editMode = $state(false);
  let editDraft = $state("");
  let modalBusy = $state(false);
  let confirmDelete = $state(false);
  let ragBusy = $state(false);
  let ragAnswer = $state("");

  let visibleNotes = $derived.by(() => {
    return notes.filter((item) => {
      if (selectedFolder && (item.folder || "") !== selectedFolder) return false;
      if (selectedTag && !(item.tags || []).some((t) => t.toLowerCase() === selectedTag.toLowerCase())) return false;
      return true;
    });
  });

  function renderMd(node, text) {
    renderMarkdownInto(node, text);
    return {
      update(nextText) {
        renderMarkdownInto(node, nextText);
      },
    };
  }

  async function loadList() {
    loading = true;
    try {
      const res = await api("GET", "/api/notes/list?limit=60", undefined, { signal });
      if (signal.aborted) return;
      available = res.available !== false;
      total = Number(res.total_notes || 0);
      folders = res.folders || [];
      allTags = res.tags || [];
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
    if (!q) {
      ragAnswer = "";
      return void loadList();
    }
    loading = true;
    try {
      const res = await api("GET", "/api/notes/search?q=" + encodeURIComponent(q) + "&limit=20", undefined, { signal });
      if (signal.aborted) return;
      total = Number(res.total_notes || 0);
      notes = res.hits || [];
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      if (!signal.aborted) loading = false;
    }
  }

  async function askRag() {
    const q = query.trim();
    if (!q || ragBusy) return;
    ragBusy = true;
    ragAnswer = "";
    try {
      const res = await api("POST", "/api/memory/rag", { query: q, ask: true, limit: 8 }, { signal });
      if (signal.aborted) return;
      ragAnswer = res.answer || "";
      const noteHits = (res.hits || []).filter((h) => h.source === "note" && h.path);
      if (noteHits.length) {
        notes = noteHits.map((h) => ({
          path: h.path,
          title: h.title,
          folder: h.folder || "",
          tags: h.tags || [],
          snippet: h.snippet || "",
          mtime: h.mtime || 0,
        }));
      }
      if (res.total_notes) total = Number(res.total_notes);
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      if (!signal.aborted) ragBusy = false;
    }
  }

  async function saveNote(event) {
    event.preventDefault();
    if (!content.trim() || busy) return;
    busy = true;
    const tags = tagsInput.split(/[,\s]+/).map((t) => t.replace(/^#/, "").trim()).filter(Boolean).slice(0, 8);
    try {
      await api("POST", "/api/notes/save", {
        title: title.trim(),
        folder: folder.trim() || "Голосовые заметки",
        content: content.trim(),
        tags,
      }, { signal });
      title = "";
      tagsInput = "";
      content = "";
      showComposer = false;
      toast("Заметка сохранена в CyberNotes");
      await loadList();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function openNoteModal(path) {
    if (modalLoading) return;
    modalLoading = true;
    confirmDelete = false;
    editMode = false;
    try {
      const res = await api("GET", "/api/notes/read?path=" + encodeURIComponent(path), undefined, { signal });
      if (signal.aborted) return;
      openNote = res;
      editDraft = res.content || "";
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      if (!signal.aborted) modalLoading = false;
    }
  }

  function closeModal() {
    openNote = null;
    editMode = false;
    editDraft = "";
    confirmDelete = false;
  }

  async function saveEditedNote() {
    if (!openNote || modalBusy) return;
    const nextText = editDraft;
    if (!nextText.trim()) return toast("Заметка не может быть пустой", true);
    modalBusy = true;
    try {
      const updated = await api("PUT", "/api/notes/update", {
        path: openNote.path,
        content: nextText,
      }, { signal });
      if (signal.aborted) return;
      openNote = { ...openNote, ...updated, content: nextText };
      editMode = false;
      toast("Заметка обновлена");
      if (query.trim()) await search();
      else await loadList();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      modalBusy = false;
    }
  }

  async function deleteOpenNote() {
    if (!openNote || modalBusy) return;
    modalBusy = true;
    try {
      await api("DELETE", "/api/notes/delete?path=" + encodeURIComponent(openNote.path), undefined, { signal });
      if (signal.aborted) return;
      toast("Заметка удалена");
      closeModal();
      if (query.trim()) await search();
      else await loadList();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      modalBusy = false;
    }
  }

  async function copyTextSafe(text) {
    try {
      await copyText(text);
      toast("Скопировано");
    } catch {
      toast("Не удалось скопировать", true);
    }
  }

  async function speakText(path, raw) {
    if (speakingPath) return;
    const text = (raw || "").trim().slice(0, 15000);
    if (!text) return;
    speakingPath = path;
    try {
      await api("POST", "/api/say", { text, direct: true }, { signal });
      toast("Озвучиваю на колонке");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      speakingPath = "";
    }
  }

  async function podcastFromNote() {
    if (!openNote || modalBusy) return;
    const raw = (openNote.content || "").trim();
    if (!raw) return;
    modalBusy = true;
    try {
      await api("POST", "/api/notebook", {
        title: openNote.title || "",
        text: raw.slice(0, 12000),
        turns: 6,
      }, { signal });
      toast("Подкаст по заметке запущен");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      modalBusy = false;
    }
  }

  function onKeydown(event) {
    if (event.key === "Escape" && openNote) {
      closeModal();
    }
  }

  onMount(() => {
    void loadList();
    return () => controller.abort();
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="studio-stack">
  <section class="card bento-card">
    <div class="card-head">
      <h2>{@html icon("note")} Заметки CyberNotes {#if total}<span class="badge">{total}</span>{/if}</h2>
      <div class="actions">
        <button type="button" class="btn sm" class:primary={!showComposer} onclick={() => { showComposer = !showComposer; }}>
          {@html icon(showComposer ? "x" : "plus")} {showComposer ? "Скрыть" : "Новая заметка"}
        </button>
        <button type="button" class="btn sm ghost" disabled={loading || ragBusy} onclick={() => { query = ""; selectedFolder = ""; selectedTag = ""; ragAnswer = ""; void loadList(); }}>
          {@html icon("refresh")} Обновить
        </button>
      </div>
    </div>

    {#if showComposer}
      <form class="mode-drawer studio-stack note-composer" onsubmit={saveNote}>
        <div class="grid g2">
          <div class="f">
            <label for="note-title">Заголовок (необяз.)</label>
            <input id="note-title" type="text" maxlength="100" placeholder="Короткий заголовок" bind:value={title} disabled={busy}>
          </div>
          <div class="grid g2">
            <div class="f">
              <label for="note-folder">Папка</label>
              <input id="note-folder" type="text" maxlength="80" list="note-folders" placeholder="Голосовые заметки" bind:value={folder} disabled={busy}>
              <datalist id="note-folders">
                <option value="Голосовые заметки"></option>
                {#each folders as f (f)}<option value={f}></option>{/each}
              </datalist>
            </div>
            <div class="f">
              <label for="note-tags">Теги</label>
              <input id="note-tags" type="text" maxlength="120" placeholder="идея, дом" bind:value={tagsInput} disabled={busy}>
            </div>
          </div>
        </div>
        <div class="f">
          <label for="note-body">Текст (Markdown)</label>
          <textarea id="note-body" rows="4" maxlength="12000" placeholder="Что нужно записать…" bind:value={content} disabled={busy}></textarea>
        </div>
        <div class="mode-foot">
          <span class="meta">Сохранится в CyberNotes (.md)</span>
          <button type="submit" class="btn primary" disabled={busy || !content.trim()}>
            {@html icon("save")} Сохранить
          </button>
        </div>
      </form>
    {/if}

    <form class="mode-row" onsubmit={search}>
      <input type="text" maxlength="120" placeholder="Поиск по заметкам или вопрос ИИ…" bind:value={query}>
      {#if folders.length > 0}
        <select class="note-folder-sel" aria-label="Фильтр по папке" bind:value={selectedFolder}>
          <option value="">Все папки</option>
          {#each folders as f (f)}<option value={f}>{f}</option>{/each}
        </select>
      {/if}
      <button type="submit" class="btn sm" disabled={loading || ragBusy}>{@html icon("search")} Найти</button>
      <button type="button" class="btn sm primary" disabled={loading || ragBusy || !query.trim()} onclick={() => void askRag()}>
        {@html icon("sparkles")} {ragBusy ? "Думаю…" : "ИИ-ответ"}
      </button>
    </form>

    {#if ragAnswer}
      <div class="note-rag-box">
        <div class="note-rag-head">
          <b>{@html icon("sparkles")} Ответ по базе знаний</b>
          <button type="button" class="ib" aria-label="Скрыть ответ" onclick={() => { ragAnswer = ""; }}>{@html icon("x")}</button>
        </div>
        <div class="md-prose" use:renderMd={ragAnswer}></div>
      </div>
    {/if}

    {#if allTags.length > 0}
      <div class="note-tags" role="group" aria-label="Фильтр по тегам">
        <button type="button" class="note-tag" class:on={!selectedTag} onclick={() => { selectedTag = ""; }}>Все</button>
        {#each allTags as tag (tag)}
          <button
            type="button"
            class="note-tag"
            class:on={selectedTag.toLowerCase() === tag.toLowerCase()}
            onclick={() => { selectedTag = selectedTag.toLowerCase() === tag.toLowerCase() ? "" : tag; }}
          >#{tag}</button>
        {/each}
      </div>
    {/if}

    {#if !available}
      <p class="empty">Каталог CyberNotes сейчас недоступен на сервере jarvis-voice.</p>
    {:else if loading}
      <p class="empty">Загружаю заметки…</p>
    {:else if !visibleNotes.length}
      <p class="empty">{query.trim() || selectedFolder || selectedTag ? "Ничего не найдено." : "Заметок пока нет."}</p>
    {:else}
      <div class="notes-grid notes-grid-wide">
        {#each visibleNotes as item (item.path)}
          <article class="note-card">
            <div class="note-card-head">
              <button type="button" class="note-open-title" onclick={() => void openNoteModal(item.path)}>
                <b>{item.title}</b>
              </button>
              <div class="note-acts">
                <button type="button" class="ib" title="Открыть / редактировать" aria-label="Открыть заметку" onclick={() => void openNoteModal(item.path)}>{@html icon("edit")}</button>
                <button type="button" class="ib" title="Копировать" aria-label="Копировать заметку" onclick={() => void copyTextSafe(item.snippet || item.title)}>{@html icon("copy")}</button>
                <button type="button" class="ib" disabled={speakingPath === item.path} title="Озвучить на колонке" aria-label="Озвучить на колонке" onclick={() => void speakText(item.path, item.snippet || item.title)}>{@html icon("voice")}</button>
              </div>
            </div>
            <small>
              {#if item.folder}{item.folder}{/if}
              {#if item.mtime}{#if item.folder} · {/if}{dateFmt.format(new Date(item.mtime * 1000))}{/if}
            </small>
            {#if item.tags?.length}
              <div class="note-card-tags">
                {#each item.tags.slice(0, 4) as t (t)}
                  <span class="chip">#{t}</span>
                {/each}
              </div>
            {/if}
            {#if item.snippet}
              <button type="button" class="note-toggle" onclick={() => void openNoteModal(item.path)}>
                <p>{item.snippet}</p>
              </button>
            {/if}
          </article>
        {/each}
      </div>
    {/if}
  </section>
</div>

{#if openNote}
  <div class="note-modal-bg" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) closeModal(); }}>
    <div class="note-modal card" role="dialog" aria-modal="true" aria-labelledby="note-modal-title">
      <div class="note-modal-head">
        <div>
          <h2 id="note-modal-title">{openNote.title || "Заметка"}</h2>
          <small class="meta">
            {openNote.path}
            {#if openNote.mtime} · {dateFmt.format(new Date(openNote.mtime * 1000))}{/if}
          </small>
        </div>
        <button type="button" class="ib lg" onclick={closeModal} aria-label="Закрыть">{@html icon("x")}</button>
      </div>

      <div class="note-modal-bar">
        <button type="button" class="btn sm" class:primary={editMode} onclick={() => { editMode = !editMode; editDraft = openNote.content || ""; }}>
          {@html icon(editMode ? "eye" : "edit")} {editMode ? "Просмотр" : "Редактировать"}
        </button>
        <button type="button" class="btn sm ghost" disabled={speakingPath === openNote.path} onclick={() => void speakText(openNote.path, openNote.content)}>
          {@html icon("voice")} Озвучить
        </button>
        <button type="button" class="btn sm ghost" disabled={modalBusy} onclick={podcastFromNote}>
          {@html icon("podcast")} Подкаст
        </button>
        <button type="button" class="btn sm ghost" onclick={() => void copyTextSafe(openNote.content || "")}>
          {@html icon("copy")} Копировать
        </button>
        <div class="note-modal-spacer"></div>
        {#if confirmDelete}
          <div class="inline-confirm" role="alert">
            <span>Удалить?</span>
            <button type="button" class="btn sm danger" disabled={modalBusy} onclick={deleteOpenNote}>{@html icon("check")} Да</button>
            <button type="button" class="btn sm ghost" disabled={modalBusy} onclick={() => { confirmDelete = false; }}>{@html icon("x")} Нет</button>
          </div>
        {:else}
          <button type="button" class="btn sm ghost danger" onclick={() => { confirmDelete = true; }}>
            {@html icon("trash")} Удалить
          </button>
        {/if}
      </div>

      {#if editMode}
        <div class="studio-stack">
          <textarea class="note-editor" rows="14" maxlength="100000" bind:value={editDraft} disabled={modalBusy}></textarea>
          <div class="mode-foot">
            <span class="meta">Markdown (.md)</span>
            <div class="row">
              <button type="button" class="btn sm ghost" disabled={modalBusy} onclick={() => { editMode = false; editDraft = openNote.content || ""; }}>{@html icon("x")} Отмена</button>
              <button type="button" class="btn sm primary" disabled={modalBusy || !editDraft.trim()} onclick={saveEditedNote}>
                {@html icon("save")} Сохранить
              </button>
            </div>
          </div>
        </div>
      {:else}
        <div class="note-md" use:renderMd={openNote.content || ""}></div>
      {/if}
    </div>
  </div>
{/if}
