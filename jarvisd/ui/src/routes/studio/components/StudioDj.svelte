<script>
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";

  const presets = [
    "Ночной Токио",
    "Синтвейв 80-х",
    "Аниме OST",
    "Лоу-фай для кода",
    "Дарк Джаз",
    "Киберпанк",
  ];
  const vibeHints = ["Поспокойнее", "Энергичнее", "Больше электроники", "Атмосферный инди"];

  let mood = $state("ночной Токио синтвейв");
  let busy = $state(false);

  let dj = $derived($activity.dj || { active: false, station: "", current_title: "", mood: "", likes: [] });

  async function startDj(targetMood = mood) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/dj/start", { mood: targetMood.trim() || "ночной Токио синтвейв" });
      await refreshActivity();
      toast("AI-DJ вышел в эфир");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function djAction(action) {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", `/api/dj/${action}`, {});
      await refreshActivity();
      if (res?.message) toast(res.message);
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function tweakVibe(hint) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/dj/vibe", { hint });
      await refreshActivity();
      toast(`Подстройка: ${hint}`);
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }
</script>

<div class="studio-grid">
  <section class="card">
    <h2>{@html icon("disc")} Настройка волны AI-DJ</h2>
    <form class="studio-stack" onsubmit={(e) => { e.preventDefault(); startDj(mood); }}>
      <div class="quick-grid">
        {#each presets as p (p)}
          <button type="button" class="scen-btn" disabled={busy} onclick={() => { mood = p; startDj(p); }}>
            {@html icon("music")}
            <span>{p}</span>
          </button>
        {/each}
      </div>

      <div class="f">
        <label for="dj-mood">Свой запрос настроения</label>
        <div class="mode-row">
          <input id="dj-mood" type="text" maxlength="140" placeholder="Опишите вайб…" bind:value={mood} disabled={busy}>
          <button type="submit" class="btn primary" disabled={busy}>{@html icon("play", "solid")} Эфир</button>
        </div>
      </div>
    </form>
  </section>

  <section class="card">
    <div class="card-head">
      <h2>{@html icon("radio")} Текущий эфир</h2>
      {#if dj.active}
        <span class="badge ok">В эфире</span>
      {/if}
    </div>

    {#if dj.active}
      <div class="dj-bar">
        <div class="dj-info">
          <b>{dj.station || "AI-DJ"}</b>
          <small>{dj.current_title || dj.mood}</small>
        </div>
        <div class="dj-actions">
          <button type="button" class="ib" title="Лайк" aria-label="Лайк треку" disabled={busy} onclick={() => djAction("like")}>{@html icon("heart")}</button>
          <button type="button" class="ib" title="Следующий" aria-label="Следующий трек" disabled={busy} onclick={() => djAction("next")}>{@html icon("skip")}</button>
          <button type="button" class="ib danger" title="Стоп" aria-label="Остановить DJ" disabled={busy} onclick={() => djAction("stop")}>{@html icon("stop", "solid")}</button>
        </div>
      </div>

      <div class="f">
        <span class="lbl">Скорректировать волну</span>
        <div class="chip-row">
          {#each vibeHints as hint (hint)}
            <button type="button" class="btn sm ghost" disabled={busy} onclick={() => tweakVibe(hint)}>{hint}</button>
          {/each}
        </div>
      </div>

      {#if dj.likes?.length}
        <div class="f">
          <span class="lbl">Лайки этой сессии</span>
          <div class="chips">
            {#each dj.likes as track (track)}
              <span class="chip">{track}</span>
            {/each}
          </div>
        </div>
      {/if}
    {:else}
      <p class="empty">AI-DJ сейчас молчит — выберите пресет слева или опишите настроение.</p>
    {/if}
  </section>
</div>
