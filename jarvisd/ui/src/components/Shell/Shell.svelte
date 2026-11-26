<script>
  import { onMount, tick } from "svelte";
  import { navPill } from "#helpers/charts.js";
  import { act, api, icon, observePills, tip, toast } from "#helpers/core.js";
  import { profilePath } from "#helpers/paths.js";
  import { activity, runActivityNext, stopActivityItem, summarizeActivity } from "#state/activity.js";
  import { refreshRoute, reload } from "#state/panelState.js";
  import { subscribeMusic } from "#state/player.js";
  import { ACCENTS, setAccent, themeState, toggleThemeMode } from "#state/theme.js";
  import CommandPalette from "#components/Shell/CommandPalette.svelte";
  import Gate from "#components/Shell/Gate/Gate.svelte";

  let { access, login, logout, sidebar, orb, navigation, children } = $props();
  let active = $derived(access.status === "active");
  let profiles = $derived(sidebar?.profiles || []);
  let activeProfileId = $derived(profiles.find((p) => p.active)?.id || "");
  let page = $derived(navigation?.nav || "home");
  let view = $state();
  let menuOpen = $state(false);
  let cmdOpen = $state(false);
  let dragged = $state("");
  let playing = $state(false);

  let isTalking = $derived(orb.state === "listening" || orb.state === "thinking" || orb.state === "speaking");
  let liveItems = $derived(summarizeActivity($activity));
  let studioActive = $derived(
    Boolean(
      $activity.reader?.running
      || $activity.dj?.active
      || $activity.games?.active_game
      || $activity.debate_active
      || Boolean($activity.timers?.length)
      || ($activity.modes?.mode && $activity.modes.mode !== "normal")
    )
  );
  let isMorePage = $derived(!["home", "studio", "player", "speaker"].includes(page));

  onMount(() => subscribeMusic((state) => { playing = !!state?.playing && !state.paused; }));

  $effect(() => {
    document.body.classList.toggle("menu-open", menuOpen);
  });

  $effect(() => {
    const route = navigation;
    closeMenu();
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
      if (cmdOpen) {
        cmdOpen = false;
      } else {
        closeMenu();
      }
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
    <a class="brand" href="/"><span class="orb" id="orb" data-s={orb.state} role="img" aria-label={orb.text} title={orb.text}><i></i></span><span class="brand-t"><b>YaJarvis</b><small id="ver">v{sidebar?.version ?? "…"}</small></span></a>
    <button type="button" class="cmdk-trigger" onclick={() => { closeMenu(); cmdOpen = true; }}>
      {@html icon("search")}
      <span>Команды</span>
      <span class="cmdk-kbd">Ctrl+K</span>
    </button>
    <nav class="nav" id="nav">
      <a href="/" data-page="home" class:on={page === "home"}>{@html icon("home")}Главная</a>
      <a href="/studio" data-page="studio" class:on={page === "studio"}>{@html icon("layers")}Студия<span class="dot" class:hidden={!studioActive}></span></a>
      <a href="/player" data-page="player" class:on={page === "player"}>{@html icon("music")}Плеер<span class="nav-eq" class:hidden={!playing} id="nav-eq"><i></i><i></i><i></i></span></a>
      <div class="nav-group">Ассистенты {@html tip("Ассистент — набор настроек: голос, модель, характер. Активный отвечает на «Джарвис». Перетаскивайте, чтобы поменять порядок.")}</div>
      <div id="profiles" class="profiles">
        {#each profiles as profile (profile.id)}
          <a href={profilePath(profile.id)} class="prof" class:on={page === "p:" + profile.id} class:drag={dragged === profile.id}
            draggable="true" data-id={profile.id}
            ondragstart={(event) => startDrag(event, profile.id)} ondragend={() => { dragged = ""; }}
            ondragover={(event) => event.preventDefault()} ondrop={(event) => reorder(event, profile.id)}>
            <span class={"av c" + profile.color}>{profile.initial}</span>
            <div class="meta"><div class="n">{profile.name}</div><div class="s">{profile.detail}</div></div>
            <button type="button" class="prof-act" class:on={profile.active}
              title={profile.active ? "Активный ассистент" : `Сделать «${profile.name}» активным`}
              aria-label={profile.active ? "Активный ассистент" : `Активировать ${profile.name}`}
              onclick={(event) => quickActivate(event, profile)}><i></i></button>
          </a>
        {/each}
      </div>
      <a href={profilePath("new")} class="navbtn" id="add-profile">{@html icon("plus")}Новый ассистент</a>
      <a href="/characters" data-page="characters" class:on={page === "characters"}>{@html icon("smile")}Характеры</a>
      <a href="/memory" data-page="memory" class:on={page === "memory"}>{@html icon("list")}Память</a>
      <div class="nav-group">Колонка</div>
      <a href="/speaker" data-page="speaker" class:on={page === "speaker"}>{@html icon("speaker")}Колонка</a>
      <a href="/connections" data-page="connections" class:on={page === "connections"}>{@html icon("puzzle")}Подключения</a>
      <div id="nav-ext" class="nav-ext">{#if sidebar?.training}<a href="/training" data-page="training" class:on={page === "training"}>{@html icon("mic")}Обучение</a>{/if}</div>
      <div class="nav-group">Состояние</div>
      <a href="/charts" data-page="charts" class:on={page === "charts"}>{@html icon("activity")}Графики</a>
      <a href="/logs" data-page="logs" class:on={page === "logs"}>{@html icon("note")}Логи</a>
      <div class="nav-sep"></div>
      <a href="/security" data-page="security" class:on={page === "security"}>{@html icon("shield")}Безопасность<span class="dot" class:hidden={!sidebar?.updateAvailable} id="upd-dot"></span></a>
    </nav>

    {#if liveItems.length}
      <div class="live-bar" aria-label="Сейчас активно">
        <div class="live-head"><span class="live-dot"></span>Сейчас активно</div>
        {#each liveItems.slice(0, 2) as item (item.id)}
          <div class="live-row">
            <a class="live-link" href={item.href}>
              <b>{item.title}</b>
              {#if item.subtitle}<small>{item.subtitle}</small>{/if}
            </a>
            <div class="live-acts">
              {#if item.canNext}
                <button type="button" class="ib" title={item.nextLabel || "Далее"} aria-label={item.nextLabel || "Далее"} onclick={() => void runActivityNext(item)}>
                  {@html icon("skip")}
                </button>
              {/if}
              <button type="button" class="ib danger" title="Остановить" aria-label="Остановить" onclick={() => void stopActivityItem(item)}>
                {@html icon("stop", "solid")}
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    <div class="spacer"></div>
    <div class="theme-bar" aria-label="Оформление">
      <button type="button" class="theme-toggle" onclick={toggleThemeMode}
        title={$themeState.mode === "dark" ? "Светлая тема" : "Тёмная тема"}
        aria-label="Переключить тему">
        {@html icon($themeState.mode === "dark" ? "sun" : "moon")}
      </button>
      <div class="accent-dots" role="group" aria-label="Акцентный цвет">
        {#each ACCENTS as a (a.id)}
          <button
            type="button"
            class={"accent-dot ac-" + a.id}
            class:on={$themeState.accent === a.id}
            title={a.label}
            aria-label={a.label}
            onclick={() => setAccent(a.id)}
          ></button>
        {/each}
      </div>
    </div>
    <button class="navbtn" id="logout" type="button" onclick={logout}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9"/></svg>Выйти</button>
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
    <a href="/studio" class="btab-item" class:on={page === "studio"}>
      {@html icon("layers")}<span>Студия</span>
      {#if studioActive}<span class="btab-dot"></span>{/if}
    </a>
    <a href="/player" class="btab-item" class:on={page === "player"}>{@html icon("music")}<span>Плеер</span></a>
    <a href="/speaker" class="btab-item" class:on={page === "speaker"}>{@html icon("speaker")}<span>Колонка</span></a>
    <button type="button" class="btab-item" class:on={isMorePage || menuOpen} onclick={() => { menuOpen = !menuOpen; }}>
      {@html icon("menu")}<span>Ещё</span>
    </button>
  </nav>
</div>

{#if active}
  <CommandPalette open={cmdOpen} onclose={() => { cmdOpen = false; }} {profiles} {activeProfileId} />
{/if}
<div id="toast" class="toast" role="status"></div>
<button type="button" class="scrim" tabindex="-1" aria-label="Закрыть меню" onclick={closeMenu}></button>
