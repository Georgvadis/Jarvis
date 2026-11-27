<script>
  import { onMount } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";

  const ICON_CHOICES = [
    { id: "flame", label: "Камин" },
    { id: "sparkles", label: "Искра" },
    { id: "sun", label: "Утро" },
    { id: "moon", label: "Ночь" },
    { id: "briefcase", label: "Работа" },
    { id: "coffee", label: "Кофе" },
    { id: "bolt", label: "Энергия" },
    { id: "music", label: "Музыка" },
    { id: "monitor", label: "ПК" },
    { id: "lock", label: "Защита" },
    { id: "radio", label: "Радио" },
    { id: "home", label: "Дом" },
    { id: "heart", label: "Уют" },
    { id: "gamepad", label: "Игры" },
  ];

  const STEP_KINDS = [
    { id: "stop_all", label: "Остановить всё" },
    { id: "volume", label: "Громкость колонки" },
    { id: "music", label: "Музыка (YouTube)" },
    { id: "ambient", label: "Фон / радио" },
    { id: "dj", label: "AI-DJ волна" },
    { id: "say", label: "Произнести фразу" },
    { id: "ai", label: "Спросить ИИ" },
    { id: "briefing", label: "Сводка дня" },
    { id: "alice", label: "Умный дом / Алиса" },
    { id: "led", label: "Подсветка / ночник" },
    { id: "mode", label: "Режим / игра" },
    { id: "focus", label: "Фокус-таймер" },
    { id: "timer", label: "Таймер / напоминание" },
    { id: "wait", label: "Пауза (сек)" },
    { id: "telegram", label: "Отправить в Telegram" },
    { id: "pc", label: "Действие на ПК" },
  ];

  const VALID_KIND_SET = new Set(STEP_KINDS.map((k) => k.id));
  const VALID_ICON_SET = new Set(ICON_CHOICES.map((c) => c.id));

  const LED_PRESETS = [
    { id: "#ff9900", label: "Тёплый янтарь" },
    { id: "#7c3aed", label: "Фиолетовый" },
    { id: "#00d2ff", label: "Неон голубой" },
    { id: "#ff4d6d", label: "Закат" },
    { id: "#10b981", label: "Изумруд" },
    { id: "off", label: "Выключить" },
  ];

  const MODE_PRESETS = [
    { id: "rpg", label: "RPG-квест" },
    { id: "quiz", label: "Викторина" },
    { id: "chat", label: "Свободный диалог" },
    { id: "translator:английский", label: "Переводчик (EN)" },
    { id: "normal", label: "Выключить режим" },
  ];

  const PC_ACTIONS = [
    { id: "media_play_pause", label: "Медиа: пауза / пуск" },
    { id: "media_next", label: "Медиа: следующий" },
    { id: "media_prev", label: "Медиа: предыдущий" },
    { id: "volume_up", label: "Громкость ПК +" },
    { id: "volume_down", label: "Громкость ПК −" },
    { id: "volume_mute", label: "Звук ПК вкл/выкл" },
    { id: "screenshot_save", label: "Скриншот на ПК" },
    { id: "lock_screen", label: "Заблокировать экран ПК" },
    { id: "sleep_timer", label: "Таймер сна ПК" },
    { id: "cancel_sleep", label: "Отменить сон ПК" },
    { id: "active_window", label: "Активное окно ПК" },
  ];

  const TEMPLATES = [
    {
      label: "Доброе утро",
      icon: "sun",
      phrase: "доброе утро",
      steps: [
        { kind: "volume", value: "", number: 5 },
        { kind: "briefing", value: "", number: null },
        { kind: "music", value: "бодрый утренний джаз", number: null },
      ],
    },
    {
      label: "Кинотеатр",
      icon: "flame",
      phrase: "включи кинотеатр",
      steps: [
        { kind: "alice", value: "выключи свет", number: null },
        { kind: "volume", value: "", number: 7 },
        { kind: "pc", value: "media_play_pause", number: null },
      ],
    },
    {
      label: "Фокус",
      icon: "briefcase",
      phrase: "пора работать",
      steps: [
        { kind: "volume", value: "", number: 3 },
        { kind: "ambient", value: "lofi", number: 60 },
        { kind: "focus", value: "", number: 25 },
      ],
    },
    {
      label: "Перед сном",
      icon: "moon",
      phrase: "спокойной ночи",
      steps: [
        { kind: "stop_all", value: "", number: null },
        { kind: "volume", value: "", number: 2 },
        { kind: "led", value: "#ff9900", number: 45 },
        { kind: "ambient", value: "rain", number: 60 },
        { kind: "pc", value: "lock_screen", number: null },
      ],
    },
    {
      label: "Ушёл из дома",
      icon: "home",
      phrase: "я ушёл",
      steps: [
        { kind: "stop_all", value: "", number: null },
        { kind: "alice", value: "выключи свет", number: null },
        { kind: "pc", value: "lock_screen", number: null },
        { kind: "telegram", value: "Режим «Ушёл из дома» активирован", number: null },
      ],
    },
  ];

  const FALLBACK_AMBIENT = [
    { id: "rain", name: "Дождь" },
    { id: "fireplace", name: "Камин" },
    { id: "ocean", name: "Океан" },
    { id: "white", name: "Белый шум" },
    { id: "lofi", name: "Lo-Fi Radio" },
    { id: "jazz", name: "Jazz Radio" },
    { id: "chill", name: "Space Chill" },
  ];

  let custom = $state([]);
  let loading = $state(true);
  let busy = $state(false);
  let genBusy = $state(false);
  let genPrompt = $state("");
  let editingId = $state("");
  let confirmDeleteId = $state("");
  let formEl = $state();

  let name = $state("");
  let phrase = $state("");
  let ic = $state("sparkles");
  let steps = $state([{ kind: "volume", value: "", number: 5 }]);

  let ambientOptions = $derived(
    Array.isArray($activity.ambient?.presets) && $activity.ambient.presets.length
      ? $activity.ambient.presets
      : FALLBACK_AMBIENT
  );

  onMount(() => {
    const ac = new AbortController();
    (async () => {
      try {
        const res = await api("GET", "/api/scenarios", undefined, { signal: ac.signal });
        if (Array.isArray(res?.custom)) custom = res.custom;
      } catch (err) {
        if (!ac.signal.aborted) toast(err.message || "Не удалось загрузить сценарии", true);
      } finally {
        if (!ac.signal.aborted) loading = false;
      }
    })();
    return () => ac.abort();
  });

  function resetForm() {
    editingId = "";
    name = "";
    phrase = "";
    ic = "sparkles";
    steps = [{ kind: "volume", value: "", number: 5 }];
  }

  function applyTemplate(tpl) {
    editingId = "";
    name = tpl.label;
    phrase = tpl.phrase;
    ic = tpl.icon;
    steps = tpl.steps.map((s) => ({ ...s }));
    formEl?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }

  function startEdit(sc) {
    confirmDeleteId = "";
    editingId = sc.id;
    name = sc.name || "";
    phrase = sc.phrase || "";
    ic = sc.icon || "sparkles";
    steps = Array.isArray(sc.steps) && sc.steps.length
      ? sc.steps.map((s) => ({ kind: s.kind, value: s.value || "", number: s.number ?? null }))
      : [{ kind: "stop_all", value: "", number: null }];
    formEl?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }

  function duplicateScenario(sc) {
    editingId = "";
    name = `${sc.name || "Сценарий"} (копия)`;
    phrase = "";
    ic = sc.icon || "sparkles";
    steps = Array.isArray(sc.steps) && sc.steps.length
      ? sc.steps.map((s) => ({ kind: s.kind, value: s.value || "", number: s.number ?? null }))
      : [{ kind: "stop_all", value: "", number: null }];
    formEl?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }

  function addStep() {
    if (steps.length >= 10) return;
    steps = [...steps, { kind: "say", value: "", number: null }];
  }

  function removeStep(idx) {
    if (steps.length <= 1) return;
    steps = steps.filter((_, i) => i !== idx);
  }

  function moveStep(idx, dir) {
    const next = idx + dir;
    if (next < 0 || next >= steps.length) return;
    const copy = [...steps];
    [copy[idx], copy[next]] = [copy[next], copy[idx]];
    steps = copy;
  }

  function changeKind(idx, kind) {
    const next = { kind, value: "", number: null };
    if (kind === "volume") next.number = 5;
    else if (kind === "ambient") next.value = ambientOptions[0]?.id || "rain";
    else if (kind === "dj") next.value = "Ночной Токио";
    else if (kind === "music") next.value = "";
    else if (kind === "focus") next.number = 25;
    else if (kind === "timer") { next.number = 5; next.value = "Таймер"; }
    else if (kind === "wait") next.number = 30;
    else if (kind === "led") { next.value = "#ff9900"; next.number = 30; }
    else if (kind === "mode") next.value = "rpg";
    else if (kind === "pc") next.value = "media_play_pause";
    steps = steps.map((s, i) => (i === idx ? next : s));
  }

  async function generateScenario() {
    if (genBusy || busy) return;
    genBusy = true;
    try {
      const res = await api("POST", "/api/scenarios/generate", { prompt: genPrompt.trim() });
      const rawSteps = Array.isArray(res?.steps) ? res.steps : [];
      const clean = rawSteps
        .filter((s) => s && VALID_KIND_SET.has(String(s.kind || "").toLowerCase()))
        .slice(0, 10)
        .map((s) => ({
          kind: String(s.kind).toLowerCase(),
          value: String(s.value ?? ""),
          number: s.number === null || s.number === undefined || s.number === "" ? null : Number(s.number),
        }));
      if (!clean.length) throw new Error("ИИ не вернул подходящих шагов");
      editingId = "";
      name = String(res?.name || "Новый сценарий").slice(0, 60);
      phrase = String(res?.phrase || "").slice(0, 80);
      ic = VALID_ICON_SET.has(res?.icon) ? res.icon : "sparkles";
      steps = clean;
      toast("Сценарий собран — проверьте и сохраните");
    } catch (err) {
      toast(err.message || "Не удалось собрать сценарий", true);
    } finally {
      genBusy = false;
    }
  }

  async function saveScenario(event) {
    event.preventDefault();
    if (!name.trim()) {
      toast("Укажите название сценария", true);
      return;
    }
    if (busy) return;
    busy = true;
    try {
      const cleanSteps = steps.map((s) => ({
        kind: s.kind,
        value: String(s.value || "").trim(),
        number: s.number === "" || s.number === null || s.number === undefined ? null : Number(s.number),
      }));
      const body = {
        name: name.trim(),
        phrase: phrase.trim(),
        icon: ic,
        steps: cleanSteps,
      };
      const wasEdit = Boolean(editingId);
      const res = wasEdit
        ? await api("PUT", `/api/scenarios/${encodeURIComponent(editingId)}`, body)
        : await api("POST", "/api/scenarios", body);
      if (Array.isArray(res?.custom)) custom = res.custom;
      await refreshActivity();
      resetForm();
      toast(wasEdit ? "Сценарий обновлён" : "Сценарий создан");
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function runCustom(sc) {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", "/api/scenario", { id: sc.id });
      await refreshActivity();
      toast(res?.message || `Сценарий «${sc.name}» запущен`);
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function deleteScenario(id) {
    if (busy) return;
    busy = true;
    try {
      const res = await api("DELETE", `/api/scenarios/${encodeURIComponent(id)}`);
      if (Array.isArray(res?.custom)) custom = res.custom;
      confirmDeleteId = "";
      if (editingId === id) resetForm();
      await refreshActivity();
      toast("Сценарий удалён");
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }
</script>

<section class="card scen-builder">
  <div class="card-head">
    <h2>{@html icon("wand")} Конструктор сценариев</h2>
    {#if custom.length}
      <span class="badge">{custom.length}</span>
    {/if}
  </div>

  <div class="scen-assist">
    <form class="row" onsubmit={(e) => { e.preventDefault(); generateScenario(); }}>
      <input
        type="text"
        maxlength="160"
        placeholder="Опишите сценарий для ИИ (напр.: перед сном тихо, дождь на час и выключи свет)…"
        bind:value={genPrompt}
        disabled={busy || genBusy}
      >
      <button type="submit" class="btn ghost" disabled={busy || genBusy}>
        {@html icon("sparkles")} {genBusy ? "Собираю…" : "Собрать ИИ"}
      </button>
    </form>

    <div class="presets">
      {#each TEMPLATES as tpl (tpl.label)}
        <button type="button" class="chip" disabled={busy || genBusy} onclick={() => applyTemplate(tpl)}>
          {@html icon(tpl.icon)} {tpl.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="studio-grid">
    <form class="sched-form" bind:this={formEl} onsubmit={saveScenario}>
      <div class="row sched-row-fields">
        <div class="f">
          <label for="sc-name">Название</label>
          <input id="sc-name" type="text" maxlength="60" placeholder="Напр: Кинотеатр" bind:value={name} disabled={busy}>
        </div>
        <div class="f">
          <label for="sc-phrase">Голосовая фраза (необяз.)</label>
          <input id="sc-phrase" type="text" maxlength="80" placeholder="включи кинотеатр" bind:value={phrase} disabled={busy}>
        </div>
      </div>

      <div class="f">
        <span class="lbl">Иконка</span>
        <div class="icon-picker">
          {#each ICON_CHOICES as choice (choice.id)}
            <button
              type="button"
              class="icon-pick"
              class:active={ic === choice.id}
              title={choice.label}
              aria-label={choice.label}
              disabled={busy}
              onclick={() => { ic = choice.id; }}
            >
              {@html icon(choice.id)}
            </button>
          {/each}
        </div>
      </div>

      <div class="f">
        <div class="card-head">
          <span class="lbl">Шаги ({steps.length}/10)</span>
          {#if steps.length < 10}
            <button type="button" class="btn sm ghost" disabled={busy} onclick={addStep}>
              {@html icon("plus")} Добавить шаг
            </button>
          {/if}
        </div>

        <div class="sched-list">
          {#each steps as step, idx (idx)}
            <div class="step-row">
              <div class="step-fields">
                <select
                  aria-label={"Тип шага " + (idx + 1)}
                  value={step.kind}
                  disabled={busy}
                  onchange={(e) => changeKind(idx, e.currentTarget.value)}
                >
                  {#each STEP_KINDS as k (k.id)}
                    <option value={k.id}>{k.label}</option>
                  {/each}
                </select>

                {#if step.kind === "volume"}
                  <input type="number" min="0" max="10" aria-label="Громкость" bind:value={step.number} disabled={busy} placeholder="0..10">
                {:else if step.kind === "music"}
                  <input type="text" maxlength="160" aria-label="Запрос музыки" bind:value={step.value} disabled={busy} placeholder="Трек, артист или жанр">
                {:else if step.kind === "ambient"}
                  <select aria-label="Фоновый звук" bind:value={step.value} disabled={busy}>
                    {#each ambientOptions as amb (amb.id)}
                      <option value={amb.id}>{amb.name || amb.label}</option>
                    {/each}
                  </select>
                  <input type="number" min="1" max="720" aria-label="Таймер сна мин" bind:value={step.number} disabled={busy} placeholder="Сон мин">
                {:else if step.kind === "focus"}
                  <input type="number" min="1" max="99" aria-label="Минуты фокуса" bind:value={step.number} disabled={busy} placeholder="Минуты">
                {:else if step.kind === "timer"}
                  <input type="number" min="1" max="1440" aria-label="Минуты таймера" bind:value={step.number} disabled={busy} placeholder="Мин">
                  <input type="text" maxlength="80" aria-label="Метка таймера" bind:value={step.value} disabled={busy} placeholder="Напоминание">
                {:else if step.kind === "wait"}
                  <input type="number" min="1" max="7200" aria-label="Пауза в секундах" bind:value={step.number} disabled={busy} placeholder="Секунды">
                {:else if step.kind === "dj"}
                  <input type="text" maxlength="120" aria-label="Настроение DJ" bind:value={step.value} disabled={busy} placeholder="Вайб волны…">
                {:else if step.kind === "say"}
                  <input type="text" maxlength="300" aria-label="Фраза" bind:value={step.value} disabled={busy} placeholder="Что сказать на колонке">
                {:else if step.kind === "ai"}
                  <input type="text" maxlength="300" aria-label="Запрос к ИИ" bind:value={step.value} disabled={busy} placeholder="Промпт для ИИ (ответ озвучится)">
                {:else if step.kind === "briefing"}
                  <input type="text" maxlength="200" aria-label="Пожелание к сводке" bind:value={step.value} disabled={busy} placeholder="Акцент сводки (необяз.)">
                {:else if step.kind === "alice"}
                  <input type="text" maxlength="160" aria-label="Команда умному дому" bind:value={step.value} disabled={busy} placeholder="Напр: выключи свет в спальне">
                {:else if step.kind === "telegram"}
                  <input type="text" maxlength="300" aria-label="Сообщение в Telegram" bind:value={step.value} disabled={busy} placeholder="Текст уведомления">
                {:else if step.kind === "led"}
                  <select aria-label="Цвет подсветки" bind:value={step.value} disabled={busy}>
                    {#each LED_PRESETS as c (c.id)}
                      <option value={c.id}>{c.label}</option>
                    {/each}
                  </select>
                  {#if step.value !== "off"}
                    <input type="number" min="1" max="480" aria-label="Минуты подсветки" bind:value={step.number} disabled={busy} placeholder="Мин">
                  {/if}
                {:else if step.kind === "mode"}
                  <select aria-label="Режим" bind:value={step.value} disabled={busy}>
                    {#each MODE_PRESETS as m (m.id)}
                      <option value={m.id}>{m.label}</option>
                    {/each}
                  </select>
                {:else if step.kind === "pc"}
                  <select aria-label="Действие на ПК" bind:value={step.value} disabled={busy}>
                    {#each PC_ACTIONS as act (act.id)}
                      <option value={act.id}>{act.label}</option>
                    {/each}
                  </select>
                  {#if step.value === "sleep_timer"}
                    <input type="number" min="1" max="720" aria-label="Минуты до сна ПК" bind:value={step.number} disabled={busy} placeholder="Мин">
                  {/if}
                {/if}
              </div>

              <div class="step-ctl">
                {#if steps.length > 1}
                  <button
                    type="button"
                    class="btn ghost sm"
                    disabled={busy || idx === 0}
                    aria-label="Выше"
                    onclick={() => moveStep(idx, -1)}
                  >
                    {@html icon("chevronUp")}
                  </button>
                  <button
                    type="button"
                    class="btn ghost sm"
                    disabled={busy || idx === steps.length - 1}
                    aria-label="Ниже"
                    onclick={() => moveStep(idx, 1)}
                  >
                    {@html icon("chevronDown")}
                  </button>
                  <button
                    type="button"
                    class="btn ghost sm"
                    disabled={busy}
                    aria-label="Удалить шаг"
                    onclick={() => removeStep(idx)}
                  >
                    {@html icon("x")}
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </div>

      <div class="row">
        <button type="submit" class="btn primary" disabled={busy}>
          {@html icon(editingId ? "save" : "plus")} {editingId ? "Сохранить сценарий" : "Создать сценарий"}
        </button>
        {#if editingId}
          <button type="button" class="btn ghost" disabled={busy} onclick={resetForm}>
            {@html icon("x")} Отмена
          </button>
        {/if}
      </div>
    </form>

    <div>
      {#if loading}
        <p class="empty-msg">Загрузка…</p>
      {:else if custom.length}
        <div class="sched-list">
          {#each custom as sc (sc.id)}
            <div class="sched-row">
              <span class="timer-ic">{@html icon(sc.icon || "sparkles")}</span>
              <div class="sched-info">
                <div class="sched-top">
                  <strong>{sc.name}</strong>
                  <span class="badge">{sc.steps?.length || 0} шаг.</span>
                </div>
                {#if sc.phrase}
                  <div class="sched-meta"><span>«{sc.phrase}»</span></div>
                {/if}
              </div>
              <div class="sched-actions">
                {#if confirmDeleteId === sc.id}
                  <button type="button" class="btn danger sm" disabled={busy} title="Подтвердить удаление" onclick={() => deleteScenario(sc.id)}>
                    {@html icon("check")}
                  </button>
                  <button type="button" class="btn ghost sm" disabled={busy} title="Отмена" onclick={() => { confirmDeleteId = ""; }}>
                    {@html icon("x")}
                  </button>
                {:else}
                  <button type="button" class="btn ghost sm" disabled={busy} title="Запустить" onclick={() => runCustom(sc)}>
                    {@html icon("play", "solid")}
                  </button>
                  <button type="button" class="btn ghost sm" disabled={busy} title="Изменить" onclick={() => startEdit(sc)}>
                    {@html icon("edit")}
                  </button>
                  <button type="button" class="btn ghost sm" disabled={busy} title="Дублировать" onclick={() => duplicateScenario(sc)}>
                    {@html icon("copy")}
                  </button>
                  <button type="button" class="btn ghost danger sm" disabled={busy} title="Удалить" onclick={() => { confirmDeleteId = sc.id; }}>
                    {@html icon("trash")}
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <p class="empty-msg">Собственных сценариев пока нет</p>
      {/if}
    </div>
  </div>
</section>

<style>
  .scen-assist {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 14px;
    padding-bottom: 14px;
    border-bottom: 1px solid var(--border);
  }
  .scen-assist .row {
    gap: 8px;
  }
  .scen-assist input {
    flex: 1;
    min-width: 0;
  }
  .step-ctl {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }
</style>
