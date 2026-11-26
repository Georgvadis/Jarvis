<script>
  import { onMount } from "svelte";
  import { refreshActivity } from "#state/activity.js";
  import { musicData } from "#state/player.js";
  import HomeAmbient from "../../components/HomeAmbient/HomeAmbient.svelte";
  import PlayerQueue from "./PlayerQueue/PlayerQueue.svelte";
  import NowPlaying from "./NowPlaying/NowPlaying.svelte";
  import YouTubeSearch from "./YouTubeSearch/YouTubeSearch.svelte";
  import YouTubeLibrary from "./YouTubeLibrary/YouTubeLibrary.svelte";

  onMount(() => {
    void musicData();
    void refreshActivity();
  });
</script>

<header class="ph">
  <h1>Плеер</h1>
  <p class="sub">Всё, что играет на колонке: YouTube Джарвиса и Яндекс Музыка Алисы</p>
</header>

<NowPlaying />
<div class="cols stagger" id="player-queue">
  <PlayerQueue />
</div>
<div class="player-ambient">
  <HomeAmbient />
</div>
<YouTubeSearch />
<YouTubeLibrary />
