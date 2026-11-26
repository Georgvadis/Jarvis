<script>
  import { onMount } from "svelte";
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

  onMount(() => onPoll(refresh));

  function refresh() {
    overview = readOverview(S);
    activeProfile = S.profiles.find((p) => p.id === S.active) || S.profiles[0];
    sys = S.sys || {};
    chat?.refresh();
  }
</script>

<header class="ph">
  <h1>Главная</h1>
  <p class="sub"><span class="dot" class:on={overview.micOnline} class:off={!overview.micOnline} id="home-dot"></span><span id="home-sub">{overview.summary}</span></p>
</header>

<HomeStrip {overview} {activeProfile} {sys} />

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
