<script>
  import { onMount } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { S } from "#state/panelState.js";

  let { activeProfile, sys = {} } = $props();
  const controller = new AbortController();
  const { signal } = controller;

  let gpu = $state.raw(null);
  let unloading = $state(false);
  let stats = $state.raw({ cpu: null });
  let timer;

  function labelPlugin(id, list) {
    if (!id) return "авто";
    const found = (list || []).find((p) => p.id === id);
    if (found?.name) return found.name;
    const names = { openai: "OpenAI", chatgpt: "ChatGPT", gemini: "Gemini", grok: "Grok", elevenlabs: "ElevenLabs", eleven_realtime: "ElevenLabs Live" };
    return names[id] || id;
  }

  let isClassic = $derived(activeProfile && (activeProfile.mode === "classic" || activeProfile.engine === "classic" || activeProfile.engine === "elevenlabs"));
  let sttLabel = $derived(activeProfile ? labelPlugin(activeProfile.stt_engine || "elevenlabs", S.stt_plugins) : "");
  let llmLabel = $derived(activeProfile ? labelPlugin(activeProfile.llm_engine || activeProfile.llm_plugin || activeProfile.engine, S.text_plugins) : "");
  let ttsLabel = $derived(activeProfile ? labelPlugin(activeProfile.tts_engine || "elevenlabs", S.tts_plugins) : "");
  let pipeShort = $derived(isClassic ? llmLabel : labelPlugin(activeProfile?.engine, []));
  let pipeFull = $derived(isClassic ? `${sttLabel} → ${llmLabel} → ${ttsLabel}` : pipeShort);

  let gpuLoaded = $derived(Boolean(gpu?.whisper_loaded || gpu?.qwen_loaded));
  let gpuLabel = $derived.by(() => {
    if (!gpu) return "GPU…";
    if (!gpu.online) return "GPU офлайн";
    const parts = [];
    if (gpu.whisper_loaded) parts.push("Whisper");
    if (gpu.qwen_loaded) parts.push("Qwen3");
    return parts.length ? parts.join("+") : "GPU готов";
  });

  async function refreshStrip() {
    try {
      const [g, st] = await Promise.all([
        api("GET", "/api/gpu-status", undefined, { signal }).catch(() => ({ online: false })),
        api("GET", "/api/stats?range=180", undefined, { signal }).catch(() => null),
      ]);
      if (signal.aborted) return;
      gpu = g;
      const pts = st?.points || [];
      const last = pts.at(-1);
      if (last && typeof last.cpu === "number") stats = { cpu: Math.round(last.cpu) };
    } catch { /* ignore */ }
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
    void refreshStrip();
    timer = setInterval(() => void refreshStrip(), 15000);
    return () => {
      clearInterval(timer);
      controller.abort();
    };
  });
</script>

<div class="sys-pill" aria-label="Статус системы">
  {#if activeProfile}
    <span class="sys-seg" title={pipeFull}>
      {@html icon("sparkles")}
      <span>{pipeShort}</span>
    </span>
    <span class="sys-div" aria-hidden="true"></span>
  {/if}

  <span class="sys-seg">
    <span class="dot" class:on={gpu?.online} class:off={gpu && !gpu.online}></span>
    <span>{gpuLabel}</span>
    {#if gpuLoaded}
      <button type="button" class="sys-vram-btn" disabled={unloading} onclick={unloadGpu} title="Освободить VRAM" aria-label="Освободить VRAM">
        {@html icon("bolt")}
      </button>
    {/if}
  </span>

  <span class="sys-div" aria-hidden="true"></span>

  <a class="sys-seg link" href="/charts" title="Открыть мониторинг">
    {@html icon("activity")}
    {#if stats.cpu != null}<span>{stats.cpu}%</span>{/if}
    {#if sys.temp_c}<span>{Math.round(sys.temp_c)}°C</span>{/if}
    {#if stats.cpu == null && !sys.temp_c}<span>Мониторинг</span>{/if}
  </a>
</div>
