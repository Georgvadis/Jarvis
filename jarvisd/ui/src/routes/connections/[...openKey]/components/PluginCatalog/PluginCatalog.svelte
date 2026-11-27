<script>
  import { refreshConnectionCatalog, updateConnectionPlugins, uploadConnectionPlugin } from "#state/connections.js";
  import { icon } from "#helpers/core.js";
  import PluginRow from "./PluginRow/PluginRow.svelte";

  let { catalog } = $props();
  let installed = $derived(catalog.plugins.filter((plugin) => plugin.x.inst));
  let installedIds = $derived(new Set(installed.map((plugin) => plugin.id)));
  let official = $derived(catalog.plugins.filter((plugin) =>
    !plugin.x.inst && plugin.x.official && !installedIds.has(plugin.id)));
  let community = $derived(catalog.plugins.filter((plugin) => !plugin.x.inst && !plugin.x.official));
  let filePicker = $state();
  let catalogBusy = $state(false);
  let updating = $state(false);
  let uploading = $state(false);

  async function refresh() {
    if (catalogBusy) return;
    catalogBusy = true;
    try { await refreshConnectionCatalog(); }
    finally { catalogBusy = false; }
  }

  async function update() {
    if (updating) return;
    updating = true;
    try { await updateConnectionPlugins(); }
    finally { updating = false; }
  }

  async function importFile(event) {
    const input = event.currentTarget;
    const file = input.files?.[0];
    input.value = "";
    if (!file || uploading) return;
    uploading = true;
    try { await uploadConnectionPlugin(file); }
    finally { uploading = false; }
  }
</script>

{#snippet rows(plugins)}
  <div class="card cx-rows">
    {#each plugins as plugin (plugin.key)}
      <PluginRow {plugin} />
    {/each}
  </div>
{/snippet}

<section class="cn-group">
  <div class="cn-gh">
    <h2>Установлены</h2>
    <span class="meta">{installed.length}</span>
    <span class="spacer"></span>
    {#if catalog.canUpdate}
      <button class="btn sm primary" id="pl-update-all" type="button" disabled={updating} onclick={update}>
        {@html icon("refresh")}{updating ? "Обновляю…" : "Обновить всё"}
      </button>
    {/if}
  </div>
  {#if installed.length}
    {@render rows(installed)}
  {:else}
    <div class="card plg-empty">
      {@html icon("puzzle")}
      <p class="muted">Плагинов пока нет — выберите ниже или загрузите .zip</p>
    </div>
  {/if}
</section>

{#if official.length || catalog.officialError || !catalog.officialReady}
  <section class="cn-group">
    <div class="cn-gh"><h2>Доступные</h2><span class="meta">официальные</span></div>
    {#if official.length}
      {@render rows(official)}
    {:else if catalog.officialError}
      <p class="note bad" role="status">{catalog.officialError}</p>
    {:else}
      <p class="note">Загружаю каталог…</p>
    {/if}
  </section>
{/if}

{#if community.length || catalog.communityError}
  <section class="cn-group">
    <div class="cn-gh">
      <h2>От сообщества</h2>
      <span class="spacer"></span>
      <a class="lnk" href={"https://github.com/" + catalog.repo + "/blob/main/docs/PLUGINS.md"} target="_blank" rel="noopener">Как создать</a>
    </div>
    {#if community.length}
      {@render rows(community)}
    {/if}
    {#if catalog.communityError}
      <p class="note bad" role="status">{catalog.communityError}</p>
    {/if}
  </section>
{/if}

<div class="cx-foot">
  <span class="meta">{catalog.freeSpace ? `Свободно ${catalog.freeSpace} МБ` : ""}</span>
  <span class="spacer"></span>
  <input type="file" id="pl-file" bind:this={filePicker} accept=".zip,application/zip" hidden onchange={importFile}>
  <button class="btn sm" type="button" disabled={uploading} onclick={() => filePicker.click()}>
    {@html icon("upload")}{uploading ? "Ставлю…" : "Из .zip"}
  </button>
  <button class="btn sm ghost" type="button" id="pl-refresh" disabled={catalogBusy} onclick={refresh}>
    {@html icon("refresh")}Обновить каталог
  </button>
</div>
