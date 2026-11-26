<script module>
  const swatches = ["#ffffff", "#4fd1c5", "#00b4ff", "#3d6bff", "#5a2dff", "#b04dff", "#ff4d8d", "#ff5a36", "#ffb020", "#3ddc84"];
  const modes = [
    ["listening", "Слушает", "кольцо плавно загорается и дышит"],
    ["thinking", "Думает", "огонёк бежит по кругу"],
    ["speaking", "Говорит", "вспыхивает в такт голосу"],
  ];
</script>

<script>
  import { onMount, untrack } from "svelte";
  import { api, icon, jelly } from "#helpers/core.js";

  let { initial, previewAvailable, globallyEnabled, onedit } = $props();
  const controller = new AbortController();
  const { signal } = controller;
  let led = $state({ ...untrack(() => initial) });
  let intensity = $derived(0.35 + 0.65 * led.brightness / 100);
  let brightSlider;
  let lastBright = untrack(() => led.brightness);

  onMount(() => () => controller.abort());

  $effect(() => {
    const v = led.brightness;
    if (v !== lastBright) {
      lastBright = v;
      brightSlider?.set(v, true);
    }
  });

  function edit(field, value) {
    if (led[field] === value) return;
    if (field === "brightness") lastBright = value;
    led[field] = value;
    onedit({ [field]: value });
  }

  function preview(mode) {
    if (!previewAvailable) return;
    void api("POST", "/api/leds/preview", { mode, color: led[mode], brightness: led.brightness }, { signal }).catch(() => {});
  }

  function chooseColor(mode, color) {
    edit(mode, color);
    preview(mode);
  }

  function mountBrightness(node) {
    brightSlider = jelly(node, {
      label: "Яркость", min: 5, max: 100, step: 5, format: (v) => `${v}%`, value: led.brightness,
      onInput: (v) => edit("brightness", v),
      onChange: (v) => { edit("brightness", v); preview("listening"); },
    });
    return { destroy: () => { brightSlider.destroy(); brightSlider = undefined; } };
  }
</script>

<section class="sec" id="profile-lighting">
  <div class="sec-h">
    <h2>{@html icon("sun")}Подсветка</h2>
    <p>Цвет кольца этого ассистента</p>
  </div>
  <div class="sec-b">
    <div id="p-led" class="ledrows">
      {#each modes as [mode, title, hint] (mode)}
        <div class="ledrow" data-k={mode}>
          <span class={"ledring " + mode} data-ring={"p-" + mode} style:--c={led[mode]} style:--b={intensity}></span>
          <div class="ledname"><b>{title}</b><small>{hint}</small></div>
          <div class="swatches" role="group" aria-label={title}>
            {#each swatches as color (color)}
              <button type="button" class="swatch" class:on={led[mode] === color} data-c={color} data-k={mode}
                style:--c={color} aria-label={"Цвет " + color} aria-pressed={led[mode] === color}
                onclick={() => chooseColor(mode, color)}></button>
            {/each}
            <label class="swatch custom" class:on={!swatches.includes(led[mode])} style:--c={led[mode]}>
              <input type="color" data-k={mode} aria-label={"Свой цвет · " + title} value={led[mode]}
                oninput={(event) => edit(mode, event.currentTarget.value)}
                onchange={(event) => chooseColor(mode, event.currentTarget.value)}>
            </label>
          </div>
        </div>
      {/each}
    </div>
    <div class="f">
      <span class="lbl">Яркость <b id="p-br-v" class="val">{led.brightness}%</b></span>
      <div id="p-br" use:mountBrightness role="slider" aria-label="Яркость" aria-valuemin="5" aria-valuemax="100" aria-valuenow={led.brightness}></div>
    </div>
    {#if !globallyEnabled}
      <p class="note">Сейчас подсветка выключена для всех — включается на странице «Колонка».</p>
    {/if}
  </div>
</section>
