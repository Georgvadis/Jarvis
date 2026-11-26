<script>
  import { onMount } from "svelte";
  import { api, icon, tip, toast } from "#helpers/core.js";

  let { overview } = $props();
  let completed = $derived(overview.steps.filter(({ done }) => done).length);
  const controller = new AbortController();
  const { signal } = controller;
  let gpu = $state.raw(null);
  let unloading = $state(false);
  let gpuTimer;

  async function loadGpu() {
    try {
      const next = await api("GET", "/api/gpu-status", undefined, { signal });
      if (!signal.aborted) gpu = next;
    } catch {
      if (!signal.aborted) gpu = { online: false };
    }
  }

  async function unloadGpu() {
    if (unloading) return;
    unloading = true;
    try {
      const next = await api("POST", "/api/gpu-unload", undefined, { signal });
      if (!signal.aborted) {
        gpu = next;
        toast("VRAM освобождена");
      }
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      unloading = false;
    }
  }

  onMount(() => {
    void loadGpu();
    gpuTimer = setInterval(() => void loadGpu(), 15000);
    return () => {
      clearInterval(gpuTimer);
      controller.abort();
    };
  });

  let gpuLoaded = $derived(Boolean(gpu?.whisper_loaded || gpu?.qwen_loaded));
  let gpuText = $derived.by(() => {
    if (!gpu) return "проверка…";
    if (!gpu.online) return "офлайн";
    const active = [];
    if (gpu.whisper_loaded) active.push("Whisper");
    if (gpu.qwen_loaded) active.push("Qwen3-TTS");
    return active.length ? `в VRAM: ${active.join(" + ")}` : "онлайн · сон";
  });
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

<div class="card">
  <h2>{@html icon("cpu")} Состояние</h2>
  <dl class="stat">
    {#each overview.statuses as status (status.id)}
      <div>
        <dt>{status.label} {@html tip(status.tip)}</dt>
        <dd class="v {status.level}" id={status.id}>{status.text}</dd>
      </div>
    {/each}
    <div>
      <dt>GPU-сервер {@html tip("Локальный сервер распознавания (Whisper) и синтеза (Qwen3-TTS) на вашей видеокарте.")}</dt>
      <dd class="v" class:ok={gpu?.online} class:bad={gpu && !gpu.online}>{gpuText}</dd>
    </div>
  </dl>
  {#if gpuLoaded}
    <div class="row gpu-act">
      <button type="button" class="btn sm ghost" disabled={unloading} onclick={unloadGpu}>
        {@html icon("bolt")} {unloading ? "Выгружаю…" : "Освободить VRAM"}
      </button>
    </div>
  {/if}
</div>
