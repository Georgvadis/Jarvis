<script>
  import SystemNav from "#components/Shell/SystemNav.svelte";
  let { enabled, showRecords } = $props();
</script>

<SystemNav current="training" />

<header class="section-head"><h1>Обучение слова</h1></header>
{#if enabled}
  <div class="studio-stack">
    {#await import("./Training.svelte") then { default: Training }}
      <Training {showRecords} />
    {:catch error}
      <p class="err">{error.message}</p>
    {/await}
  </div>
{:else}
  <p class="note">Включите плагин «Обучение слова» в <a href="/connections/training">подключениях</a>.</p>
{/if}
