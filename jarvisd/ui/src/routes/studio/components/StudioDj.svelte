<script>
  import { untrack } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";
  import { S, reload } from "#state/panelState.js";

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
  let savedPresets = $state(untrack(() => S.settings?.dj_presets || []));
  let busy = $state(false);

  let dj = $derived($activity.dj || { active: false, station: "", current_title: "", mood: "", likes: [], music_sleep_s: 0 });
  let musicSleepS = $derived(Number(dj.music_sleep_s || 0));

  async function saveDjPresets(nextList, msg) {
    if (busy) return;
    busy = true;
    try {
      await api("PUT", "/api/settings", { dj_presets: nextList });
      savedPresets = nextList;
      await reload();
      toast(msg);
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  function addPreset() {
    const clean = mood.trim();
    if (!clean) return toast("Введите название волны", true);
    if (savedPresets.includes(clean)) return toast("Такая волна уже сохранена");
    void saveDjPresets([...savedPresets, clean].slice(0, 24), `Волна «${clean}» сохранена`);
  }

  function removePreset(name) {
    void saveDjPresets(savedPresets.filter((x) => x !== name), "Волна удалена");
  }

  async function setMusicSleep(mins) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/music/sleep", { minutes: mins });
      await refreshActivity();
      toast(mins ? `Плавный сон музыки: ${mins} мин` : "Таймер сна отменён");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

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
        {#each savedPresets as p (p)}
          <div class="dj-saved-wrap">
            <button type="button" class="scen-btn" disabled={busy} onclick={() => { mood = p; startDj(p); }}>
              {@html icon("heart")}
              <span>{p}</span>
            </button>
            <button type="button" class="dj-saved-del" disabled={busy} onclick={() => removePreset(p)} title="Удалить волну" aria-label={`Удалить волну ${p}`}>
              {@html icon("x")}
            </button>
          </div>
        {/each}
      </div>

      <div class="f">
        <label for="dj-mood">Свой запрос настроения</label>
        <div class="mode-row">
          <input id="dj-mood" type="text" maxlength="140" placeholder="Опишите вайб…" bind:value={mood} disabled={busy}>
          <button type="button" class="btn ghost" disabled={busy || !mood.trim()} onclick={addPreset} title="Сохранить волну в избранное" aria-label="Сохранить волну">{@html icon("plus")}</button>
          <button type="submit" class="btn primary" disabled={busy}>{@html icon("play", "solid")} Эфир</button>
        </div>
      </div>

      <div class="amb-sleep">
        <span class="amb-sleep-lbl">{@html icon("moon")} {musicSleepS > 0 ? `Плавный сон: ${Math.ceil(musicSleepS / 60)} мин` : "Таймер сна музыки"}</span>
        <div class="amb-sleep-btns">
          {#each [15, 30, 60] as m (m)}
            <button type="button" class="btn sm ghost" disabled={busy} onclick={() => setMusicSleep(m)}>{m}м</button>
          {/each}
          {#if musicSleepS > 0}
            <button type="button" class="btn sm ghost danger" disabled={busy} onclick={() => setMusicSleep(0)}>{@html icon("x")}</button>
          {/if}
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
