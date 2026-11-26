<script>
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, fmtClock, refreshActivity, runActivityNext, runScenario, stopActivityItem, summarizeActivity } from "#state/activity.js";

  let { overview } = $props();
  let busy = $state(false);

  let completed = $derived(overview.steps.filter(({ done }) => done).length);
  let focusRunning = $derived(Boolean($activity.focus?.running));
  let focusRem = $derived(Number($activity.focus?.remaining_s || 0));
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
    <button type="button" class="scen-btn" class:on={focusRunning} disabled={busy} onclick={toggleFocus}>
      {@html icon("bolt")}
      <span>{focusRunning ? `Фокус ${fmtClock(focusRem)}` : "Фокус 25м"}</span>
    </button>
  </div>

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
