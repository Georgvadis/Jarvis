<script>
  import { onMount } from "svelte";
  import { icon } from "#helpers/core.js";
  import { getPanel } from "#state/panelContext.js";
  import { S } from "#state/panelState.js";
  import HomeChat from "./HomeChat/HomeChat.svelte";
  import HomeLogs from "./HomeLogs/HomeLogs.svelte";
  import HomeMetrics from "./HomeMetrics/HomeMetrics.svelte";
  import HomeOverview from "./HomeOverview/HomeOverview.svelte";
  import { readOverview } from "./HomeOverview/overview.js";
  import HomeModes from "./HomeModes/HomeModes.svelte";
  import HomeAmbient from "./HomeAmbient/HomeAmbient.svelte";
  import HomePlayer from "./HomePlayer/HomePlayer.svelte";
  import HomeVolume from "./HomeVolume/HomeVolume.svelte";

  const { poll, onPoll } = getPanel();
  let overview = $state.raw(readOverview(S));
  let activeProfile = $state.raw(S.profiles.find((p) => p.id === S.active) || S.profiles[0]);
  let chat = $state();
  let metrics = $state();
  let logs = $state();

  onMount(() => onPoll(refresh));

  function refresh() {
    overview = readOverview(S);
    activeProfile = S.profiles.find((p) => p.id === S.active) || S.profiles[0];
    chat?.refresh();
    metrics?.refresh();
    logs?.refresh();
  }

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
</script>

<header class="ph ph-row">
  <div>
    <h1>Главная</h1>
    <p class="sub"><span class="dot" class:on={overview.micOnline} class:off={!overview.micOnline} id="home-dot"></span><span id="home-sub">{overview.summary}</span></p>
  </div>
</header>

{#if activeProfile}
  <div class="pipe-bar" aria-label="Активный пайплайн">
    <span class="pipe-pill" class:ok={overview.micOnline} class:bad={!overview.micOnline}>
      {@html icon("mic")} {overview.micOnline ? "Микрофон" : "Нет микрофона"}
    </span>
    {#if isClassic}
      <span class="pipe-pill">{@html icon("cpu")} {sttLabel}</span>
      <span class="pipe-sep">→</span>
      <span class="pipe-pill">{@html icon("sparkles")} {llmLabel}</span>
      <span class="pipe-sep">→</span>
      <span class="pipe-pill">{@html icon("voice")} {ttsLabel}</span>
    {:else}
      <span class="pipe-pill">{@html icon("sparkles")} {labelPlugin(activeProfile.engine, [])}</span>
    {/if}
  </div>
{/if}

<HomeMetrics bind:this={metrics} />
<div class="home">
  <section class="tg" id="home-chat" aria-label="Переписка с ассистентом">
    <HomeChat {poll} bind:this={chat} />
  </section>
  <aside class="rail">
    <HomeOverview {overview} />
    <HomeModes />
    <HomeAmbient />
    <HomePlayer />
    <HomeVolume />
    <HomeLogs bind:this={logs} />
  </aside>
</div>
