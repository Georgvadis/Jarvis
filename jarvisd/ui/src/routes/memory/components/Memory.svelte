<script>
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { api, icon, plural, tip, toast } from "#helpers/core.js";
  import { renderMarkdownInto } from "#helpers/markdown.js";
  import { reload } from "#state/panelState.js";
  import StudioNotes from "../../studio/components/StudioNotes.svelte";
  import Archive from "./Archive/Archive.svelte";
  import styles from "./Memory.module.css";

  const VALID_TABS = new Set(["facts", "notes", "archive"]);

  const providers = [
    ["chatgpt", "Подписка ChatGPT"],
    ["gemini", "Google Gemini — ключ"],
    ["openai", "OpenAI — ключ, платно"],
  ];
  const dateFormat = new Intl.DateTimeFormat("ru-RU", { day: "numeric", month: "long", year: "numeric" });
  const summaryFormat = new Intl.DateTimeFormat("ru-RU", {
    day: "numeric", month: "long", hour: "2-digit", minute: "2-digit",
  });
  const controller = new AbortController();
  const { signal } = controller;

  let data = $state.raw(null);
  let loadError = $state("");
  let busy = $state(false);
  let tab = $state("facts");
  let archiveView = $state();

  $effect(() => {
    const q = page.url.searchParams.get("tab");
    if (q && VALID_TABS.has(q)) tab = q;
    else if (!q) tab = "facts";
  });

  function switchTab(id) {
    tab = id;
    void goto(id === "facts" ? "/memory" : `/memory?tab=${id}`, { replaceState: true, keepFocus: true, noScroll: true });
  }
  let editor = $state.raw(null);
  let draft = $state("");
  let newFact = $state("");
  let factQuery = $state("");
  let confirmMonth = $state("");
  let confirmReset = $state("");
  let ragQuery = $state("");
  let ragBusy = $state(false);
  let ragAnswer = $state("");
  let ragHits = $state.raw([]);
  let ragDone = $state(false);
  let memory = $derived(data?.memory || {});
  let settings = $derived(data?.settings || {});
  let facts = $derived((memory.profile || "").split("\n").map((line) => line.trim())
    .filter(Boolean).map((line) => line.replace(/^[-•*]\s*/, "")));
  let filteredFacts = $derived(
    facts.map((fact, index) => ({ fact, index }))
      .filter(({ fact }) => !factQuery.trim() || fact.toLowerCase().includes(factQuery.trim().toLowerCase()))
  );
  let chronicle = $derived((memory.chronicle || []).slice().reverse());
  let archiveCount = $derived(memory.archive || 0);
  let provider = $derived(settings.provider || "");
  let available = $derived(settings.has || {});
  let modelProvider = $derived(available[provider] ? provider : settings.active);

  onMount(() => {
    loadMemory().catch((error) => {
      if (!signal.aborted) loadError = error.message;
    });
    return () => controller.abort();
  });

  async function loadMemory() {
    data = await api("GET", "/api/memory", undefined, { signal });
  }

  async function run(action) {
    if (busy) return;
    busy = true;
    try {
      await action();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  function edit(kind, text, key) {
    editor = { kind, key };
    draft = text;
  }

  function focus(input) {
    input.focus();
  }

  function cancelEdit() {
    if (!busy) editor = null;
  }

  function save(path, body, message, afterSave) {
    void run(async () => {
      const next = await api("PUT", path, body);
      if (signal.aborted) return;
      data = next;
      editor = null;
      afterSave?.();
      toast(message);
    });
  }

  function saveFacts(list, message, afterSave) {
    const profile = list.map((fact) => fact.trim()).filter(Boolean).map((fact) => "- " + fact).join("\n");
    save("/api/memory", { profile }, message, afterSave);
  }

  function addFact(event) {
    event.preventDefault();
    if (!newFact.trim()) return toast("Напишите, что Джарвису стоит знать", true);
    saveFacts([...facts, newFact], "Запомнил", () => newFact = "");
  }

  function saveFact(event, index) {
    event.preventDefault();
    const next = facts.slice();
    if (draft.trim()) next[index] = draft;
    else next.splice(index, 1);
    saveFacts(next, "Сохранено");
  }

  function saveSummary(event) {
    event.preventDefault();
    save("/api/memory", { summary: draft }, "Конспект сохранён");
  }

  function saveMonth(event, month) {
    event.preventDefault();
    if (!draft.trim()) return toast("Пустой месяц — удалите его кнопкой с корзиной", true);
    save("/api/memory/month", { month, text: draft.trim() }, "Летопись сохранена");
  }

  function deleteMonth(event, month) {
    event.preventDefault();
    confirmMonth = "";
    save("/api/memory/month", { month, text: "" }, "Месяц удалён");
  }

  function changeSettings(changes, message = "Сохранено") {
    const next = { off: !!settings.off, provider, model: settings.model || "", ...changes };
    void run(async () => {
      await api("PUT", "/api/settings", { memory: next });
      if (signal.aborted) return;
      await reload();
      await loadMemory();
      toast(message);
    });
  }

  function changeEnabled(event) {
    const input = event.currentTarget;
    const off = !input.checked;
    input.checked = !settings.off;
    changeSettings({ off }, off ? "Память выключена — каждый разговор с чистого листа" : "Джарвис снова запоминает разговоры");
  }

  function changeProvider(event) {
    const input = event.currentTarget;
    const value = input.value;
    input.value = provider;
    changeSettings({ provider: value, model: "" });
  }

  function changeVersion(event) {
    const input = event.currentTarget;
    const model = input.value;
    input.value = settings.active_model ?? "";
    changeSettings({ model });
  }

  function providerName(id) {
    return providers.find(([key]) => key === id)?.[1] || id;
  }

  function versionName(version) {
    return version || "по очереди: кончился лимит — следующая";
  }

  function compact() {
    void run(async () => {
      const result = await api("POST", "/api/memory/compact");
      if (signal.aborted) return;
      await loadMemory();
      await archiveView?.refresh();
      if (!signal.aborted) toast(result.turns ? "Досье и конспект обновлены" : "Сжимать пока нечего");
    });
  }

  function renderMd(node, text) {
    renderMarkdownInto(node, text);
    return {
      update(nextText) {
        renderMarkdownInto(node, nextText);
      },
    };
  }

  async function runRag(ask = false) {
    const q = ragQuery.trim();
    if (!q || ragBusy) return;
    ragBusy = true;
    try {
      const res = await api("POST", "/api/memory/rag", { query: q, ask, limit: 8 }, { signal });
      if (signal.aborted) return;
      ragAnswer = res.answer || "";
      ragHits = res.hits || [];
      ragDone = true;
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      ragBusy = false;
    }
  }

  function clearRag() {
    ragQuery = "";
    ragAnswer = "";
    ragHits = [];
    ragDone = false;
  }

  function exportToNotes() {
    void run(async () => {
      const res = await api("POST", "/api/memory/export-notes", {}, { signal });
      if (!signal.aborted) toast("Сохранено в CyberNotes: " + res.path);
    });
  }

  function resetChat(all) {
    confirmReset = "";
    void run(async () => {
      await api("POST", all ? "/api/chatgpt/newchat?all=1" : "/api/chatgpt/newchat");
      if (signal.aborted) return;
      editor = null;
      await loadMemory();
      await archiveView?.refresh(all);
      if (!signal.aborted) toast(all ? "Память стёрта" : "Новый чат начат");
    });
  }
</script>

<svelte:window onkeydown={(event) => { if (event.key === "Escape") cancelEdit(); }} />

<header class="ph ph-row">
  <div>
    <h1>Знания</h1>
  </div>
  {#if data}
    <div class="actions">
      <label class={["sw", styles.toggle]}>
        <input type="checkbox" checked={!settings.off} disabled={busy} onchange={changeEnabled}>
        <span class="tr"></span>
        Запоминать
      </label>
    </div>
  {/if}
</header>

<div class="hub-tabs" role="tablist" aria-label="Разделы знаний">
  <button type="button" role="tab" aria-selected={tab === "facts"} class="hub-tab" class:on={tab === "facts"} onclick={() => switchTab("facts")}>
    {@html icon("brain")}
    <span>Память и поиск</span>
  </button>
  <button type="button" role="tab" aria-selected={tab === "notes"} class="hub-tab" class:on={tab === "notes"} onclick={() => switchTab("notes")}>
    {@html icon("note")}
    <span>Заметки</span>
  </button>
  <button type="button" role="tab" aria-selected={tab === "archive"} class="hub-tab" class:on={tab === "archive"} onclick={() => switchTab("archive")}>
    {@html icon("archive")}
    <span>Архив и настройки</span>
  </button>
</div>

{#snippet stat(count, label, since = "")}
  <div class={styles.stat}>
    <b>{count}</b><span>{label}</span>
    {#if since}<small>{since}</small>{/if}
  </div>
{/snippet}

{#snippet editorActions(small = false)}
  <div class="row">
    <span class="spacer"></span>
    <button class={["btn ghost", { sm: small }]} type="button" disabled={busy} onclick={cancelEdit}>Отмена</button>
    <button class={["btn primary", { sm: small }]} type="submit" disabled={busy}>Сохранить</button>
  </div>
{/snippet}

{#if tab === "notes"}
  <StudioNotes />
{:else if loadError}
  <p class="err" role="alert">{loadError}</p>
{:else if !data}
  <div class="card plg-empty" role="status"><span class="spin"></span><p class="muted">Загружаю память…</p></div>
{:else if tab === "facts"}
  <div class={styles.memory}>
    {#if settings.off}
      <p class={["note", styles.off]}>
        {@html icon("moon")}
        <span>Память выключена — новые разговоры не сохраняются.</span>
      </p>
    {/if}

    <div class={styles.stats}>
      {@render stat(facts.length, plural(facts.length, "факт", "факта", "фактов"))}
      {@render stat(archiveCount, plural(archiveCount, "в архиве", "в архиве", "в архиве"),
        memory.archive_since ? "с " + dateFormat.format(memory.archive_since * 1000) : "")}
      {@render stat(chronicle.length, plural(chronicle.length, "месяц", "месяца", "месяцев"))}
      {@render stat(memory.turns || 0, "в чате")}
    </div>

    <section class={["card", styles.section]}>
      <div class={styles.heading}>
        <div>
          <h2>{@html icon("sparkles")}Умный поиск</h2>
        </div>
      </div>
      <form class={styles.ragForm} onsubmit={(e) => { e.preventDefault(); void runRag(false); }}>
        <input type="search" maxlength="160" placeholder="Вопрос или тема по памяти и заметкам…" bind:value={ragQuery} disabled={ragBusy}>
        <button class="btn sm ghost" type="submit" disabled={ragBusy || !ragQuery.trim()}>{@html icon("search")} Найти</button>
        <button class="btn sm primary" type="button" disabled={ragBusy || !ragQuery.trim()} onclick={() => void runRag(true)}>
          {@html icon("sparkles")} {ragBusy ? "Ищу…" : "ИИ-ответ"}
        </button>
        {#if ragDone}
          <button class="ib" type="button" aria-label="Сбросить поиск" onclick={clearRag}>{@html icon("x")}</button>
        {/if}
      </form>
      {#if ragAnswer}
        <div class={["md-prose", styles.ragAnswer]} use:renderMd={ragAnswer}></div>
      {/if}
      {#if ragHits.length}
        <div class={styles.ragHits}>
          {#each ragHits as hit, i (`${hit.source}:${hit.id || hit.path}:${i}`)}
            <article class={styles.ragHit}>
              <div class={styles.ragHitHead}>
                <span class="chip">{hit.source === "note" ? "Заметка" : "Память"}</span>
                <b>{hit.title}</b>
                {#if hit.folder}<small>{hit.folder}</small>{/if}
                <small class={styles.push}>{Math.round((hit.score || 0) * 100)}%</small>
              </div>
              <p>{hit.snippet}</p>
            </article>
          {/each}
        </div>
      {:else if ragDone && !ragAnswer}
        <p class={styles.empty}>Ничего не найдено.</p>
      {/if}
    </section>

    <section class={["card", styles.section]}>
      <div class={styles.heading}>
        <div>
          <h2>{@html icon("read")}Факты о вас</h2>
        </div>
        <div class="row">
          {#if facts.length > 3}
            <label class="cx-search mem-search">
              {@html icon("search")}
              <input type="search" aria-label="Поиск по фактам" placeholder="Поиск по фактам" bind:value={factQuery}>
            </label>
          {/if}
          {#if facts.length || memory.summary || chronicle.length}
            <button class="btn sm ghost" type="button" disabled={busy} onclick={exportToNotes}
              title="Экспортировать в CyberNotes">
              {@html icon("upload")} В заметки
            </button>
          {/if}
        </div>
      </div>
      {#if facts.length}
        <ul class={styles.facts}>
          {#each filteredFacts as { fact, index } (index)}
            <li>
              {#if editor?.kind === "fact" && editor.key === index}
                <form class={[styles.fact, styles.editFact]} onsubmit={(event) => saveFact(event, index)}>
                  <input type="text" aria-label="Факт" bind:value={draft} use:focus disabled={busy}>
                  <button class="btn sm primary" type="submit" disabled={busy}>{@html icon("save")}Сохранить</button>
                  <button class="btn sm ghost" type="button" onclick={cancelEdit} disabled={busy}>Отмена</button>
                </form>
              {:else}
                <div class={styles.fact}>
                  <span>{fact}</span>
                  <button class="ib" type="button" aria-label="Изменить" onclick={() => edit("fact", fact, index)}
                    disabled={busy}>{@html icon("edit")}</button>
                  <button class="ib danger" type="button" aria-label="Удалить"
                    disabled={busy} onclick={() => saveFacts(facts.filter((_, item) => item !== index), "Факт удалён")}>{@html icon("trash")}</button>
                </div>
              {/if}
            </li>
          {:else}
            <p class={styles.empty}>По запросу «{factQuery}» ничего не найдено.</p>
          {/each}
        </ul>
      {:else}
        <p class={styles.empty}>Пока пусто. Добавьте первый факт ниже.</p>
      {/if}
      <form class={styles.add} onsubmit={addFact}>
        <input type="text" aria-label="Новый факт" placeholder="Например: «люблю фильтр-кофе»"
          maxlength="300" bind:value={newFact} disabled={busy}>
        <button class="btn" type="submit" disabled={busy}>{@html icon("plus")}Добавить</button>
      </form>
    </section>

    <section class={["card", styles.section]}>
      <div class={styles.heading}>
        <div>
          <h2>{@html icon("todo")}Конспект разговоров</h2>
          {#if memory.summary_at}<p class="muted">{summaryFormat.format(memory.summary_at * 1000)}</p>{/if}
        </div>
        <div class="row">
          {#if !settings.off && settings.active && archiveCount}
            <button class="btn sm" type="button" disabled={busy} onclick={compact}
              data-tip="Сжать разговор в досье и конспект сейчас">{@html icon("refresh")}Обновить</button>
          {/if}
          {#if memory.summary && editor?.kind !== "summary"}
            <button class="btn sm ghost" type="button" disabled={busy}
              onclick={() => edit("summary", memory.summary)}>{@html icon("edit")}Изменить</button>
          {/if}
        </div>
      </div>
      {#if editor?.kind === "summary"}
        <form class={styles.editText} onsubmit={saveSummary}>
          <textarea rows="10" aria-label="Конспект" bind:value={draft} use:focus disabled={busy}></textarea>
          {@render editorActions()}
        </form>
      {:else if memory.summary}
        <div class={styles.text}>{memory.summary}</div>
      {:else}
        <p class={styles.empty}>Появится после первого длинного разговора.</p>
      {/if}
    </section>
  </div>
{:else}
  <div class={styles.memory}>
    {#if chronicle.length}
      <section class={["card", styles.section]}>
        <div class={styles.heading}>
          <div>
            <h2>{@html icon("moon")}Летопись</h2>
          </div>
        </div>
        <div class={styles.months}>
          {#each chronicle as item (item.month)}
            {#if editor?.kind === "month" && editor.key === item.month}
              <form class={[styles.month, styles.editMonth]} onsubmit={(event) => saveMonth(event, item.month)}>
                <div class={styles.monthHeading}><b>{item.label}</b></div>
                <div class={styles.editText}>
                  <textarea rows="8" aria-label="Текст летописи" bind:value={draft} use:focus disabled={busy}></textarea>
                  {@render editorActions(true)}
                </div>
              </form>
            {:else}
              <details class={styles.month}>
                <summary>
                  <b>{item.label}</b><small>{item.turns} {plural(item.turns, "реплика", "реплики", "реплик")}</small>
                  <button class={["ib", styles.push]} type="button" aria-label="Изменить" disabled={busy}
                    onclick={(event) => { event.preventDefault(); edit("month", item.text, item.month); }}>{@html icon("edit")}</button>
                  {#if confirmMonth === item.month}
                    <button class="ib danger" type="button" aria-label="Подтвердить удаление" disabled={busy}
                      onclick={(event) => deleteMonth(event, item.month)}>{@html icon("check")}</button>
                    <button class="ib" type="button" aria-label="Отмена" disabled={busy}
                      onclick={(event) => { event.preventDefault(); confirmMonth = ""; }}>{@html icon("x")}</button>
                  {:else}
                    <button class="ib danger" type="button" aria-label="Удалить" disabled={busy}
                      onclick={(event) => { event.preventDefault(); confirmMonth = item.month; }}>{@html icon("trash")}</button>
                  {/if}
                </summary>
                <div class={styles.text}>{item.text}</div>
              </details>
            {/if}
          {/each}
        </div>
      </section>
    {/if}

    <Archive bind:this={archiveView} {busy} {run} onforget={loadMemory} />

    {#if !settings.off}
      <section class={["card", styles.section]}>
        <div class={styles.heading}>
          <div>
            <h2>{@html icon("tune")}Сжатие разговоров</h2>
          </div>
        </div>
        <div class="row">
          <div class="f half">
            <label for="mem-prov">Модель {@html tip("«Сам выбирает» берёт сначала бесплатное: подписку ChatGPT, потом Gemini, потом платный ключ OpenAI.")}</label>
            <select id="mem-prov" value={provider} disabled={busy} onchange={changeProvider}>
              <option value="">Сам выбирает</option>
              {#each providers as [id, name] (id)}
                <option value={id} disabled={!available[id]}>{name}{available[id] ? "" : " — не подключено"}</option>
              {/each}
              <option value="none">Не сжимать</option>
            </select>
          </div>
          {#if modelProvider && provider !== "none"}
            <div class="f half">
              <label for="mem-model">Версия</label>
              <select id="mem-model" value={settings.active_model ?? ""} disabled={busy} onchange={changeVersion}>
                {#each settings.models?.[modelProvider] || [] as model (model)}
                  <option value={model}>{versionName(model)}</option>
                {/each}
              </select>
            </div>
          {/if}
        </div>
        {#if provider === "none"}
          <p class="note">Сжатие выключено: поиск работает по архиву.</p>
        {:else if settings.active}
          <p class="note">Активно: <b>{providerName(settings.active)}</b> · {versionName(settings.active_model)}.</p>
        {:else}
          <p class="err">Нужна текстовая модель в <a class="lnk" href="/connections">Подключениях</a>.</p>
        {/if}
      </section>
    {/if}

    <section class={["card", styles.section, styles.danger]}>
      <div class={styles.heading}>
        <div>
          <h2>Сброс памяти</h2>
        </div>
      </div>
      <div class="row">
        <button class="btn" type="button" disabled={busy || !(memory.turns || memory.summary)}
          onclick={() => { confirmReset = "chat"; }}>{@html icon("sparkles")}Новый чат</button>
        <span class="spacer"></span>
        <button class="btn ghost danger" type="button" disabled={busy || !(archiveCount || facts.length || memory.summary)}
          onclick={() => { confirmReset = "all"; }}>{@html icon("trash")}Стереть всё</button>
      </div>
      {#if confirmReset}
        <div class="inline-confirm" role="alert">
          <span>
            {confirmReset === "all"
              ? "Стереть всю память Джарвиса?"
              : "Начать новый чат и сбросить конспект?"}
          </span>
          <button class="btn danger sm" type="button" disabled={busy} onclick={() => resetChat(confirmReset === "all")}>
            {@html icon("check")}{confirmReset === "all" ? "Стереть" : "Сбросить"}
          </button>
          <button class="btn ghost sm" type="button" disabled={busy} onclick={() => { confirmReset = ""; }}>
            {@html icon("x")}Отмена
          </button>
        </div>
      {/if}
    </section>
  </div>
{/if}
