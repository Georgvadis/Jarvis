<script>
  import { api, icon, toast, upload } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";
  import { S } from "#state/panelState.js";

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

  let profiles = $derived(S.profiles || []);
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

  let activeMode = $derived($activity.modes?.mode || "normal");
  let activeLang = $derived($activity.modes?.lang || "");
  let debateActive = $derived(Boolean($activity.debate_active));

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

<div class="studio-grid">
  <section class="card">
    <h2>{@html icon("podcast")} Локальный NotebookLM {debateActive ? "· в эфире" : ""}</h2>
    <form class="studio-stack" onsubmit={submitNotebook}>
      <label
        class="dropzone"
        class:over={dragOver}
        ondragover={(e) => { e.preventDefault(); dragOver = true; }}
        ondragleave={() => { dragOver = false; }}
        ondrop={onDropFile}
      >
        {@html icon("upload")}
        <b>{nbFile ? nbFile.name : "Перетащите PDF, EPUB, FB2 или TXT"}</b>
        <small>{nbFile ? "Нажмите, чтобы выбрать другой файл" : "Или кликните для выбора файла"}</small>
        <input type="file" accept=".pdf,.epub,.fb2,.txt,.md" hidden onchange={(e) => { nbFile = e.currentTarget.files?.[0] || null; }}>
      </label>

      {#if nbFile}
        <button type="button" class="btn sm ghost" onclick={() => { nbFile = null; }}>{@html icon("x")} Убрать файл</button>
      {/if}

      <div class="f">
        <label for="nb-url">Ссылка на YouTube или статью</label>
        <input id="nb-url" type="text" maxlength="500" placeholder="https://…" bind:value={nbUrl} disabled={busy}>
      </div>

      <div class="f">
        <label for="nb-text">Или вставьте текст / конспект</label>
        <textarea id="nb-text" rows="3" maxlength="10000" placeholder="Текст или тезисы для разбора двумя голосами…" bind:value={nbText} disabled={busy}></textarea>
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
  </section>

  <div class="studio-stack">
    <section class="card">
      <h2>{@html icon("sparkles")} Мини-спор двух ассистентов</h2>
      <form class="studio-stack" onsubmit={submitDebate}>
        <div class="f">
          <label for="deb-topic">Тема спора</label>
          <input id="deb-topic" type="text" maxlength="300" placeholder="Например: Кофе или чай?" bind:value={debateTopic} disabled={busy}>
        </div>
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
    </section>

    <section class="card">
      <h2>{@html icon("moon")} История на ночь</h2>
      <form class="studio-stack" onsubmit={submitBedtime}>
        <div class="mode-row">
          <input type="text" maxlength="200" placeholder="Тема: уютный маяк у ночного океана" bind:value={bedtimeTopic} disabled={busy}>
          <button type="submit" class="btn primary" disabled={busy}>{@html icon("moon")} Слушать</button>
        </div>
      </form>
    </section>

    <section class="card">
      <h2>{@html icon("globe")} Синхронный переводчик</h2>
      <div class="mode-row">
        <select aria-label="Язык перевода" bind:value={transLang} disabled={busy}>
          {#each languages as [val, label] (val)}<option value={val}>{label}</option>{/each}
        </select>
        <button type="button" class="btn" class:primary={activeMode !== "translator"} disabled={busy} onclick={toggleTranslator}>
          {@html icon(activeMode === "translator" ? "stop" : "play", "solid")}
          {activeMode === "translator" ? "Выключить" : "Включить"}
        </button>
      </div>
    </section>
  </div>
</div>
