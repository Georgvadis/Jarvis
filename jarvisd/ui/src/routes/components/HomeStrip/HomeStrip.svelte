<script>
  import { onMount } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { activity } from "#state/activity.js";
  import { S } from "#state/panelState.js";

  let { overview, activeProfile, sys = {} } = $props();
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

  let memPct = $derived(sys.mem_total_mb ? Math.round(((sys.mem_total_mb - (sys.mem_avail_mb || 0)) / sys.mem_total_mb) * 100) : null);
  let gpuLoaded = $derived(Boolean(gpu?.whisper_loaded || gpu?.qwen_loaded));
  let gpuLabel = $derived.by(() => {
    if (!gpu) return "GPU: …";
    if (!gpu.online) return "GPU офлайн";
    const parts = [];
    if (gpu.whisper_loaded) parts.push("Whisper");
    if (gpu.qwen_loaded) parts.push("Qwen3");
    return parts.length ? `VRAM: ${parts.join("+")}` : "GPU: сон";
  });
  let presence = $derived($activity.presence || { state: "unknown" });

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

<div class="home-strip" aria-label="Статус колонки и пайплайна">
  <span class="strip-pill" class:ok={overview.micOnline} class:bad={!overview.micOnline}>
    {@html icon("mic")} {overview.micOnline ? "Микрофон" : "Нет микрофона"}
  </span>

  {#if activeProfile}
    {#if isClassic}
      <span class="strip-pill">{@html icon("sparkles")} {sttLabel} → {llmLabel} → {ttsLabel}</span>
    {:else}
      <span class="strip-pill">{@html icon("sparkles")} {labelPlugin(activeProfile.engine, [])}</span>
    {/if}
  {/if}

  <span class="strip-pill" class:ok={gpu?.online} class:bad={gpu && !gpu.online}>
    {@html icon("cpu")} {gpuLabel}
    {#if gpuLoaded}
      <button type="button" class="btn sm ghost strip-act" disabled={unloading} onclick={unloadGpu} title="Освободить VRAM">
        {@html icon("bolt")}
      </button>
    {/if}
  </span>

  {#if presence.phone_ip && presence.state !== "unknown"}
    <span class="strip-pill" class:ok={presence.state === "home"} class:warn={presence.state === "away"}>
      {@html icon("radar")} {presence.state === "home" ? "Дома" : "Нет дома"}
    </span>
  {/if}

  <a class="strip-link" href="/charts" title="Открыть графики телеметрии">
    {#if stats.cpu != null}<span class="strip-stat">CPU {stats.cpu}%</span>{/if}
    {#if memPct != null}<span class="strip-stat">RAM {memPct}%</span>{/if}
    {#if sys.temp_c}<span class="strip-stat">{Math.round(sys.temp_c)}°C</span>{/if}
    {@html icon("right")}
  </a>
</div>
