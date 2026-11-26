<script>
  import { onMount } from "svelte";
  import { afterNavigate, beforeNavigate, goto } from "$app/navigation";
  import { page } from "$app/state";
  import Shell from "#components/Shell/Shell.svelte";
  import { api, dirty, setDirty, toast } from "#helpers/core.js";
  import { legacyHashPath } from "#helpers/paths.js";
  import { setPanel } from "#state/panelContext.js";
  import { S, access, readJournal, readOrb, readSidebar, reload, routeRequests, showGate, speaker } from "#state/panelState.js";
  import { musicData, refreshVolume } from "#state/player.js";
  import { resetActivityPolling, startActivityPolling } from "#state/activity.js";
  import { initTheme } from "#state/theme.js";
  import { closeWizard, wizardRequest, wizOpen } from "#state/wizard.js";

  let { children } = $props();
  const lifetime = new AbortController();
  let sidebar = $derived($speaker ? readSidebar($speaker) : null);
  let navigation = $derived({
    nav: page.params.id !== undefined ? "p:" + page.params.id : page.url.pathname.split("/")[1] || "home",
    path: page.url.pathname + page.url.search,
    revision: $routeRequests,
  });
  let orb = $state.raw({ state: "idle", text: "Состояние колонки" });
  let Wizard = $state.raw(null);
  let pageRefresh;
  let session;
  let interval;
  let fastPoll;

  setPanel({ poll, onPoll });

  let dirtyConfirmUntil = 0;
  beforeNavigate(({ willUnload, cancel }) => {
    if (!willUnload && dirty) {
      if (Date.now() > dirtyConfirmUntil) {
        dirtyConfirmUntil = Date.now() + 5000;
        cancel();
        toast("Есть несохранённые изменения — нажмите ещё раз для выхода", true);
        return;
      }
      dirtyConfirmUntil = 0;
    }
  });
  afterNavigate(() => {
    dirtyConfirmUntil = 0;
    setDirty(false);
  });

  function onGlobalKey(event) {
    if (event.defaultPrevented || event.ctrlKey || event.metaKey) return;
    const target = event.target;
    const tag = target?.tagName;
    const editing = tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable;
    if (editing || document.querySelector("dialog[open], .note-modal-bg, .modal-bg")) return;
    if (event.key === "/") {
      const input = document.querySelector("#say, main input[type='search']");
      if (input instanceof HTMLElement) {
        event.preventDefault();
        input.focus();
      }
    } else if (event.altKey && (event.code === "KeyS" || event.key.toLowerCase() === "s")) {
      event.preventDefault();
      void api("POST", "/api/stop").then(() => toast("Остановлено")).catch((e) => toast(e.message, true));
    }
  }

  onMount(() => {
    initTheme();
    if ("serviceWorker" in navigator) {
      void navigator.serviceWorker.register("/sw.js").catch(() => {});
    }
    const legacyPath = location.pathname === "/" ? legacyHashPath(location.hash) : "";
    if (legacyPath) void goto(legacyPath, { replaceState: true });
    const unsubscribeRoutes = routeRequests.subscribe(() => setDirty(false));
    const unsubscribeAccess = access.subscribe(({ status }) => {
      stopPolling();
      resetActivityPolling();
      if (status !== "active") {
        setDirty(false);
        closeWizard();
        return;
      }
      session = new AbortController();
      orb = readOrb();
      if (!S.settings.onboarded) wizOpen(0);
      interval = setInterval(poll, 2000);
      void poll();
      startActivityPolling();
    });

    void start().catch(() => {});
    return () => {
      lifetime.abort();
      stopPolling();
      resetActivityPolling();
      unsubscribeRoutes();
      unsubscribeAccess();
      closeWizard();
    };
  });

  $effect(() => {
    if ($access.status !== "active" || !$wizardRequest || Wizard) return;
    let cancelled = false;
    void import("#components/Wizard/Wizard.svelte").then(({ default: component }) => {
      if (!cancelled) Wizard = component;
    }).catch((error) => {
      if (!cancelled) toast(error.message, true);
    });
    return () => { cancelled = true; };
  });

  async function start() {
    await reload(lifetime.signal);
    access.set({ status: "active" });
  }

  async function login(setup, password) {
    await api("POST", setup ? "/api/setup" : "/api/login", { password }, { signal: lifetime.signal });
    await start();
  }

  async function logout() {
    await api("POST", "/api/logout", undefined, { signal: lifetime.signal }).catch(() => {});
    if (!lifetime.signal.aborted) showGate(false);
  }

  function stopPolling() {
    session?.abort();
    session = undefined;
    clearInterval(interval);
    clearTimeout(fastPoll);
  }

  function onPoll(refresh) {
    pageRefresh = refresh;
    return () => {
      if (pageRefresh === refresh) pageRefresh = undefined;
    };
  }

  async function poll() {
    if (!session) return;
    const { signal } = session;
    const view = navigation;
    try {
      if (!await readJournal(signal) || signal.aborted) return;
    } catch {
      return;
    }
    orb = readOrb();
    refreshVolume();
    if (view === navigation) void pageRefresh?.();
    void musicData();
    clearTimeout(fastPoll);
    if (S.speaker.talking && navigation.nav === "home") fastPoll = setTimeout(poll, 500);
  }
</script>

<svelte:window onbeforeunload={(event) => { if (dirty) event.preventDefault(); }} onkeydown={onGlobalKey} />

<Shell access={$access} {login} {logout} {sidebar} {orb} {navigation}>
  {#if $access.status === "active"}
    {#key $routeRequests}
      {@render children?.()}
    {/key}
  {/if}
</Shell>

{#if $access.status === "active" && $wizardRequest && Wizard}
  <Wizard request={$wizardRequest} />
{/if}
