<script>
  import { onMount } from "svelte";
  import { activity } from "#state/activity.js";
  import { getPanel } from "#state/panelContext.js";
  import { S } from "#state/panelState.js";
  import HomeAmbient from "./HomeAmbient/HomeAmbient.svelte";
  import HomeChat from "./HomeChat/HomeChat.svelte";
  import { readOverview } from "./HomeOverview/overview.js";
  import HomePlayer from "./HomePlayer/HomePlayer.svelte";
  import HomeQuick from "./HomeQuick/HomeQuick.svelte";
  import HomeStrip from "./HomeStrip/HomeStrip.svelte";
  import HomeVolume from "./HomeVolume/HomeVolume.svelte";

  const { poll, onPoll } = getPanel();
  let overview = $state.raw(readOverview(S));
  let activeProfile = $state.raw(S.profiles.find((p) => p.id === S.active) || S.profiles[0]);
  let sys = $state.raw(S.sys || {});
  let chat = $state();

  let presence = $derived($activity.presence || { state: "unknown" });
  let greeting = $derived.by(() => {
    const h = new Date().getHours();
    if (h >= 5 && h < 12) return "Доброе утро";
    if (h >= 12 && h < 18) return "Добрый день";
    if (h >= 18 && h < 23) return "Добрый вечер";
    return "Доброй ночи";
  });

  onMount(() => onPoll(refresh));

  function refresh() {
    overview = readOverview(S);
    activeProfile = S.profiles.find((p) => p.id === S.active) || S.profiles[0];
    sys = S.sys || {};
    chat?.refresh();
  }
</script>

<header class="home-head">
  <div class="home-greet">
    <h1>{greeting}</h1>
    <span class="home-status-pill">
      <span class="dot" class:on={overview.micOnline} class:off={!overview.micOnline} id="home-dot"></span>
      <span id="home-sub">{overview.micOnline ? "На связи" : "Микрофон выкл"}</span>
      {#if presence.phone_ip && presence.state !== "unknown"}
        <span class="sep">·</span>
        <span>{presence.state === "home" ? "Дома" : "Нет дома"}</span>
      {/if}
    </span>
  </div>

  <HomeStrip {activeProfile} {sys} />
</header>

<div class="home">
  <section class="tg" id="home-chat" aria-label="Переписка с ассистентом">
    <HomeChat {poll} bind:this={chat} />
  </section>
  <aside class="rail">
    <HomeQuick {overview} />
    <HomeAmbient />
    <div class="mobile-only-cards">
      <HomePlayer />
      <HomeVolume />
    </div>
  </aside>
</div>
