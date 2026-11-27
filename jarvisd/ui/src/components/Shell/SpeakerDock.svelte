<script>
  import { onMount } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, refreshActivity, runActivityNext, stopActivityItem } from "#state/activity.js";
  import { artOf, musicAct, musicData, setArt, setVolume, subscribeMusic, subscribeVolume } from "#state/player.js";

  let { orb, liveItems = [] } = $props();

  let state = $state.raw(null);
  let cover = $state();
  let volume = $state(null);
  let lastNonZero = 5;
  let pending = $state("");
  let talkBusy = $state(false);

  let track = $derived(state?.track || {});
  let active = $derived(!!(state?.playing || state?.paused));
  let yandex = $derived(state?.source === "yandex");
  let source = $derived(yandex ? "Яндекс Музыка" : "YouTube");
  let playing = $derived(Boolean(state?.playing && !state?.paused));
  let image = $derived(active ? artOf(track, false, state) : "");
  let isTalking = $derived(orb.state === "listening" || orb.state === "thinking" || orb.state === "speaking");
  let djActive = $derived(Boolean($activity.dj?.active));
  let topActivity = $derived(!active && liveItems.length ? liveItems[0] : null);
  let progressPct = $derived(active && state?.dur > 0 ? Math.min(100, Math.max(0, ((state.pos || 0) / state.dur) * 100)) : 0);

  const steps = Array.from({ length: 10 }, (_, i) => i + 1);

  onMount(() => {
    const offMusic = subscribeMusic((snap) => { state = snap; });
    const offVol = subscribeVolume((lvl) => {
      volume = lvl;
      if (lvl > 0) lastNonZero = lvl;
    });
    void musicData();
    return () => {
      offMusic();
      offVol();
    };
  });

  $effect(() => {
    if (cover) setArt(cover, image, image);
  });

  async function control(event, action) {
    if (pending) return;
    pending = action;
    try {
      if (action === "next" && djActive && !yandex && !state?.next?.length) {
        if (state) state = { ...state, loading: true, pos: 0 };
        await api("POST", "/api/dj/next", {});
        void refreshActivity();
      } else {
        await musicAct(action, event.currentTarget, { disableControl: false });
      }
    } catch (error) {
      toast(error.message, true);
    } finally {
      pending = "";
    }
  }

  function toggleMute() {
    if (volume === null) return;
    setVolume(volume === 0 ? lastNonZero : 0);
  }

  async function toggleTalk() {
    if (talkBusy) return;
    talkBusy = true;
    const talkingNow = isTalking;
    try {
      await api("POST", talkingNow ? "/api/stop" : "/api/talk");
      if (!talkingNow) toast("Слушаю…");
    } catch (error) {
      toast(error.message, true);
    } finally {
      talkBusy = false;
    }
  }
</script>

<div class="dock-wrap">
<footer class="dock" class:talking={isTalking} aria-label="Панель колонки">
  {#if active && progressPct > 0}
    <div class="dock-progress" aria-hidden="true">
      <span style:width={`${progressPct.toFixed(2)}%`}></span>
    </div>
  {/if}
  {#if active}
    <a class="dock-track" href="/player">
      <div class="dock-art">
        {@html icon("music")}
        <img bind:this={cover} alt="" />
      </div>
      <div class="dock-info">
        <b title={track.Title || "…"}>{track.Title || "…"}</b>
        <small>
          {#if state?.loading}
            Переключаю…
          {:else}
            {track.Author ? `${track.Author} · ${source}` : source}{state?.paused ? " · пауза" : ""}
          {/if}
        </small>
      </div>
    </a>

    <div class="dock-ctl">
      <button type="button" class="ib" aria-label="Предыдущий"
        disabled={pending === "prev" || (!yandex && (state?.history?.length || 0) < 2)}
        onclick={(e) => control(e, "prev")}>
        {@html icon("prev", "solid")}
      </button>
      <button type="button" class="dock-play" aria-label={playing ? "Пауза" : "Продолжить"}
        disabled={pending === "toggle"}
        onclick={(e) => control(e, "toggle")}>
        {@html icon(playing ? "pause" : "play", "solid")}
      </button>
      <button type="button" class="ib" aria-label="Следующий"
        disabled={pending === "next" || (!yandex && !djActive && !state?.next?.length)}
        onclick={(e) => control(e, "next")}>
        {@html icon("skip", "solid")}
      </button>
    </div>
  {:else if topActivity}
    <a class="dock-track" href={topActivity.href}>
      <div class="dock-art">
        {@html icon(topActivity.id === "ambient" ? "flame" : "layers")}
      </div>
      <div class="dock-info">
        <b>{topActivity.title}</b>
        {#if topActivity.subtitle}<small>{topActivity.subtitle}</small>{/if}
      </div>
    </a>

    <div class="dock-ctl">
      {#if topActivity.canNext}
        <button type="button" class="ib" aria-label={topActivity.nextLabel || "Далее"}
          onclick={() => void runActivityNext(topActivity)}>
          {@html icon("skip")}
        </button>
      {/if}
      <button type="button" class="ib danger" aria-label="Остановить"
        onclick={() => void stopActivityItem(topActivity)}>
        {@html icon("stop", "solid")}
      </button>
    </div>
  {:else}
    <a class="dock-track" href="/player">
      <div class="dock-art">
        {@html icon("music")}
      </div>
      <div class="dock-info">
        <b>Тишина</b>
        <small>{orb.text || "Колонка готова"}</small>
      </div>
    </a>
  {/if}

  {#if volume !== null}
    <div class="dock-vol" aria-label="Громкость колонки">
      <button type="button" class="ib" aria-label={volume === 0 ? "Включить звук" : "Выключить звук"} onclick={toggleMute}>
        {@html icon(volume === 0 ? "mute" : "voice")}
      </button>
      <div class="dock-steps" role="group" aria-label="Уровень громкости">
        {#each steps as step (step)}
          <button
            type="button"
            class="dock-step"
            class:on={step <= volume}
            aria-label={`Громкость ${step}`}
            onclick={() => setVolume(step)}
          ></button>
        {/each}
      </div>
      <span class="dock-vol-num">{volume}</span>
    </div>
  {/if}

  <button type="button" class="dock-talk" class:live={isTalking} disabled={talkBusy} onclick={toggleTalk}>
    {@html icon(isTalking ? "stop" : "mic", isTalking ? "solid" : "")}
    <span>{isTalking ? "Стоп" : "Слушать"}</span>
  </button>
</footer>
</div>
