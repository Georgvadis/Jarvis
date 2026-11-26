<script>
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { icon } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";
  import { musicData } from "#state/player.js";
  import HomeAmbient from "../../components/HomeAmbient/HomeAmbient.svelte";
  import StudioDj from "../../studio/components/StudioDj.svelte";
  import PlayerQueue from "./PlayerQueue/PlayerQueue.svelte";
  import NowPlaying from "./NowPlaying/NowPlaying.svelte";
  import YouTubeSearch from "./YouTubeSearch/YouTubeSearch.svelte";
  import YouTubeLibrary from "./YouTubeLibrary/YouTubeLibrary.svelte";

  const VALID_TABS = new Set(["search", "queue", "dj", "ambient"]);
  let tab = $state("search");

  $effect(() => {
    const q = page.url.searchParams.get("tab");
    if (q && VALID_TABS.has(q)) tab = q;
    else if (!q) tab = "search";
  });

  function switchTab(id) {
    tab = id;
    void goto(id === "search" ? "/player" : `/player?tab=${id}`, { replaceState: true, keepFocus: true, noScroll: true });
  }

  let djLive = $derived(Boolean($activity.dj?.active));
  let ambLive = $derived(Boolean($activity.ambient?.active));

  onMount(() => {
    void musicData();
    void refreshActivity();
  });
</script>

<header class="ph">
  <h1>Медиа</h1>
</header>

<NowPlaying />

<div class="hub-tabs" role="tablist" aria-label="Разделы медиа">
  <button type="button" role="tab" aria-selected={tab === "search"} class="hub-tab" class:on={tab === "search"} onclick={() => switchTab("search")}>
    {@html icon("search")}
    <span>Поиск и медиатека</span>
  </button>
  <button type="button" role="tab" aria-selected={tab === "queue"} class="hub-tab" class:on={tab === "queue"} onclick={() => switchTab("queue")}>
    {@html icon("list")}
    <span>Очередь</span>
  </button>
  <button type="button" role="tab" aria-selected={tab === "dj"} class="hub-tab" class:on={tab === "dj"} onclick={() => switchTab("dj")}>
    {@html icon("disc")}
    <span>AI-DJ</span>
    {#if djLive}<span class="sdot"></span>{/if}
  </button>
  <button type="button" role="tab" aria-selected={tab === "ambient"} class="hub-tab" class:on={tab === "ambient"} onclick={() => switchTab("ambient")}>
    {@html icon("radio")}
    <span>Фон и радио</span>
    {#if ambLive}<span class="sdot"></span>{/if}
  </button>
</div>

{#if tab === "search"}
  <YouTubeSearch />
  <YouTubeLibrary />
{:else if tab === "queue"}
  <div class="cols stagger" id="player-queue">
    <PlayerQueue />
  </div>
{:else if tab === "dj"}
  <StudioDj />
{:else}
  <div class="player-ambient">
    <HomeAmbient />
  </div>
{/if}
