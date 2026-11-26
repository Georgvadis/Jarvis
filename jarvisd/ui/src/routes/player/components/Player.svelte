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

  const VALID_TABS = new Set(["search", "library", "queue", "dj", "ambient"]);
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

<div class="media-layout">
  <aside class="media-stage" aria-label="Сейчас играет">
    <NowPlaying />
  </aside>

  <div class="media-deck">
    <div class="hub-tabs media-tabs" role="tablist" aria-label="Разделы медиа">
      <button type="button" role="tab" aria-selected={tab === "search"} class="hub-tab" class:on={tab === "search"} onclick={() => switchTab("search")}>
        {@html icon("search")}
        <span>Поиск</span>
      </button>
      <button type="button" role="tab" aria-selected={tab === "library"} class="hub-tab" class:on={tab === "library"} onclick={() => switchTab("library")}>
        {@html icon("heart")}
        <span>Медиатека</span>
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
        {@html icon("flame")}
        <span>Камин и радио</span>
        {#if ambLive}<span class="sdot"></span>{/if}
      </button>
    </div>

    <div class="media-pane">
      {#if tab === "search"}
        <YouTubeSearch />
      {:else if tab === "library"}
        <YouTubeLibrary />
      {:else if tab === "queue"}
        <div class="media-queue-stack" id="player-queue">
          <PlayerQueue />
        </div>
      {:else if tab === "dj"}
        <StudioDj />
      {:else}
        <HomeAmbient />
      {/if}
    </div>
  </div>
</div>
