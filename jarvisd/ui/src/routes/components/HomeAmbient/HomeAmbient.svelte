<script>
  import { api, icon, jelly, toast } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";

  let { embedded = false } = $props();
  const controller = new AbortController();
  const { signal } = controller;
  const fallbackPresets = [
    { id: "rain", name: "Дождь", kind: "ambient", custom: false },
    { id: "fireplace", name: "Камин", kind: "ambient", custom: false },
    { id: "ocean", name: "Океан", kind: "ambient", custom: false },
    { id: "white", name: "Белый шум", kind: "ambient", custom: false },
    { id: "lofi", name: "Lo-Fi Radio", kind: "radio", custom: false },
    { id: "jazz", name: "Jazz Radio", kind: "radio", custom: false },
    { id: "chill", name: "Space Chill", kind: "radio", custom: false },
  ];

  let showAdd = $state(false);
  let newName = $state("");
  let newUrl = $state("");
  let busy = $state(false);
  let volSlider;
  let draggingVol = false;
  let localVol = $state(70);

  let amb = $derived($activity.ambient || {});
  let active = $derived(amb.active || "");
  let sleepS = $derived(Number(amb.sleep_s || 0));
  let presets = $derived(amb.presets?.length ? amb.presets : fallbackPresets);
  let fireplaceItem = $derived(presets.find((p) => p.id === "fireplace") || null);
  let otherPresets = $derived(presets.filter((p) => p.id !== "fireplace"));
  let displayVol = $derived(draggingVol ? localVol : (typeof amb.volume === "number" ? amb.volume : localVol));

  function presetIcon(item) {
    if (item.kind === "radio") return "radio";
    if (item.id === "fireplace") return "flame";
    if (item.id === "rain") return "droplets";
    if (item.id === "ocean") return "waves";
    if (item.id === "white") return "wind";
    return "cloud";
  }

  $effect(() => {
    if (!draggingVol && typeof amb.volume === "number" && volSlider) {
      volSlider.set(amb.volume, true);
    }
  });

  async function toggle(id) {
    if (busy) return;
    busy = true;
    const next = active === id ? "" : id;
    try {
      await api("POST", "/api/ambient", { id: next }, { signal });
      await refreshActivity();
      if (!next) toast("Фон и радио выключены");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function setSleep(mins) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/ambient", { sleep_min: mins }, { signal });
      await refreshActivity();
      toast(mins ? `Таймер сна: ${mins} мин` : "Таймер сна отменён");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function saveVolume(val) {
    try {
      await api("POST", "/api/ambient", { volume: val }, { signal });
      await refreshActivity();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    }
  }

  function mountVol(node) {
    volSlider = jelly(node, {
      label: "Громкость фона и радио", min: 5, max: 100, step: 5, format: (v) => `${v}%`, value: displayVol,
      onInput: (v) => {
        draggingVol = true;
        localVol = v;
      },
      onChange: (v) => {
        draggingVol = false;
        localVol = v;
        void saveVolume(v);
      },
    });
    return { destroy: () => { volSlider?.destroy(); volSlider = undefined; controller.abort(); } };
  }

  async function addStation(event) {
    event.preventDefault();
    if (busy) return;
    const name = newName.trim();
    const url = newUrl.trim();
    if (!name || !url) return toast("Укажите название и URL потока", true);
    busy = true;
    try {
      await api("POST", "/api/ambient/stations", { name, url }, { signal });
      await refreshActivity();
      newName = "";
      newUrl = "";
      showAdd = false;
      toast(`Станция «${name}» добавлена`);
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function removeStation(id) {
    if (busy) return;
    busy = true;
    try {
      await api("DELETE", "/api/ambient/stations/" + encodeURIComponent(id), undefined, { signal });
      await refreshActivity();
      toast("Станция удалена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  function formatSleep(sec) {
    return `${Math.ceil(sec / 60)} мин`;
  }
</script>

{#snippet body()}
  {#if showAdd}
    <form class="mode-drawer amb-add" onsubmit={addStation}>
      <input type="text" maxlength="40" placeholder="Название станции" bind:value={newName} disabled={busy}>
      <div class="mode-row">
        <input type="text" maxlength="300" placeholder="https://stream.example.com/radio.mp3" bind:value={newUrl} disabled={busy}>
        <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("plus")}</button>
      </div>
    </form>
  {/if}
  {#if fireplaceItem}
    <button
      type="button"
      class="hearth-hero"
      class:on={active === "fireplace"}
      disabled={busy}
      onclick={() => toggle("fireplace")}
    >
      <span class="hearth-icon">{@html icon("flame")}</span>
      <span class="hearth-meta">
        <b>{fireplaceItem.name}</b>
        <small>{active === "fireplace" ? "Горит · тёплый треск" : "Уютный треск дров"}</small>
      </span>
      <span class="hearth-pill">{active === "fireplace" ? "Горит" : "Зажечь"}</span>
    </button>
  {/if}
  <div class="amb-grid">
    {#each otherPresets as item (item.id)}
      <div class="amb-wrap">
        <button type="button" class="amb-btn" class:on={active === item.id} class:has-del={item.custom} disabled={busy} onclick={() => toggle(item.id)}>
          {@html icon(presetIcon(item))}
          <span>{item.name}</span>
        </button>
        {#if item.custom}
          <button type="button" class="amb-del" disabled={busy} onclick={() => removeStation(item.id)} aria-label={"Удалить " + item.name} title="Удалить станцию">
            {@html icon("x")}
          </button>
        {/if}
      </div>
    {/each}
  </div>
  <div class="amb-vol">
    <span class="lbl">
      Громкость
      <span class="amb-head-actions">
        <button class="btn sm ghost" type="button" onclick={() => { showAdd = !showAdd; }} title="Добавить радиопоток">
          {@html icon(showAdd ? "x" : "plus")} Радио
        </button>
        {#if active}
          <button class="btn sm ghost" type="button" disabled={busy} onclick={() => toggle(active)}>
            {@html icon("stop", "solid")} Стоп
          </button>
        {/if}
        <b class="val">{displayVol}%</b>
      </span>
    </span>
    <div use:mountVol role="slider" aria-label="Громкость фона и радио" aria-valuemin="5" aria-valuemax="100" aria-valuenow={displayVol}></div>
  </div>
  {#if active}
    <div class="amb-sleep">
      <span class="amb-sleep-lbl">{@html icon("moon")} {sleepS > 0 ? `Сон: ${formatSleep(sleepS)}` : "Таймер сна"}</span>
      <div class="amb-sleep-btns">
        {#each [15, 30, 60] as m (m)}
          <button type="button" class="btn sm ghost" disabled={busy} onclick={() => setSleep(m)}>{m}м</button>
        {/each}
        {#if sleepS > 0}
          <button type="button" class="btn sm ghost" disabled={busy} onclick={() => setSleep(0)} aria-label="Отменить таймер">{@html icon("x")}</button>
        {/if}
      </div>
    </div>
  {/if}
{/snippet}

{#if embedded}
  {@render body()}
{:else}
  <section class="card amb-card" aria-label="Камин и атмосфера">
    <h2>{@html icon("flame")} Камин и атмосфера</h2>
    <div class="amb-stack">
      {@render body()}
    </div>
  </section>
{/if}

<style>
  .amb-card h2 {
    margin-bottom: 14px;
  }
  .amb-stack {
    display: grid;
    gap: 14px;
  }
  .amb-stack :global(.amb-grid) {
    gap: 10px;
  }
  .amb-stack :global(.amb-vol),
  .amb-stack :global(.amb-sleep) {
    margin-top: 2px;
  }
</style>
