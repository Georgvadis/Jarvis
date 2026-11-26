<script>
  import { page } from "$app/state";
  import { goto } from "$app/navigation";
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";
  import StudioBooks from "./StudioBooks.svelte";
  import StudioDj from "./StudioDj.svelte";
  import StudioGames from "./StudioGames.svelte";
  import StudioNotes from "./StudioNotes.svelte";
  import StudioPodcast from "./StudioPodcast.svelte";

  const VALID_TABS = new Set(["podcast", "books", "games", "dj", "notes"]);
  let tab = $state("podcast");
  let busy = $state(false);

  $effect(() => {
    const q = page.url.searchParams.get("tab");
    if (q && VALID_TABS.has(q)) tab = q;
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
      || $activity.dj?.active
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
    { id: "podcast", label: "Подкаст", ic: "podcast", live: Boolean($activity.debate_active || $activity.modes?.mode === "translator") },
    { id: "books", label: "Книги", ic: "book", live: Boolean($activity.reader?.running) },
    { id: "games", label: "Игры", ic: "gamepad", live: Boolean($activity.games?.active_game || $activity.modes?.mode === "rpg" || $activity.modes?.mode === "quiz") },
    { id: "dj", label: "AI-DJ", ic: "disc", live: Boolean($activity.dj?.active) },
    { id: "notes", label: "Заметки", ic: "note", live: false },
  ]);
</script>

<header class="ph ph-row">
  <div>
    <h1>Студия</h1>
    <p class="sub">Подкасты, аудиокниги, игры, персональный AI-DJ и заметки</p>
  </div>
  {#if anyActive}
    <div class="actions">
      <button type="button" class="btn danger" disabled={busy} onclick={stopAll}>
        {@html icon("stop", "solid")} Остановить всё
      </button>
    </div>
  {/if}
</header>

<div class="studio-tabs" role="tablist" aria-label="Разделы студии">
  {#each tabs as t (t.id)}
    <button
      type="button"
      role="tab"
      aria-selected={tab === t.id}
      class="studio-tab"
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
{:else if tab === "dj"}
  <StudioDj />
{:else}
  <StudioNotes />
{/if}
