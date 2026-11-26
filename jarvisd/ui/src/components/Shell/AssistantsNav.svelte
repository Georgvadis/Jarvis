<script>
  import { icon } from "#helpers/core.js";
  import { profilePath } from "#helpers/paths.js";
  import { speaker } from "#state/panelState.js";

  let { current = "" } = $props();

  let profiles = $derived($speaker?.profiles || []);
  let activeId = $derived($speaker?.active || "");
</script>

<nav class="hub-tabs asst-tabs" aria-label="Ассистенты и характеры">
  {#each profiles as p (p.id)}
    <a href={profilePath(p.id)} class="hub-tab" class:on={current === "p:" + p.id}>
      <span class={"av sm c" + (p.color || 1)}>{(p.name || "?").trim()[0] || "?"}</span>
      <span>{p.name}</span>
      {#if p.id === activeId}<span class="sdot ok" title="Активный"></span>{/if}
    </a>
  {/each}
  <a href={profilePath("new")} class="hub-tab" class:on={current === "p:new"}>
    {@html icon("plus")}
    <span>Новый</span>
  </a>
  <span class="hub-tab-sep" aria-hidden="true"></span>
  <a href="/characters" class="hub-tab" class:on={current === "characters"}>
    {@html icon("smile")}
    <span>Характеры</span>
  </a>
</nav>
