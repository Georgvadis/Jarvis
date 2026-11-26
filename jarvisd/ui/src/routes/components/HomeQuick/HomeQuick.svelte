<script>
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, fmtClock, refreshActivity, runActivityNext, runScenario, stopActivityItem, summarizeActivity } from "#state/activity.js";
  import { speaker } from "#state/panelState.js";

  let { overview } = $props();
  let busy = $state(false);
  let pcBusy = $state("");

  let completed = $derived(overview.steps.filter(({ done }) => done).length);
  let focusRunning = $derived(Boolean($activity.focus?.running));
  let focusRem = $derived(Number($activity.focus?.remaining_s || 0));
  let hasPc = $derived(Boolean($speaker?.settings?.pc_agent_url?.trim()));
  let liveItems = $derived(summarizeActivity($activity).filter((i) => i.id !== "focus" && i.id !== "ambient"));

  async function toggleFocus() {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", "/api/focus", { minutes: focusRunning ? 0 : 25 });
      await refreshActivity();
      toast(res.running ? "Фокус 25 мин запущен" : "Фокус остановлен");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function triggerScenario(id) {
    if (busy) return;
    busy = true;
    try {
      await runScenario(id);
    } finally {
      busy = false;
    }
  }

  async function quickTimer5m() {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/schedule/timer", { minutes: 5, label: "Таймер" });
      await refreshActivity();
      toast("Таймер на 5 мин");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function pcCmd(action) {
    if (pcBusy) return;
    pcBusy = action;
    try {
      if (action === "screen") {
        const res = await api("POST", "/api/pc/screen", { question: "Кратко опиши, что сейчас на экране." });
        toast(res.text || "Экран проанализирован");
      } else if (action === "clip_speak") {
        await api("POST", "/api/pc/clipboard", { speak: true });
        toast("Озвучиваю буфер ПК");
      } else {
        const res = await api("POST", "/api/pc/action", { action });
        toast(res.message || "Выполнено на ПК");
      }
    } catch (error) {
      toast(error.message, true);
    } finally {
      pcBusy = "";
    }
  }
</script>

{#if completed < overview.steps.length}
  <div class="card">
    <h2>{@html icon("sparkles")} Настройка <span class="count">{completed} из {overview.steps.length}</span></h2>
    <progress class="bar" value={completed} max={overview.steps.length} aria-label="Настройка колонки"></progress>
    {#each overview.steps as step (step.title)}
      <a class="step" class:done={step.done} href={step.href}>
        {@html icon(step.done ? "done" : "todo")}
        <span class="sb"><b>{step.title}</b><small>{step.text}</small></span>
        {#if !step.done}{@html icon("right")}{/if}
      </a>
    {/each}
  </div>
{/if}

<section class="card" aria-label="Быстрый пульт">
  <div class="quick-head">
    <h2>{@html icon("bolt")} Быстрый пульт</h2>
    <a class="quick-link" href="/studio">Студия {@html icon("right")}</a>
  </div>
  <div class="quick-grid">
    <button type="button" class="scen-btn" disabled={busy} onclick={() => triggerScenario("morning")}>
      {@html icon("sun")}
      <span>Утро</span>
    </button>
    <button type="button" class="scen-btn" disabled={busy} onclick={() => triggerScenario("work")}>
      {@html icon("briefcase")}
      <span>Работа</span>
    </button>
    <button type="button" class="scen-btn" disabled={busy} onclick={() => triggerScenario("evening")}>
      {@html icon("sparkles")}
      <span>Вечер</span>
    </button>
    <button type="button" class="scen-btn" disabled={busy} onclick={() => triggerScenario("sleep")}>
      {@html icon("moon")}
      <span>Сон</span>
    </button>
    {#each $activity.custom_scenarios || [] as sc (sc.id)}
      <button type="button" class="scen-btn" disabled={busy} onclick={() => triggerScenario(sc.id)}>
        {@html icon(sc.icon || "sparkles")}
        <span>{sc.name}</span>
      </button>
    {/each}
  </div>

  <div class="quick-util">
    <button type="button" class="btn sm ghost" class:on={focusRunning} disabled={busy} onclick={toggleFocus}>
      {@html icon("bolt")}
      <span>{focusRunning ? `Фокус ${fmtClock(focusRem)}` : "Фокус 25м"}</span>
    </button>
    <button type="button" class="btn sm ghost" disabled={busy} onclick={quickTimer5m}>
      {@html icon("clock")}
      <span>Таймер 5м</span>
    </button>
    <a class="btn sm ghost" href="/studio?tab=schedule" title="Расписание и таймеры" aria-label="Расписание и таймеры">
      {@html icon("bell")}
    </a>
  </div>

  {#if hasPc}
    <div class="pc-bar" aria-label="Управление ПК">
      <span class="pc-bar-lbl">{@html icon("monitor")} ПК</span>
      <div class="pc-bar-btns">
        <button type="button" class="ib" disabled={!!pcBusy} onclick={() => pcCmd("screen")} title="Посмотреть экран" aria-label="Посмотреть экран">
          {@html icon("eye")}
        </button>
        <button type="button" class="ib" disabled={!!pcBusy} onclick={() => pcCmd("clip_speak")} title="Озвучить буфер ПК" aria-label="Озвучить буфер ПК">
          {@html icon("clipboard")}
        </button>
        <button type="button" class="ib" disabled={!!pcBusy} onclick={() => pcCmd("media_play_pause")} title="Медиа плей/пауза" aria-label="Медиа плей/пауза">
          {@html icon("play", "solid")}
        </button>
        <button type="button" class="ib" disabled={!!pcBusy} onclick={() => pcCmd("screenshot_save")} title="Сохранить скриншот" aria-label="Сохранить скриншот">
          {@html icon("camera")}
        </button>
        <button type="button" class="ib" disabled={!!pcBusy} onclick={() => pcCmd("lock_screen")} title="Заблокировать экран ПК" aria-label="Заблокировать экран ПК">
          {@html icon("lock")}
        </button>
      </div>
    </div>
  {/if}

  {#if liveItems.length}
    <div class="quick-live">
      {#each liveItems as item (item.id)}
        <div class="live-row">
          <a class="live-link" href={item.href}>
            <b>{item.title}</b>
            {#if item.subtitle}<small>{item.subtitle}</small>{/if}
          </a>
          <div class="live-acts">
            {#if item.canNext}
              <button type="button" class="ib" title={item.nextLabel || "Далее"} aria-label={item.nextLabel || "Далее"} onclick={() => void runActivityNext(item)}>
                {@html icon("skip")}
              </button>
            {/if}
            <button type="button" class="ib danger" title="Остановить" aria-label="Остановить" onclick={() => void stopActivityItem(item)}>
              {@html icon("stop", "solid")}
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>
