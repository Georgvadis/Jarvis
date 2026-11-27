<script>
  import { onMount } from "svelte";
  import { act, api, icon, observePills, toast } from "#helpers/core.js";
  import { profilePath } from "#helpers/paths.js";
  import { activity, runActivityNext, stopActivityItem, summarizeActivity } from "#state/activity.js";
  import { refreshRoute, reload } from "#state/panelState.js";
  import { subscribeMusic } from "#state/player.js";
  import { ACCENTS, setAccent, themeState, toggleThemeMode } from "#state/theme.js";
  import CommandPalette from "#components/Shell/CommandPalette.svelte";
  import Gate from "#components/Shell/Gate/Gate.svelte";
  import SpeakerDock from "#components/Shell/SpeakerDock.svelte";

  let { access, login, logout, sidebar, orb, navigation, children } = $props();
  let active = $derived(access.status === "active");
  let profiles = $derived(sidebar?.profiles || []);
  let activeProfile = $derived(profiles.find((p) => p.active) || profiles[0] || null);
  let activeProfileId = $derived(activeProfile?.id || "");
  let page = $derived(navigation?.nav || "home");
  let view = $state();
  let cmdOpen = $state(false);
  let asstOpen = $state(false);
  let themeOpen = $state(false);
  let dragged = $state("");
  let playing = $state(false);

  let isTalking = $derived(orb.state === "listening" || orb.state === "thinking" || orb.state === "speaking");
  let liveItems = $derived(summarizeActivity($activity));
  let studioActive = $derived(
    Boolean(
      $activity.reader?.running
      || $activity.games?.active_game
      || $activity.debate_active
      || Boolean($activity.timers?.length)
      || ($activity.modes?.mode && $activity.modes.mode !== "normal")
    )
  );
  let mediaActive = $derived(Boolean(playing || $activity.dj?.active || $activity.ambient?.active));
  let isSettingsPage = $derived(["speaker", "connections", "charts", "logs", "security", "training"].includes(page));
  let isAssistantsPage = $derived(page === "assistants" || page.startsWith("p:") || page === "characters");

  onMount(() => subscribeMusic((state) => { playing = !!state?.playing && !state.paused; }));

  $effect(() => {
    document.body.classList.toggle("hearth-lit", $activity.ambient?.active === "fireplace");
  });

  $effect(() => {
    const route = navigation;
    asstOpen = false;
    themeOpen = false;
    if (active && route && view) {
      view.classList.remove("enter");
      void view.offsetWidth;
      view.classList.add("enter");
    }
  });

  function closePopovers() {
    asstOpen = false;
    themeOpen = false;
  }

  function handleWindowKey(event) {
    if (active && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      closePopovers();
      cmdOpen = !cmdOpen;
      return;
    }
    if (event.key === "Escape") {
      if (cmdOpen) cmdOpen = false;
      else closePopovers();
    }
  }

  function handleWindowClick(event) {
    const target = event.target;
    if (!(target instanceof Element)) return;
    if (asstOpen && !target.closest(".top-asst")) asstOpen = false;
    if (themeOpen && !target.closest(".top-theme")) themeOpen = false;
  }

  function startDrag(event, id) {
    dragged = id;
    event.dataTransfer.setData("text/plain", id);
  }

  async function reorder(event, to) {
    event.preventDefault();
    const from = event.dataTransfer.getData("text/plain");
    const ids = profiles.map((profile) => profile.id);
    if (!ids.includes(from) || from === to) return;
    ids.splice(ids.indexOf(from), 1);
    ids.splice(ids.indexOf(to), 0, from);
    await act(null, async () => {
      await api("POST", "/api/profiles/order", { ids });
      await reload();
    });
  }

  async function quickActivate(event, profile) {
    event.preventDefault();
    event.stopPropagation();
    if (profile.active) return;
    try {
      await api("POST", "/api/active", { id: profile.id });
      await reload();
      toast(`Активен: ${profile.name}`);
      refreshRoute();
    } catch (error) {
      toast(error.message, true);
    }
  }

  let mobileBusy = $state(false);
  async function mobileControl() {
    if (mobileBusy) return;
    mobileBusy = true;
    const talkingNow = isTalking;
    try {
      await api("POST", talkingNow ? "/api/stop" : "/api/talk");
      if (!talkingNow) toast("Слушаю…");
    } catch (error) {
      toast(error.message, true);
    } finally {
      mobileBusy = false;
    }
  }
</script>

<svelte:window onkeydown={handleWindowKey} onclick={handleWindowClick} />

<Gate {access} {login} />

