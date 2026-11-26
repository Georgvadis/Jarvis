<script>
  import { onMount } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";
  import { speaker } from "#state/panelState.js";
  import {
    artOf,
    clockStr,
    mountPlayback,
    mountVolume,
    musicAct,
    musicLink,
    setArt,
    setYoutubeLike,
    subscribeLikes,
  } from "#state/player.js";
  import styles from "./NowPlaying.module.css";

  let state = $state.raw(null);
  let cover = $state();
  let bars = $state();
  let time = $state();
  let seek = $state();
  let volume = $state();
  let level = $state(null);
  let titleWidth = $state(0);
  let textWidth = $state(0);
  let pending = $state(false);
  let likedSet = $state.raw(new Set());

  let track = $derived(state?.track || {});
  let active = $derived(!!(state?.playing || state?.paused));
  let yandex = $derived(state?.source === "yandex");
  let source = $derived(yandex ? "Яндекс Музыка" : "YouTube");
  let playback = $derived(active ? (state.loading ? "loading" : state.paused ? "paused" : "playing") : state?.loading ? "loading" : "idle");
  let title = $derived(active ? track.Title || "…" : playback === "loading" ? "Ищу и включаю…" : "Ничего не играет");
  let artist = $derived(active ? track.Author || source : "Скажите «Джарвис, включи с ютуба …» или попросите Алису");
  let kicker = $derived(
    active
      ? source + (state.loading ? " · переключаю…" : state.paused ? " · пауза" : "")
      : playback === "loading" ? "YouTube · загрузка" : "Тишина"
  );
  let djActive = $derived(Boolean($activity.dj?.active));
  let image = $derived(active ? artOf(track, true, state) : "");
  let fallback = $derived(active ? artOf(track, false, state) : "");
  let link = $derived(active ? musicLink(state) : "");
  let overflow = $derived(Math.max(0, textWidth - titleWidth));
  let duration = $derived(state?.dur ? clockStr(state.dur) : "");
  let youtubeSigned = $derived(
    Boolean($speaker?.youtube?.plugin && ($speaker?.youtube?.account?.signed_in || $speaker?.youtube?.set)),
  );
  let hasPc = $derived(Boolean($speaker?.settings?.pc_agent_url?.trim()));
  let musicSleepS = $derived(Number($activity.dj?.music_sleep_s || 0));
  let isLiked = $derived(
    Boolean(!yandex && active && track?.ID && (likedSet.has(track.ID) || state?.liked)),
  );

  onMount(() => {
    const releasePlayback = mountPlayback(seek, time, bars, (snapshot) => { state = snapshot; });
    const releaseVolume = mountVolume(volume, (value) => { level = value; });
    const releaseLikes = subscribeLikes((ids) => { likedSet = new Set(ids); });
    void refreshActivity();
    return () => {
      releasePlayback();
      releaseVolume();
      releaseLikes();
    };
  });

  $effect(() => {
    if (cover) setArt(cover, image, fallback);
  });

  async function control(event, action) {
    if (pending) return;
    pending = true;
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
      pending = false;
    }
  }

  async function toggleLike(event) {
    if (pending || !track?.ID || !youtubeSigned) return;
    pending = true;
    try {
      await setYoutubeLike(track.ID, !isLiked, track, event.currentTarget);
    } finally {
      pending = false;
    }
  }

  async function saveTrackToNotes() {
    if (pending || !active) return;
    pending = true;
    try {
      const lines = [`Трек: ${title}`, `Исполнитель: ${artist}`, `Источник: ${source}`];
      if (link) lines.push(`Ссылка: ${link}`);
      await api("POST", "/api/notes/save", {
        title: `${title} — ${artist}`,
        content: lines.join("\n"),
        tags: ["музыка"],
        folder: "Музыка",
      });
      toast("Трек сохранён в CyberNotes");
    } catch (e) {
      toast(e.message, true);
    } finally {
      pending = false;
    }
  }

  async function openTrackOnPc() {
    if (pending || !link) return;
    pending = true;
    try {
      await api("POST", "/api/pc/open-url", { url: link });
      toast("Трек открыт на ПК");
    } catch (e) {
      toast(e.message, true);
    } finally {
      pending = false;
    }
  }

  async function toggleSleep() {
    if (pending) return;
    pending = true;
    const nextMins = musicSleepS > 0 ? 0 : 30;
    try {
      await api("POST", "/api/music/sleep", { minutes: nextMins });
      await refreshActivity();
      toast(nextMins ? "Плавный сон музыки: 30 мин" : "Таймер сна отменён");
    } catch (e) {
      toast(e.message, true);
    } finally {
      pending = false;
    }
  }
</script>

