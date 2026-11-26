<script>
  import { onMount, tick } from "svelte";
  import { navPill } from "#helpers/charts.js";
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
  let menuOpen = $state(false);
  let cmdOpen = $state(false);
  let asstOpen = $state(false);
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
  let isSystemPage = $derived(["speaker", "connections", "charts", "logs", "security", "training"].includes(page));
  let isAssistantsPage = $derived(page.startsWith("p:") || page === "characters");
  let defaultAssistantHref = $derived(activeProfileId ? profilePath(activeProfileId) : "/characters");

  onMount(() => subscribeMusic((state) => { playing = !!state?.playing && !state.paused; }));

  $effect(() => {
    document.body.classList.toggle("menu-open", menuOpen);
  });

  $effect(() => {
    document.body.classList.toggle("hearth-lit", $activity.ambient?.active === "fireplace");
  });

  $effect(() => {
    const route = navigation;
    closeMenu();
    asstOpen = false;
    if (active && route && view) {
      view.classList.remove("enter");
      void view.offsetWidth;
      view.classList.add("enter");
    }
  });

  $effect(() => {
    if (active && sidebar && navigation) void tick().then(navPill);
  });

  function closeMenu() { menuOpen = false; }

  function handleWindowKey(event) {
    if (active && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      cmdOpen = !cmdOpen;
      return;
    }
    if (event.key === "Escape") {
      if (cmdOpen) cmdOpen = false;
      else if (asstOpen) asstOpen = false;
      else closeMenu();
    }
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

<svelte:window onkeydown={handleWindowKey} />

<Gate {access} {login} />
<div id="app" class="app" class:hidden={!active}>
  <aside class="side" class:open={menuOpen} id="side">
    <a class="brand" href="/">
      <span class="brand-mark" aria-hidden="true">
        {@html icon("flame")}
        <span class="brand-mark-dot" id="orb" data-s={orb.state} title={orb.text}></span>
      </span>
      <span class="brand-t">
        <span class="brand-row"><b>YaJarvis</b><small class="brand-ver" id="ver">v{sidebar?.version ?? "…"}</small></span>
        <span class="brand-st">{orb.text || "Колонка готова"}</span>
      </span>
    </a>

    {#if activeProfile}
      <div class="asst-box">
        <button type="button" class="asst-card" aria-expanded={asstOpen} onclick={() => { asstOpen = !asstOpen; }}>
          <span class={"av c" + activeProfile.color}>{activeProfile.initial}</span>
          <span class="asst-meta">
            <b>{activeProfile.name}</b>
            <small>{activeProfile.detail || "Активный ассистент"}</small>
          </span>
          <span class="asst-chev">{@html icon(asstOpen ? "x" : "right")}</span>
        </button>

        {#if asstOpen}
          <div class="asst-pop" id="profiles" role="list" aria-label="Ассистенты">
            {#each profiles as profile (profile.id)}
              <div class="asst-pop-row" role="listitem" class:on={page === "p:" + profile.id} class:drag={dragged === profile.id}
                draggable="true" data-id={profile.id}
                ondragstart={(event) => startDrag(event, profile.id)} ondragend={() => { dragged = ""; }}
                ondragover={(event) => event.preventDefault()} ondrop={(event) => reorder(event, profile.id)}>
                <a href={profilePath(profile.id)} class="asst-pop-link">
                  <span class={"av c" + profile.color}>{profile.initial}</span>
                  <div class="meta"><div class="n">{profile.name}</div><div class="s">{profile.detail}</div></div>
                </a>
                <button type="button" class="prof-act" class:on={profile.active}
                  title={profile.active ? "Активный" : `Активировать «${profile.name}»`}
                  aria-label={profile.active ? "Активный" : `Активировать ${profile.name}`}
                  onclick={(event) => quickActivate(event, profile)}><i></i></button>
              </div>
            {/each}
            <div class="asst-pop-foot">
              <a href={profilePath("new")} id="add-profile">{@html icon("plus")}Новый</a>
              <a href="/characters">{@html icon("smile")}Характеры</a>
            </div>
          </div>
        {/if}
      </div>
    {/if}

    <button type="button" class="cmdk-trigger" onclick={() => { closeMenu(); cmdOpen = true; }}>
      {@html icon("search")}
      <span>Команды</span>
      <span class="cmdk-kbd">⌘K</span>
    </button>

    <nav class="nav" id="nav">
      <div class="nav-group">Дом</div>
      <a href="/" data-page="home" class:on={page === "home"}>{@html icon("home")}Главная</a>
      <a href="/player" data-page="player" class:on={page === "player"}>{@html icon("music")}Медиа<span class="nav-eq" class:hidden={!playing} id="nav-eq"><i></i><i></i><i></i></span><span class="dot" class:hidden={playing || !mediaActive}></span></a>
      <a href="/studio" data-page="studio" class:on={page === "studio"}>{@html icon("layers")}Студия<span class="dot" class:hidden={!studioActive}></span></a>

      <div class="nav-group">Память и ИИ</div>
      <a href="/memory" data-page="memory" class:on={page === "memory"}>{@html icon("brain")}Знания</a>
      <a href={defaultAssistantHref} data-page="assistants" class:on={isAssistantsPage}>{@html icon("users")}Ассистенты</a>

      <div class="nav-group">Устройство</div>
      <a href="/speaker" data-page="system" class:on={isSystemPage}>{@html icon("sliders")}Система<span class="dot" class:hidden={!sidebar?.updateAvailable} id="upd-dot"></span></a>
    </nav>

    <div class="side-foot">
      <div class="side-theme-row" aria-label="Оформление">
        <button type="button" class="theme-toggle" onclick={toggleThemeMode}
          title={$themeState.mode === "dark" ? "Светлая тема" : "Тёмная тема"}
          aria-label="Переключить тему">
          {@html icon($themeState.mode === "dark" ? "sun" : "moon")}
        </button>
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
      <button class="navbtn side-logout" id="logout" type="button" onclick={logout}>
        <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9"/></svg>
        Выйти
      </button>
    </div>
  </aside>

  <header class="mbar">
    <button class="ib" id="menu" type="button" aria-label="Меню" aria-expanded={menuOpen} aria-controls="side" onclick={() => { menuOpen = !menuOpen; }}>{@html icon("menu")}</button>
    <span class="brand"><span class="orb" data-s={orb.state} aria-hidden="true"><i></i></span><b>YaJarvis</b></span>
    <div class="mbar-actions">
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
    <a href="/" class="btab-item" class:on={page === "home"}>{@html icon("home")}<span>Главная</span></a>
    <a href="/player" class="btab-item" class:on={page === "player"}>
      {@html icon("music")}<span>Медиа</span>
      {#if mediaActive}<span class="btab-dot"></span>{/if}
    </a>
    <a href="/studio" class="btab-item" class:on={page === "studio"}>
      {@html icon("layers")}<span>Студия</span>
      {#if studioActive}<span class="btab-dot"></span>{/if}
    </a>
    <a href="/memory" class="btab-item" class:on={page === "memory"}>{@html icon("brain")}<span>Знания</span></a>
    <a href="/speaker" class="btab-item" class:on={isSystemPage}>
      {@html icon("sliders")}<span>Система</span>
      {#if sidebar?.updateAvailable}<span class="btab-dot"></span>{/if}
    </a>
  </nav>
</div>

{#if active}
  <CommandPalette open={cmdOpen} onclose={() => { cmdOpen = false; }} {profiles} {activeProfileId} />
{/if}
<div id="toast" class="toast" role="status"></div>
<button type="button" class="scrim" tabindex="-1" aria-label="Закрыть меню" onclick={closeMenu}></button>