<div id="app" class="app topbar-shell" class:hidden={!active}>
  <!-- Парящая верхняя стеклянная навигация (вместо старого админского сайдбара) -->
  <div class="topbar-wrap">
    <header class="topbar" aria-label="Главная навигация">
      <a class="top-brand" href="/" title={orb.text || "Готов"}>
        <span class="brand-mark" aria-hidden="true">
          <svg viewBox="0 0 32 32" width="22" height="22" fill="none" aria-hidden="true">
            <circle cx="16" cy="16" r="11.5" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-dasharray="14 5 7 5"/>
            <circle cx="16" cy="16" r="6.8" stroke="currentColor" stroke-opacity=".55" stroke-width="1.4" stroke-dasharray="8 4"/>
            <circle cx="16" cy="16" r="3" fill="currentColor"/>
          </svg>
          <span class="brand-mark-dot" id="orb" data-s={orb.state} title={orb.text}></span>
        </span>
        <span class="top-brand-copy">
          <b>Jarvis</b>
        </span>
      </a>

      <nav class="top-nav" id="nav" aria-label="Разделы">
        <a href="/" data-page="home" class:on={page === "home"}>
          {@html icon("flame")}<span>Гостиная</span>
        </a>
        <a href="/assistants" data-page="assistants" class:on={isAssistantsPage}>
          {@html icon("users")}<span>Ассистенты</span>
        </a>
        <a href="/player" data-page="player" class:on={page === "player"}>
          {@html icon("music")}<span>Медиа</span>
          <span class="nav-eq" class:hidden={!playing} id="nav-eq"><i></i><i></i><i></i></span>
          <span class="top-dot" class:hidden={playing || !mediaActive}></span>
        </a>
        <a href="/studio" data-page="studio" class:on={page === "studio"}>
          {@html icon("layers")}<span>Студия</span>
          <span class="top-dot" class:hidden={!studioActive}></span>
        </a>
        <a href="/memory" data-page="memory" class:on={page === "memory"}>
          {@html icon("brain")}<span>Знания</span>
        </a>
        <a href="/speaker" data-page="system" class:on={isSettingsPage}>
          {@html icon("sliders")}<span>Настройки</span>
        </a>
      </nav>

      <div class="top-tools">
        {#if activeProfile}
          <div class="top-asst">
            <button
              type="button"
              class="top-asst-btn"
              class:on={isAssistantsPage || asstOpen}
              aria-expanded={asstOpen}
              title="Ассистенты и характеры"
              onclick={() => { themeOpen = false; asstOpen = !asstOpen; }}
            >
              <span class={"av sm c" + activeProfile.color} class:av-img={Boolean(activeProfile.avatar)}>
                {#if activeProfile.avatar}<img src={activeProfile.avatar} alt="">{:else}{activeProfile.initial}{/if}
              </span>
              <span class="top-asst-name">{activeProfile.name}</span>
              <span class="top-asst-chev" class:open={asstOpen} aria-hidden="true">{@html icon("chevronDown")}</span>
            </button>

            {#if asstOpen}
              <div class="top-pop asst-pop" id="profiles" role="list" aria-label="Ассистенты">
                {#each profiles as profile (profile.id)}
                  <div class="asst-pop-row" role="listitem" class:on={page === "p:" + profile.id} class:drag={dragged === profile.id}
                    draggable="true" data-id={profile.id}
                    ondragstart={(event) => startDrag(event, profile.id)} ondragend={() => { dragged = ""; }}
                    ondragover={(event) => event.preventDefault()} ondrop={(event) => reorder(event, profile.id)}>
                    <a href={profilePath(profile.id)} class="asst-pop-link">
                      <span class={"av c" + profile.color} class:av-img={Boolean(profile.avatar)}>
                        {#if profile.avatar}<img src={profile.avatar} alt="">{:else}{profile.initial}{/if}
                      </span>
                      <div class="meta"><div class="n">{profile.name}</div><div class="s">{profile.tagline || profile.detail}</div></div>
                    </a>
                    <button type="button" class="asst-act-pill" class:on={profile.active}
                      title={profile.active ? "Активный ассистент" : `Выбрать «${profile.name}»`}
                      aria-label={profile.active ? "Активный" : `Активировать ${profile.name}`}
                      onclick={(event) => quickActivate(event, profile)}>
                      {#if profile.active}{@html icon("check")}<span>Активен</span>{:else}<span>Выбрать</span>{/if}
                    </button>
                  </div>
                {/each}
                <div class="asst-pop-foot">
                  <a href="/assistants">{@html icon("users")}<span>Все ассистенты</span></a>
                  <a href={profilePath("new")} id="add-profile">{@html icon("plus")}<span>Новый</span></a>
                </div>
              </div>
            {/if}
          </div>
        {/if}

        <button type="button" class="top-cmd-btn" title="Быстрые команды (Ctrl+K)" aria-label="Поиск команд" onclick={() => { closePopovers(); cmdOpen = true; }}>
          {@html icon("search")}
          <kbd class="top-cmd-kbd">⌘K</kbd>
        </button>

        <div class="top-theme">
          <button
            type="button"
            class="ib top-ib"
            class:on={themeOpen}
            title="Оформление и оттенок"
            aria-label="Оформление"
            aria-expanded={themeOpen}
            onclick={() => { asstOpen = false; themeOpen = !themeOpen; }}
          >
            {@html icon("palette")}
          </button>

          {#if themeOpen}
            <div class="top-pop theme-pop" role="dialog" aria-label="Оформление">
              <div class="theme-pop-row">
                <span class="theme-pop-lbl">Тема</span>
                <button type="button" class="btn sm" onclick={toggleThemeMode}>
                  {@html icon($themeState.mode === "dark" ? "sun" : "moon")}
                  <span>{$themeState.mode === "dark" ? "Светлая" : "Тёмная"}</span>
                </button>
              </div>
              <div class="theme-pop-row">
                <span class="theme-pop-lbl">Свечение</span>
                <div class="side-accents" role="group" aria-label="Оттенок">
                  {#each ACCENTS as acc (acc.id)}
                    <button
                      type="button"
                      class={"side-acc-dot ac-" + acc.id}
                      class:on={$themeState.accent === acc.id}
                      title={acc.label}
                      aria-label={acc.label}
                      aria-pressed={$themeState.accent === acc.id}
                      onclick={() => setAccent(acc.id)}
                    ></button>
                  {/each}
                </div>
              </div>
              <div class="theme-pop-foot">
                <small class="mono">v{sidebar?.version ?? "…"}</small>
                <button class="btn sm ghost" id="logout" type="button" onclick={logout}>
                  {@html icon("logOut")}<span>Выйти</span>
                </button>
              </div>
            </div>
          {/if}
        </div>
      </div>
    </header>
  </div>

  <!-- Мобильная верхняя шапка -->
  <header class="mbar">
    <a class="mbar-brand" href="/" title={orb.text || "Готов"}>
      <span class="brand-mark sm" aria-hidden="true">
        <svg viewBox="0 0 32 32" width="18" height="18" fill="none" aria-hidden="true">
          <circle cx="16" cy="16" r="11.5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-dasharray="14 5 7 5"/>
          <circle cx="16" cy="16" r="6.8" stroke="currentColor" stroke-opacity=".55" stroke-width="1.5" stroke-dasharray="8 4"/>
          <circle cx="16" cy="16" r="3.2" fill="currentColor"/>
        </svg>
        <span class="brand-mark-dot" data-s={orb.state}></span>
      </span>
      <b>Jarvis</b>
    </a>
    <div class="mbar-actions">
      {#if activeProfile}
        <a class="ib" href="/assistants" aria-label="Студия ассистентов">
          <span class={"av sm c" + activeProfile.color} class:av-img={Boolean(activeProfile.avatar)}>
            {#if activeProfile.avatar}<img src={activeProfile.avatar} alt="">{:else}{activeProfile.initial}{/if}
          </span>
        </a>
      {/if}
      <button type="button" class="ib" aria-label="Переключить тему" onclick={toggleThemeMode}>
        {@html icon($themeState.mode === "dark" ? "sun" : "moon")}
      </button>
      <button type="button" class="ib" aria-label="Поиск команд" onclick={() => { cmdOpen = true; }}>{@html icon("search")}</button>
      <button type="button" class="mbar-btn" class:live={isTalking} disabled={mobileBusy} onclick={mobileControl}>
        {@html icon(isTalking ? "stop" : "mic", isTalking ? "solid" : "")}
        <span>{isTalking ? "Стоп" : "Слушать"}</span>
      </button>
    </div>
  </header>

  <main id="view" class="view" use:observePills bind:this={view}>{@render children?.()}</main>

  {#if active}
    <SpeakerDock {orb} {liveItems} />
  {/if}

  {#if liveItems.length}
    {@const topItem = liveItems[0]}
    <div class="live-mbar" aria-label="Активный режим">
      <span class="live-dot"></span>
      <a class="live-link" href={topItem.href}>
        <b>{topItem.title}</b>
        {#if topItem.subtitle}<small>{topItem.subtitle}</small>{/if}
      </a>
      <div class="live-acts">
        {#if topItem.canNext}
          <button type="button" class="ib" aria-label={topItem.nextLabel || "Далее"} onclick={() => void runActivityNext(topItem)}>{@html icon("skip")}</button>
        {/if}
        <button type="button" class="ib danger" aria-label="Остановить" onclick={() => void stopActivityItem(topItem)}>{@html icon("stop", "solid")}</button>
      </div>
    </div>
  {/if}

  <nav class="btab" aria-label="Быстрая навигация">
    <a href="/" class="btab-item" class:on={page === "home"}>{@html icon("flame")}<span>Гостиная</span></a>
    <a href="/player" class="btab-item" class:on={page === "player"}>
      {@html icon("music")}<span>Медиа</span>
      {#if mediaActive}<span class="btab-dot"></span>{/if}
    </a>
    <a href="/studio" class="btab-item" class:on={page === "studio"}>
      {@html icon("layers")}<span>Студия</span>
      {#if studioActive}<span class="btab-dot"></span>{/if}
    </a>
    <a href="/memory" class="btab-item" class:on={page === "memory"}>{@html icon("brain")}<span>Знания</span></a>
    <a href="/speaker" class="btab-item" class:on={isSettingsPage}>
      {@html icon("sliders")}<span>Настройки</span>
    </a>
  </nav>
</div>

{#if active}
  <CommandPalette open={cmdOpen} onclose={() => { cmdOpen = false; }} {profiles} {activeProfileId} />
{/if}
<div id="toast" class="toast" role="status"></div>