<section class={["card", styles.card]} id="plh" data-state={playback} aria-labelledby="pl-title">
  <img class={styles.glow} src={fallback || undefined} alt="">
  <div class={styles.top}>
    <div class={styles.cover}>
      {@html icon("music")}
      <img bind:this={cover} alt="">
    </div>
    <div class={styles.meta}>
      <div class={styles.kicker}>
        <span class="np-eq" bind:this={bars} aria-hidden="true"><i></i><i></i><i></i></span><span>{kicker}</span>
      </div>
      {#key title}
        <h2 class={[styles.title, { [styles.scroll]: overflow > 4 }]} id="pl-title" {title}
          bind:clientWidth={titleWidth} style:--distance={`${-overflow - 24}px`}
          style:--duration={`${Math.max(6, overflow / 28)}s`}>
          <span class={styles.titleText} bind:clientWidth={textWidth}>{title}</span>
        </h2>
      {/key}
      {#key artist}
        <div class={styles.artist} title={artist}>{artist}</div>
      {/key}
      {#if active}
        <div class={styles.chips}>
          <span class="chip">{yandex ? "через Алису" : "через Джарвиса"}</span>
          {#if state.album}<span class="chip">Альбом «{state.album}»</span>{/if}
          {#if !yandex && state.radio}<span class="chip">радио</span>{/if}
          {#if !yandex && state.repeat}<span class="chip">повтор</span>{/if}
        </div>
      {/if}
    </div>
  </div>

  <div class={styles.bottom}>
    <div class={["progress", styles.seek]} aria-hidden={!active}>
      <span class="roll" id="pl-cur" bind:this={time}></span><div bind:this={seek}></div><span>{duration}</span>
    </div>
    <div class={styles.bar}>
      <div class={styles.actions}>
        {#if !yandex}
          <button class="cbtn" class:on={!!state?.radio} id="pl-radio" type="button" disabled={pending}
            data-tip="Радио: когда очередь кончится, играть похожее" aria-label="Радио" aria-pressed={!!state?.radio}
            onclick={(event) => control(event, "radio")}>{@html icon("radio")}</button>
          {#if active && track?.ID}
            <button class="cbtn" class:liked={isLiked} id="pl-like" type="button"
              disabled={pending || !youtubeSigned}
              data-tip={!youtubeSigned ? "Войдите в аккаунт YouTube в настройках" : isLiked ? "Убрать лайк" : "Поставить лайк"}
              aria-label={isLiked ? "Убрать лайк" : "Поставить лайк"} aria-pressed={isLiked}
              onclick={toggleLike}>{@html icon("heart")}</button>
          {/if}
        {/if}
        {#if active}
          <button class="cbtn" class:on={musicSleepS > 0} type="button" disabled={pending}
            data-tip={musicSleepS > 0 ? `Сон музыки: ${Math.ceil(musicSleepS / 60)}м (выкл)` : "Таймер сна с затуханием (30м)"}
            aria-label="Таймер сна музыки" aria-pressed={musicSleepS > 0}
            onclick={toggleSleep}>{@html icon("moon")}</button>
          <button class="cbtn" type="button" disabled={pending} data-tip="Сохранить трек в CyberNotes" aria-label="Сохранить трек в CyberNotes"
            onclick={saveTrackToNotes}>{@html icon("note")}</button>
        {/if}
        {#if link}
          <button class="cbtn" type="button" disabled={pending} data-tip="Скопировать ссылку" aria-label="Скопировать ссылку"
            onclick={(event) => control(event, "copy")}>{@html icon("link")}</button>
          {#if hasPc}
            <button class="cbtn" type="button" disabled={pending} data-tip="Открыть трек на ПК" aria-label="Открыть трек на ПК"
              onclick={openTrackOnPc}>{@html icon("monitor")}</button>
          {/if}
          <a class="cbtn" href={link} target="_blank" rel="noopener" aria-label="Открыть в браузере"
            data-tip={yandex ? "Открыть в Яндекс Музыке" : "Открыть на YouTube"}>{@html icon("ext")}</a>
        {/if}
      </div>
      <div class={styles.controls}>
        <button class="cbtn" type="button" disabled={!active || pending} data-tip="Выключить" aria-label="Выключить"
          onclick={(event) => control(event, "stop")}>{@html icon("stop", "solid")}</button>
        <button class="cbtn" id="pl-prev" type="button" disabled={!active || pending || (!yandex && (state?.history?.length || 0) < 2)}
          aria-label="Предыдущий" onclick={(event) => control(event, "prev")}>{@html icon("prev", "solid")}</button>
        <button class="cbtn big" id="pl-toggle" type="button" disabled={!active || pending}
          aria-label={playback === "playing" ? "Пауза" : "Продолжить"}
          onclick={(event) => control(event, "toggle")}>{@html icon(playback === "playing" ? "pause" : "play", "solid")}</button>
        <button class="cbtn" id="pl-next" type="button" disabled={!active || pending || (!yandex && !djActive && !state?.next?.length)}
          aria-label="Следующий" onclick={(event) => control(event, "next")}>{@html icon("skip", "solid")}</button>
        <button class="cbtn" class:on={!yandex && !!state?.repeat} id="pl-repeat" type="button"
          disabled={!active || pending || yandex}
          data-tip={yandex ? "Повтор доступен для YouTube" : state?.repeat ? "Повтор включён: этот трек по кругу" : "Повторять этот трек"}
          aria-label="Повтор" aria-pressed={!yandex && !!state?.repeat}
          onclick={(event) => control(event, "repeat")}>{@html icon("repeat")}</button>
      </div>
      <div class={styles.volume} class:hidden={level === null} id="np-vol" title="Громкость колонки">
        {@html icon("speaker")}<div bind:this={volume}></div><b class="vol-big" id="vol-big" data-v={level ?? undefined}>{level ?? "–"}</b>
      </div>
    </div>
  </div>
</section>
