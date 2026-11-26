<script>
  import { onMount } from "svelte";
  import { Motion, icon } from "#helpers/core.js";
  import { mountVolume, setVolume } from "#state/player.js";

  const ticks = Array.from({ length: 11 }, (_, index) => index);
  let slider = $state();
  let level = $state(null);
  let lastNonZero = 5;
  let band = $derived(level === null ? undefined : level === 0 ? "mute" : level <= 3 ? "low" : level <= 7 ? "mid" : "high");

  onMount(() => mountVolume(slider, (value) => {
    level = value;
    if (value > 0) lastNonZero = value;
  }));

  function toggleMute() {
    if (level === null) return;
    setVolume(level === 0 ? lastNonZero : 0);
  }

  function step(event, delta) {
    const button = event.currentTarget;
    button.classList.remove("k-pop");
    void button.offsetWidth;
    button.classList.add("k-pop");
    if (level === null) return;
    const next = Motion.clamp(level + delta, 0, 10);
    if (next !== level) setVolume(next);
  }
</script>

<section class="card vol-card" class:hidden={level === null} id="np-vol" data-lvl={band}>
  <div class="card-head">
    <h2>{@html icon("speaker")} Громкость</h2>
    <button
      type="button"
      class="btn sm ghost"
      class:on={level === 0}
      disabled={level === null}
      onclick={toggleMute}
    >
      {@html icon(level === 0 ? "voice" : "mute")}
      <span>{level === 0 ? "Включить" : "Без звука"}</span>
    </button>
  </div>
  <div class="vol-main">
    <div class="vol-num"><b class="vol-big" id="vol-big" data-v={level ?? undefined}>{level ?? "–"}</b><small>из 10</small></div>
    <div class="vol-ctl">
      <button class="cbtn" type="button" data-vol="-1" aria-label="Тише" onclick={(event) => step(event, -1)}>{@html icon("minus")}</button>
      <div class="vol-sl">
        <div id="vol" bind:this={slider}></div>
        <div class="vol-ticks" aria-hidden="true">
          {#each ticks as tick}
            <i data-n={tick} class:on={level !== null && tick <= level}></i>
          {/each}
        </div>
      </div>
      <button class="cbtn" type="button" data-vol="1" aria-label="Громче" onclick={(event) => step(event, 1)}>{@html icon("plus")}</button>
    </div>
  </div>
</section>
