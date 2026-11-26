<script>
  import { onMount } from "svelte";
  import { api, icon, jelly, toast } from "#helpers/core.js";

  const controller = new AbortController();
  const { signal } = controller;
  let active = $state("");
  let sleepS = $state(0);
  let volume = $state(70);
  let showAdd = $state(false);
  let newName = $state("");
  let newUrl = $state("");
  let presets = $state.raw([
    { id: "rain", name: "Дождь", kind: "ambient", custom: false },
    { id: "fireplace", name: "Камин", kind: "ambient", custom: false },
    { id: "ocean", name: "Океан", kind: "ambient", custom: false },
    { id: "white", name: "Белый шум", kind: "ambient", custom: false },
    { id: "lofi", name: "Lo-Fi Radio", kind: "radio", custom: false },
    { id: "jazz", name: "Jazz Radio", kind: "radio", custom: false },
    { id: "chill", name: "Space Chill", kind: "radio", custom: false },
  ]);
  let busy = $state(false);
  let timer;
  let volSlider;
  let lastVol = 70;

  onMount(() => {
    void load();
    timer = setInterval(() => {
      if (sleepS > 0) sleepS = Math.max(0, sleepS - 1);
    }, 1000);
    return () => {
      clearInterval(timer);
      controller.abort();
    };
  });

  function applyStatus(data) {
    active = data.active || "";
    sleepS = Number(data.sleep_s || 0);
    if (typeof data.volume === "number") {
      volume = data.volume;
      if (volume !== lastVol) {
        lastVol = volume;
        volSlider?.set(volume, true);
      }
    }
    if (Array.isArray(data.presets) && data.presets.length) presets = data.presets;
  }

  async function load() {
    try {
      const data = await api("GET", "/api/ambient", undefined, { signal });
      if (!signal.aborted) applyStatus(data);
    } catch { /* ignore */ }
  }

  async function toggle(id) {
    if (busy) return;
    busy = true;
    const next = active === id ? "" : id;
    try {
      const data = await api("POST", "/api/ambient", { id: next }, { signal });
      if (signal.aborted) return;
      applyStatus(data);
      if (!active) toast("Фон и радио выключены");
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
      const data = await api("POST", "/api/ambient", { sleep_min: mins }, { signal });
      if (signal.aborted) return;
      applyStatus(data);
      toast(mins ? `Таймер сна: ${mins} мин` : "Таймер сна отменён");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  async function saveVolume(val) {
    try {
      const data = await api("POST", "/api/ambient", { volume: val }, { signal });
      if (!signal.aborted) applyStatus(data);
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    }
  }

  function mountVol(node) {
    volSlider = jelly(node, {
      label: "Громкость фона и радио", min: 5, max: 100, step: 5, format: (v) => `${v}%`, value: volume,
      onInput: (v) => {
        lastVol = v;
        volume = v;
      },
      onChange: (v) => {
        lastVol = v;
        volume = v;
        void saveVolume(v);
      },
    });
    return { destroy: () => { volSlider.destroy(); volSlider = undefined; } };
  }

  async function addStation(event) {
    event.preventDefault();
    if (busy) return;
    const name = newName.trim();
    const url = newUrl.trim();
    if (!name || !url) return toast("Укажите название и URL потока", true);
    busy = true;
    try {
      const data = await api("POST", "/api/ambient/stations", { name, url }, { signal });
      if (signal.aborted) return;
      applyStatus(data);
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
      const data = await api("DELETE", "/api/ambient/stations/" + encodeURIComponent(id), undefined, { signal });
      if (signal.aborted) return;
      applyStatus(data);
      toast("Станция удалена");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  function formatSleep(sec) {
    const m = Math.ceil(sec / 60);
    return `${m} мин`;
  }
</script>

<section class="card" aria-label="Фон и радио">
  <h2>
    {@html icon("radio")} Фон и радио
    <span class="amb-head-actions">
      <button class="btn sm ghost" type="button" onclick={() => { showAdd = !showAdd; }} title="Добавить радиопоток">
        {@html icon(showAdd ? "x" : "plus")} Станция
      </button>
      {#if active}
        <button class="btn sm ghost" type="button" disabled={busy} onclick={() => toggle(active)}>
          {@html icon("stop", "solid")} Стоп
        </button>
      {/if}
    </span>
  </h2>
  {#if showAdd}
    <form class="mode-drawer amb-add" onsubmit={addStation}>
      <input type="text" maxlength="40" placeholder="Название станции" bind:value={newName} disabled={busy}>
      <div class="mode-row">
        <input type="text" maxlength="300" placeholder="https://stream.example.com/radio.mp3" bind:value={newUrl} disabled={busy}>
        <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("plus")}</button>
      </div>
    </form>
  {/if}
  <div class="amb-grid">
    {#each presets as item (item.id)}
      <div class="amb-wrap">
        <button type="button" class="amb-btn" class:on={active === item.id} class:has-del={item.custom} disabled={busy} onclick={() => toggle(item.id)}>
          {@html icon(item.kind === "radio" ? "radio" : "cloud")}
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
    <span class="lbl">Громкость <b class="val">{volume}%</b></span>
    <div use:mountVol role="slider" aria-label="Громкость фона и радио" aria-valuemin="5" aria-valuemax="100" aria-valuenow={volume}></div>
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
</section>
