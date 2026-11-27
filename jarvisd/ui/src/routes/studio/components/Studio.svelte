<script>
  import { page } from "$app/state";
  import { goto } from "$app/navigation";
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";
  import StudioBooks from "./StudioBooks.svelte";
  import StudioGames from "./StudioGames.svelte";
  import StudioLearn from "./StudioLearn.svelte";
  import StudioPodcast from "./StudioPodcast.svelte";
  import StudioSchedule from "./StudioSchedule.svelte";

  const VALID_TABS = new Set(["podcast", "books", "games", "learn", "schedule"]);
  let tab = $state("podcast");
  let busy = $state(false);

  $effect(() => {
    const q = page.url.searchParams.get("tab");
    if (q === "notes") {
      void goto("/memory?tab=notes", { replaceState: true });
      return;
    }
    if (q === "dj") {
      void goto("/player?tab=dj", { replaceState: true });
      return;
    }
    if (q && VALID_TABS.has(q)) tab = q;
    else if (!q) tab = "podcast";
  });

  function switchTab(id) {
    tab = id;
    void goto(id === "podcast" ? "/studio" : `/studio?tab=${id}`, { replaceState: true, keepFocus: true, noScroll: true });
  }

  let anyActive = $derived(
    Boolean(
      $activity.debate_active
      || $activity.reader?.running
      || $activity.games?.active_game
      || ($activity.modes?.mode && $activity.modes.mode !== "normal")
    )
  );

  async function stopAll() {
    if (busy) return;
    busy = true;
    try {
      await api("POST", "/api/stop", {});
      await refreshActivity();
      toast("Остановлено");
    } catch (error) {
      toast(error.message, true);
    } finally {
      busy = false;
    }
  }

  let tabs = $derived([
    { id: "podcast", label: "Подкасты и эфир", ic: "podcast", live: Boolean($activity.debate_active || $activity.modes?.mode === "translator") },
    { id: "books", label: "Книги", ic: "book", live: Boolean($activity.reader?.running) },
    { id: "games", label: "Игры", ic: "gamepad", live: Boolean($activity.games?.active_game || $activity.modes?.mode === "rpg" || $activity.modes?.mode === "quiz" || $activity.modes?.mode === "custom") },
    { id: "learn", label: "Учёба", ic: "graduation", live: Boolean($activity.modes?.mode === "tutor") },
    { id: "schedule", label: "Автоматизации", ic: "clock", live: Boolean($activity.timers?.length) },
  ]);
</script>

<header class="ph ph-row">
  <div>
    <h1>Студия</h1>
  </div>
  {#if anyActive}
    <div class="actions">
      <button type="button" class="btn danger" disabled={busy} onclick={stopAll}>
        {@html icon("stop", "solid")} Остановить всё
      </button>
    </div>
  {/if}
</header>

<div class="hub-tabs" role="tablist" aria-label="Разделы студии">
  {#each tabs as t (t.id)}
    <button
      type="button"
      role="tab"
      aria-selected={tab === t.id}
      class="hub-tab"
      class:on={tab === t.id}
      onclick={() => switchTab(t.id)}
    >
      {@html icon(t.ic)}
      <span>{t.label}</span>
      {#if t.live}<span class="sdot"></span>{/if}
    </button>
  {/each}
</div>

{#if tab === "podcast"}
  <StudioPodcast />
{:else if tab === "books"}
  <StudioBooks />
{:else if tab === "games"}
  <StudioGames />
{:else if tab === "learn"}
  <StudioLearn />
{:else}
  <StudioSchedule />
{/if}
