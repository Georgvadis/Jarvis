<script>
  import { onMount } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, fmtClock, refreshActivity } from "#state/activity.js";

  const QUICK_TIMERS = [
    { minutes: 3, label: "3 мин" },
    { minutes: 5, label: "5 мин" },
    { minutes: 10, label: "10 мин" },
    { minutes: 15, label: "15 мин" },
    { minutes: 30, label: "30 мин" },
    { minutes: 60, label: "1 час" },
  ];

  const DAYS = [
    { id: 1, label: "Пн" },
    { id: 2, label: "Вт" },
    { id: 3, label: "Ср" },
    { id: 4, label: "Чт" },
    { id: 5, label: "Пт" },
    { id: 6, label: "Сб" },
    { id: 7, label: "Вс" },
  ];

  const ACTION_META = {
    scenario: { label: "Сценарий", placeholder: "morning / work / relax / sleep" },
    briefing: { label: "Сводка дня", placeholder: "Необязательно: акцент сводки" },
    say: { label: "Фраза", placeholder: "Что произнести вслух" },
    dj: { label: "AI-DJ", placeholder: "Вайб волны, например: Ночной Токио" },
    ambient: { label: "Фоновый звук", placeholder: "rain / forest / fireplace / ocean" },
  };

  const SCENARIO_PRESETS = [
    { id: "morning", label: "Утро" },
    { id: "work", label: "Работа" },
    { id: "relax", label: "Релакс" },
    { id: "sleep", label: "Сон" },
  ];

  const AMBIENT_PRESETS = [
    { id: "rain", label: "Дождь" },
    { id: "forest", label: "Лес" },
    { id: "fireplace", label: "Камин" },
    { id: "ocean", label: "Океан" },
    { id: "cafe", label: "Кафе" },
    { id: "space", label: "Космос" },
  ];

  let routines = $state([]);
  let loading = $state(true);
  let busy = $state(false);
  let confirmDeleteId = $state("");

  let timerMinutes = $state(5);
  let timerLabel = $state("");

  let rTime = $state("08:00");
  let rTitle = $state("");
  let rDays = $state([1, 2, 3, 4, 5]);
  let rAction = $state("scenario");
  let rPayload = $state("morning");
  let rVolume = $state("");

  let timers = $derived(Array.isArray($activity.timers) ? $activity.timers : []);

  function applyStatus(data) {
    if (Array.isArray(data?.routines)) {
      routines = data.routines;
    }
  }

  onMount(() => {
    const ac = new AbortController();
    (async () => {
      try {
        const data = await api("GET", "/api/schedule", undefined, { signal: ac.signal });
        applyStatus(data);
        await refreshActivity();
      } catch (err) {
        if (!ac.signal.aborted) toast(err.message || "Не удалось загрузить расписание", true);
      } finally {
        if (!ac.signal.aborted) loading = false;
      }
    })();
    return () => ac.abort();
  });

  function formatDays(days) {
    if (!Array.isArray(days) || days.length === 0 || days.length === 7) return "Ежедневно";
    const key = [...days].sort((a, b) => a - b).join(",");
    if (key === "1,2,3,4,5") return "Будни";
    if (key === "6,7") return "Выходные";
    return DAYS.filter((d) => days.includes(d.id)).map((d) => d.label).join(", ");
  }

  function toggleDay(dayId) {
    if (rDays.includes(dayId)) {
      rDays = rDays.filter((d) => d !== dayId);
    } else {
      rDays = [...rDays, dayId].sort((a, b) => a - b);
    }
  }

  function handleActionChange() {
    if (rAction === "scenario") rPayload = "morning";
    else if (rAction === "dj") rPayload = "Ночной Токио";
    else if (rAction === "ambient") rPayload = "rain";
    else rPayload = "";
  }

  async function addQuickTimer(minutes) {
    if (busy) return;
    busy = true;
    try {
      const data = await api("POST", "/api/schedule/timer", {
        minutes,
        label: timerLabel.trim() || "Таймер",
      });
      applyStatus(data);
      await refreshActivity();
      toast(`Таймер на ${minutes} мин`);
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function addCustomTimer() {
    const mins = Number(timerMinutes);
    if (!Number.isFinite(mins) || mins < 1 || mins > 1440) {
      toast("Укажите минуты от 1 до 1440", true);
      return;
    }
    if (busy) return;
    busy = true;
    try {
      const label = timerLabel.trim() || "Напоминание";
      const data = await api("POST", "/api/schedule/timer", {
        minutes: Math.round(mins),
        label,
      });
      applyStatus(data);
      timerLabel = "";
      await refreshActivity();
      toast(`Запущено: ${label}`);
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function cancelTimer(id) {
    if (busy) return;
    busy = true;
    try {
      const data = await api("DELETE", `/api/schedule/timer/${encodeURIComponent(id)}`);
      applyStatus(data);
      await refreshActivity();
      toast("Таймер снят");
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function createRoutine() {
    if (!rTime) {
      toast("Укажите время", true);
      return;
    }
    if (rAction === "say" && !rPayload.trim()) {
      toast("Введите фразу для озвучки", true);
      return;
    }
    if (busy) return;
    busy = true;
    try {
      const volNum = rVolume === "" ? null : Number(rVolume);
      const data = await api("POST", "/api/schedule/routine", {
        title: rTitle.trim(),
        time: rTime,
        days: rDays,
        action: rAction,
        payload: rPayload.trim(),
        volume: Number.isFinite(volNum) ? volNum : null,
      });
      applyStatus(data);
      rTitle = "";
      toast("Рутина добавлена");
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function toggleRoutine(r) {
    if (busy) return;
    busy = true;
    try {
      const data = await api("POST", `/api/schedule/routine/${encodeURIComponent(r.id)}/toggle`, {
        enabled: !r.enabled,
      });
      applyStatus(data);
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function runRoutineNow(r) {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", `/api/schedule/routine/${encodeURIComponent(r.id)}/run`, {});
      await refreshActivity();
      toast(res?.message || `Запущено: ${r.title}`);
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function deleteRoutine(id) {
    if (busy) return;
    busy = true;
    try {
      const data = await api("DELETE", `/api/schedule/routine/${encodeURIComponent(id)}`);
      applyStatus(data);
      confirmDeleteId = "";
      toast("Рутина удалена");
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }
</script>

<div class="studio-grid">
  <section class="card">
    <div class="card-head">
      <h2>{@html icon("clock")} Таймеры и напоминания</h2>
      {#if timers.length}
        <span class="badge ok">{timers.length} акт.</span>
      {/if}
    </div>

    <div class="studio-stack">
      <div class="quick-grid">
        {#each QUICK_TIMERS as qt (qt.minutes)}
          <button
            type="button"
            class="scen-btn"
            disabled={busy}
            onclick={() => addQuickTimer(qt.minutes)}
          >
            {@html icon("clock")}
            <span>{qt.label}</span>
          </button>
        {/each}
      </div>

      <form class="sched-form" onsubmit={(e) => { e.preventDefault(); addCustomTimer(); }}>
        <div class="row sched-row-fields">
          <div class="f sched-min-f">
            <label for="sched-t-min">Минуты</label>
            <input
              id="sched-t-min"
              type="number"
              min="1"
              max="1440"
              step="1"
              bind:value={timerMinutes}
              disabled={busy}
            >
          </div>
          <div class="f">
            <label for="sched-t-lbl">Напоминание</label>
            <input
              id="sched-t-lbl"
              type="text"
              maxlength="80"
              placeholder="Например: Проверить духовку"
              bind:value={timerLabel}
              disabled={busy}
            >
          </div>
        </div>
        <button type="submit" class="btn primary" disabled={busy}>
          {@html icon("bell")} Запустить таймер
        </button>
      </form>

      {#if timers.length}
        <div class="sched-list">
          {#each timers as t (t.id)}
            <div class="timer-row">
              <span class="timer-ic">{@html icon("clock")}</span>
              <div class="timer-main">
                <strong>{t.label}</strong>
              </div>
              <span class="badge ok timer-clock">{fmtClock(t.remaining_s)}</span>
              <button
                type="button"
                class="btn ghost sm"
                disabled={busy}
                aria-label="Отменить таймер"
                onclick={() => cancelTimer(t.id)}
              >
                {@html icon("x")}
              </button>
            </div>
          {/each}
        </div>
      {:else}
        <p class="empty-msg">Активных таймеров нет</p>
      {/if}
    </div>
  </section>

  <section class="card">
    <div class="card-head">
      <h2>{@html icon("bell")} Рутины по расписанию</h2>
      {#if routines.length}
        <span class="badge">{routines.length}</span>
      {/if}
    </div>

    <div class="studio-stack">
      <form class="sched-form" onsubmit={(e) => { e.preventDefault(); createRoutine(); }}>
        <div class="row sched-row-fields">
          <div class="f sched-time-f">
            <label for="sched-r-time">Время</label>
            <input id="sched-r-time" type="time" bind:value={rTime} disabled={busy}>
          </div>
          <div class="f">
            <label for="sched-r-title">Название</label>
            <input
              id="sched-r-title"
              type="text"
              maxlength="80"
              placeholder="Напр: Доброе утро"
              bind:value={rTitle}
              disabled={busy}
            >
          </div>
        </div>

        <div class="f">
          <span class="lbl">Дни недели</span>
          <div class="day-pills">
            {#each DAYS as d (d.id)}
              <button
                type="button"
                class="day-pill"
                class:active={rDays.includes(d.id)}
                disabled={busy}
                onclick={() => toggleDay(d.id)}
              >
                {d.label}
              </button>
            {/each}
            <button
              type="button"
              class="day-pill ghost-pill"
              class:active={rDays.length === 0 || rDays.length === 7}
              disabled={busy}
              onclick={() => { rDays = []; }}
            >
              Все
            </button>
            <button
              type="button"
              class="day-pill ghost-pill"
              class:active={rDays.join(",") === "1,2,3,4,5"}
              disabled={busy}
              onclick={() => { rDays = [1, 2, 3, 4, 5]; }}
            >
              Будни
            </button>
          </div>
        </div>

        <div class="row sched-row-fields">
          <div class="f">
            <label for="sched-r-act">Действие</label>
            <select
              id="sched-r-act"
              bind:value={rAction}
              onchange={handleActionChange}
              disabled={busy}
            >
              {#each Object.entries(ACTION_META) as [val, meta] (val)}
                <option value={val}>{meta.label}</option>
              {/each}
            </select>
          </div>
          <div class="f sched-vol-f">
            <label for="sched-r-vol">Громкость</label>
            <select id="sched-r-vol" bind:value={rVolume} disabled={busy}>
              <option value="">Текущая</option>
              {#each [2, 3, 4, 5, 6, 7, 8, 9, 10] as v (v)}
                <option value={String(v)}>{v} / 10</option>
              {/each}
            </select>
          </div>
        </div>

        {#if rAction === "scenario"}
          <div class="f">
            <label for="sched-r-scen">Сценарий</label>
            <select id="sched-r-scen" bind:value={rPayload} disabled={busy}>
              {#each SCENARIO_PRESETS as sc (sc.id)}
                <option value={sc.id}>{sc.label}</option>
              {/each}
            </select>
          </div>
        {:else if rAction === "ambient"}
          <div class="f">
            <label for="sched-r-amb">Звуковой фон</label>
            <select id="sched-r-amb" bind:value={rPayload} disabled={busy}>
              {#each AMBIENT_PRESETS as amb (amb.id)}
                <option value={amb.id}>{amb.label}</option>
              {/each}
            </select>
          </div>
        {:else if rAction !== "briefing"}
          <div class="f">
            <label for="sched-r-pay">Параметры</label>
            <input
              id="sched-r-pay"
              type="text"
              maxlength="300"
              placeholder={ACTION_META[rAction]?.placeholder || ""}
              bind:value={rPayload}
              disabled={busy}
            >
          </div>
        {/if}

        <button type="submit" class="btn primary" disabled={busy}>
          {@html icon("plus")} Добавить рутину
        </button>
      </form>

      {#if loading}
        <p class="empty-msg">Загрузка…</p>
      {:else if routines.length}
        <div class="sched-list">
          {#each routines as r (r.id)}
            <div class="sched-row" class:dimmed={!r.enabled}>
              <label class="sw" title={r.enabled ? "Отключить" : "Включить"}>
                <input
                  type="checkbox"
                  checked={r.enabled}
                  disabled={busy}
                  onchange={() => toggleRoutine(r)}
                >
                <span class="tr"></span>
              </label>

              <div class="sched-info">
                <div class="sched-top">
                  <span class="sched-time">{r.time}</span>
                  <strong>{r.title}</strong>
                  {#if r.volume}
                    <span class="badge">гр. {r.volume}</span>
                  {/if}
                </div>
                <div class="sched-meta">
                  <span>{ACTION_META[r.action]?.label || r.action}</span>
                  {#if r.payload}
                    <span>· {r.payload}</span>
                  {/if}
                  <span>· {formatDays(r.days)}</span>
                </div>
              </div>

              <div class="sched-actions">
                {#if confirmDeleteId === r.id}
                  <button
                    type="button"
                    class="btn danger sm"
                    disabled={busy}
                    title="Подтвердить удаление"
                    onclick={() => deleteRoutine(r.id)}
                  >
                    {@html icon("check")}
                  </button>
                  <button
                    type="button"
                    class="btn ghost sm"
                    disabled={busy}
                    title="Отмена"
                    onclick={() => { confirmDeleteId = ""; }}
                  >
                    {@html icon("x")}
                  </button>
                {:else}
                  <button
                    type="button"
                    class="btn ghost sm"
                    disabled={busy}
                    title="Запустить сейчас"
                    onclick={() => runRoutineNow(r)}
                  >
                    {@html icon("play", "solid")}
                  </button>
                  <button
                    type="button"
                    class="btn ghost danger sm"
                    disabled={busy}
                    title="Удалить"
                    onclick={() => { confirmDeleteId = r.id; }}
                  >
                    {@html icon("trash")}
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <p class="empty-msg">Нет запланированных рутин</p>
      {/if}
    </div>
  </section>
</div>
