<script>
  import { onMount } from "svelte";
  import { api, icon, toast, upload } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";

  const controller = new AbortController();
  const { signal } = controller;

  let books = $state([]);
  let loading = $state(true);
  let busy = $state(false);
  let dragOver = $state(false);
  let showUrl = $state(false);
  let bookUrl = $state("");
  let bookUrlTitle = $state("");
  let multiVoice = $state(false);
  let confirmId = $state("");
  let recapText = $state("");

  let reader = $derived($activity.reader || {});

  async function loadBooks() {
    loading = true;
    try {
      const res = await api("GET", "/api/books", undefined, { signal });
      if (!signal.aborted && Array.isArray(res?.books)) books = res.books;
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      if (!signal.aborted) loading = false;
    }
  }

  onMount(() => {
    void loadBooks();
    return () => controller.abort();
  });

  async function uploadFile(file) {
    if (!file || busy) return;
    busy = true;
    try {
      const qs = new URLSearchParams({ filename: file.name || "book.txt" }).toString();
      await upload("POST", "/api/books/upload?" + qs, file, { signal });
      await loadBooks();
      toast("Книга добавлена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  function onDrop(event) {
    event.preventDefault();
    dragOver = false;
    const f = event.dataTransfer?.files?.[0];
    if (f) void uploadFile(f);
  }

  async function submitBookUrl(event) {
    event.preventDefault();
    if (!bookUrl.trim() || busy) return;
    busy = true;
    try {
      await api("POST", "/api/books/url", { url: bookUrl.trim(), title: bookUrlTitle.trim() }, { signal });
      bookUrl = "";
      bookUrlTitle = "";
      showUrl = false;
      await loadBooks();
      toast("Глава добавлена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function playBook(id) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/books/play", { book_id: id, multi_voice: multiVoice }, { signal });
      await refreshActivity();
      toast("Читаю с закладки");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function pauseBook() {
    try {
      await api("POST", "/api/books/stop", {}, { signal });
      await refreshActivity();
      toast("Книга на паузе");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    }
  }

  async function navBook(delta) {
    try {
      const res = await api("POST", "/api/books/nav", { delta }, { signal });
      await Promise.all([refreshActivity(), loadBooks()]);
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
      recapText = res.text || "";
      toast("Пересказ озвучен");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function deleteBook(id) {
    confirmId = "";
    try {
      await api("DELETE", "/api/books/" + encodeURIComponent(id), undefined, { signal });
      books = books.filter((b) => b.id !== id);
      await refreshActivity();
      toast("Книга удалена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    }
  }
</script>

<section class="card">
  <div class="card-head">
    <h2>{@html icon("book")} Библиотека книг и ранобэ</h2>
    <div class="actions">
      <button type="button" class="btn sm" class:primary={multiVoice} onclick={() => { multiVoice = !multiVoice; }}>
        {@html icon("users")} Многоголосие
      </button>
      <button type="button" class="btn sm ghost" onclick={() => { showUrl = !showUrl; }}>
        {@html icon("link")} По ссылке
      </button>
      {#if books.length}
        <button type="button" class="btn sm ghost" disabled={busy} onclick={recapBook}>
          {@html icon("refresh")} Напомнить суть
        </button>
      {/if}
    </div>
  </div>

  <label
    class="dropzone"
    class:over={dragOver}
    ondragover={(e) => { e.preventDefault(); dragOver = true; }}
    ondragleave={() => { dragOver = false; }}
    ondrop={onDrop}
  >
    {@html icon("upload")}
    <b>Перетащите EPUB, FB2, ZIP, PDF или TXT</b>
    <small>Закладка сохраняется автоматически после каждого абзаца</small>
    <input type="file" accept=".epub,.fb2,.zip,.txt,.md,.pdf" hidden onchange={(e) => { const f = e.currentTarget.files?.[0]; e.currentTarget.value = ""; if (f) void uploadFile(f); }}>
  </label>

  {#if showUrl}
    <form class="mode-drawer" onsubmit={submitBookUrl}>
      <input type="text" maxlength="400" placeholder="Ссылка на главу или текст…" bind:value={bookUrl} disabled={busy}>
      <div class="mode-row">
        <input type="text" maxlength="80" placeholder="Название (необяз.)" bind:value={bookUrlTitle} disabled={busy}>
        <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("plus")} Добавить</button>
      </div>
    </form>
  {/if}

  {#if reader.running}
    <div class="dj-bar">
      <div class="dj-info">
        <b>{@html icon("book")} {reader.title}</b>
        <small>{reader.chapter_title || `Глава ${(reader.chapter || 0) + 1}`}</small>
      </div>
      <div class="dj-actions">
        <button type="button" class="ib" title="Пред. глава" aria-label="Предыдущая глава" onclick={() => navBook(-1)}>{@html icon("prev")}</button>
        <button type="button" class="ib" title="Пауза" aria-label="Пауза книги" onclick={pauseBook}>{@html icon("pause")}</button>
        <button type="button" class="ib" title="След. глава" aria-label="Следующая глава" onclick={() => navBook(1)}>{@html icon("skip")}</button>
      </div>
    </div>
  {/if}

  {#if recapText}
    <div class="recap-box">
      <div class="recap-box-h">
        <span>Краткий пересказ с закладки</span>
        <button type="button" class="ib" aria-label="Закрыть пересказ" onclick={() => { recapText = ""; }}>{@html icon("x")}</button>
      </div>
      <p>{recapText}</p>
    </div>
  {/if}

  {#if loading}
    <p class="empty">Загружаю библиотеку…</p>
  {:else if !books.length}
    <p class="empty">Библиотека пока пуста — добавьте файл или ссылку выше.</p>
  {:else}
    <div class="book-grid">
      {#each books as b (b.id)}
        {@const active = reader.running && reader.book_id === b.id}
        <article class="book-card" class:on={active}>
          <div class="book-card-h">
            <b>{b.title}</b>
            <div class="dj-actions">
              {#if active}
                <button type="button" class="ib on" title="Пауза" aria-label="Пауза" onclick={pauseBook}>{@html icon("pause")}</button>
              {:else}
                <button type="button" class="ib" title="Слушать" aria-label="Слушать книгу" onclick={() => playBook(b.id)}>{@html icon("play", "solid")}</button>
              {/if}
              {#if confirmId === b.id}
                <button type="button" class="ib danger" aria-label="Подтвердить удаление" onclick={() => deleteBook(b.id)}>{@html icon("check")}</button>
                <button type="button" class="ib" aria-label="Отмена" onclick={() => { confirmId = ""; }}>{@html icon("x")}</button>
              {:else}
                <button type="button" class="ib danger" title="Удалить" aria-label="Удалить книгу" onclick={() => { confirmId = b.id; }}>{@html icon("trash")}</button>
              {/if}
            </div>
          </div>
          <div class="meta">Глава {b.chapter + 1} из {b.chapters_count} · {b.progress_pct}%</div>
          <progress class="bar" value={b.progress_pct} max="100" aria-label="Прогресс чтения"></progress>
        </article>
      {/each}
    </div>
  {/if}
</section>
