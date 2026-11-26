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
  let showAdd = $state(false);
  let showUrl = $state(false);
  let bookUrl = $state("");
  let bookUrlTitle = $state("");
  let multiVoice = $state(false);
  let confirmId = $state("");
  let recapText = $state("");
  let tocBookId = $state("");
  let tocData = $state(null);
  let tocLoading = $state(false);

  let reader = $derived($activity.reader || {});
  let bookSleepS = $derived(Number(reader.sleep_s || 0));
  let featuredBook = $derived.by(() => {
    if (reader.running) return books.find((b) => b.id === reader.book_id) || null;
    return books[0] || null;
  });

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
      showAdd = false;
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
      if (tocBookId === id) {
        tocBookId = "";
        tocData = null;
      }
      await refreshActivity();
      toast("Книга удалена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    }
  }

  async function toggleToc(id) {
    if (tocBookId === id) {
      tocBookId = "";
      tocData = null;
      return;
    }
    tocLoading = true;
    try {
      const res = await api("GET", `/api/books/${encodeURIComponent(id)}/chapters`, undefined, { signal });
      if (!signal.aborted) {
        tocBookId = id;
        tocData = res;
      }
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      if (!signal.aborted) tocLoading = false;
    }
  }

  async function jumpToChapter(bookId, chapter) {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", "/api/books/jump", { book_id: bookId, chapter }, { signal });
      await Promise.all([refreshActivity(), loadBooks()]);
      tocBookId = "";
      toast(res.message || `Глава ${chapter + 1}`);
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function setBookSleep(mins) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/books/sleep", { minutes: mins }, { signal });
      await refreshActivity();
      toast(mins ? `Таймер сна книги: ${mins} мин` : "Таймер сна книги отменён");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function saveBookQuote() {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", "/api/books/quote", {}, { signal });
      toast(res.message || "Цитата сохранена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }
</script>

<div class="studio-stack">
  {#if reader.running || featuredBook}
    <section class="card book-hero" class:live={reader.running}>
      <div class="book-hero-top">
        <span class="book-hero-cover" aria-hidden="true">{@html icon("book")}</span>
        <div class="book-hero-meta">
          <span class="hearth-now-kicker">{reader.running ? "Читаю сейчас" : "Продолжить чтение"}</span>
          <h2>{reader.running ? reader.title : featuredBook.title}</h2>
          <small>
            {#if reader.running}
              {reader.chapter_title || `Глава ${(reader.chapter || 0) + 1}`}
              {#if bookSleepS > 0} · сон через {Math.ceil(bookSleepS / 60)}м{/if}
            {:else}
              Глава {featuredBook.chapter + 1} из {featuredBook.chapters_count} · {featuredBook.progress_pct}%
            {/if}
          </small>
        </div>
        <div class="book-hero-acts">
          {#if reader.running}
            <button type="button" class="ib" title="Цитата в CyberNotes" aria-label="Сохранить цитату" disabled={busy} onclick={saveBookQuote}>{@html icon("note")}</button>
            <button type="button" class="ib" title="Оглавление" aria-label="Оглавление" onclick={() => toggleToc(reader.book_id)}>{@html icon("list")}</button>
            <button type="button" class="ib" title="Пред. глава" aria-label="Предыдущая глава" onclick={() => navBook(-1)}>{@html icon("prev")}</button>
            <button type="button" class="dock-play" title="Пауза" aria-label="Пауза книги" onclick={pauseBook}>{@html icon("pause", "solid")}</button>
            <button type="button" class="ib" title="След. глава" aria-label="Следующая глава" onclick={() => navBook(1)}>{@html icon("skip")}</button>
          {:else}
            <button type="button" class="btn sm ghost" disabled={busy} onclick={recapBook}>{@html icon("refresh")} Суть</button>
            <button type="button" class="btn primary" disabled={busy} onclick={() => playBook(featuredBook.id)}>
              {@html icon("play", "solid")} Слушать
            </button>
          {/if}
        </div>
      </div>
      {#if featuredBook}
        <progress class="bar" value={featuredBook.progress_pct} max="100" aria-label="Прогресс чтения"></progress>
      {/if}
      {#if reader.running}
        <div class="amb-sleep">
          <span class="amb-sleep-lbl">{@html icon("moon")} {bookSleepS > 0 ? `Сон книги: ${Math.ceil(bookSleepS / 60)} мин` : "Таймер сна книги"}</span>
          <div class="amb-sleep-btns">
            {#each [15, 30, 45] as m (m)}
              <button type="button" class="btn sm ghost" disabled={busy} onclick={() => setBookSleep(m)}>{m}м</button>
            {/each}
            {#if bookSleepS > 0}
              <button type="button" class="btn sm ghost danger" disabled={busy} onclick={() => setBookSleep(0)}>{@html icon("x")}</button>
            {/if}
          </div>
        </div>
      {/if}
    </section>
  {/if}

  <section class="card">
    <div class="card-head">
      <h2>{@html icon("book")} Книжная полка {#if books.length}<span class="badge">{books.length}</span>{/if}</h2>
      <div class="actions">
        <button type="button" class="btn sm" class:primary={multiVoice} onclick={() => { multiVoice = !multiVoice; }}>
          {@html icon("users")} Многоголосие
        </button>
        <button type="button" class="btn sm" class:primary={showAdd} onclick={() => { showUrl = false; showAdd = !showAdd; }}>
          {@html icon(showAdd ? "x" : "plus")} Файл
        </button>
        <button type="button" class="btn sm ghost" class:on={showUrl} onclick={() => { showAdd = false; showUrl = !showUrl; }}>
          {@html icon("link")} По ссылке
        </button>
      </div>
    </div>

    {#if showAdd || (!loading && !books.length)}
      <label
        class="dropzone"
        class:over={dragOver}
        ondragover={(e) => { e.preventDefault(); dragOver = true; }}
        ondragleave={() => { dragOver = false; }}
        ondrop={onDrop}
      >
        {@html icon("upload")}
        <b>Перетащите EPUB, FB2, ZIP, PDF или TXT</b>
        <small>Закладка сохраняется автоматически</small>
        <input type="file" accept=".epub,.fb2,.zip,.txt,.md,.pdf" hidden onchange={(e) => { const f = e.currentTarget.files?.[0]; e.currentTarget.value = ""; if (f) void uploadFile(f); }}>
      </label>
    {/if}

    {#if showUrl}
      <form class="mode-drawer" onsubmit={submitBookUrl}>
        <input type="text" maxlength="400" placeholder="Ссылка на главу или текст…" bind:value={bookUrl} disabled={busy}>
        <div class="mode-row">
          <input type="text" maxlength="80" placeholder="Название (необяз.)" bind:value={bookUrlTitle} disabled={busy}>
          <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("plus")} Добавить</button>
        </div>
      </form>
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
    {:else if books.length}
      <div class="book-grid">
        {#each books as b (b.id)}
          {@const active = reader.running && reader.book_id === b.id}
          <article class="book-card" class:on={active}>
            <div class="book-card-h">
              <b>{b.title}</b>
              <div class="dj-actions">
                <button type="button" class="ib" class:on={tocBookId === b.id} title="Оглавление" aria-label="Оглавление" disabled={tocLoading} onclick={() => toggleToc(b.id)}>{@html icon("list")}</button>
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
            {#if tocBookId === b.id && tocData?.chapters?.length}
              <div class="chap-list">
                {#each tocData.chapters as ch (ch.index)}
                  <button
                    type="button"
                    class="chap-btn"
                    class:on={b.chapter === ch.index}
                    disabled={busy}
                    onclick={() => jumpToChapter(b.id, ch.index)}
                  >{ch.index + 1}. {ch.title}</button>
                {/each}
              </div>
            {/if}
          </article>
        {/each}
      </div>
    {/if}
  </section>
</div>
