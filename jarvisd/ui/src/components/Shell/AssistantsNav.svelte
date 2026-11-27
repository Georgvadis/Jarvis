<script>
  import { icon } from "#helpers/core.js";
  import { profilePath } from "#helpers/paths.js";
  import { speaker } from "#state/panelState.js";

  let { current = "" } = $props();

  let profiles = $derived($speaker?.profiles || []);
  let activeId = $derived($speaker?.active || "");
</script>

<nav class="hub-tabs asst-tabs" aria-label="Ассистенты и характеры">
  <a href="/assistants" class="hub-tab" class:on={current === "assistants"}>
    {@html icon("users")}
    <span>Галерея</span>
  </a>
  <span class="hub-tab-sep" aria-hidden="true"></span>
  {#each profiles as p (p.id)}
    <a href={profilePath(p.id)} class="hub-tab" class:on={current === "p:" + p.id}>
      <span class={"av sm c" + (p.color || 1)} class:av-img={Boolean(p.avatar)}>
        {#if p.avatar}<img src={p.avatar} alt="">{:else}{(p.name || "?").trim()[0] || "?"}{/if}
      </span>
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
