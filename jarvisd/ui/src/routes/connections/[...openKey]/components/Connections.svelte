<script>
  import { onMount } from "svelte";
  import SystemNav from "#components/Shell/SystemNav.svelte";
  import {
    bindConnections, closeConnectionDialog, connNetTest, connOpen, CONN_GROUPS, selectConnectionTab,
  } from "#state/connections.js";
  import { icon } from "#helpers/core.js";
  import { plIcon } from "#state/plugins.js";
  import PluginSettings from "./ConnectionDialog/PluginSettings.svelte";
  import PluginWarning from "./ConnectionDialog/PluginWarning.svelte";
  import PluginCatalog from "./PluginCatalog/PluginCatalog.svelte";
  import ServiceEditor from "#components/ServiceEditor/ServiceEditor.svelte";
  import styles from "./Connections.module.css";

  const tabs = [
    ["svc", "Сервисы", "cpu"],
    ["pl", "Плагины", "puzzle"],
  ];

  const groupIcons = {
    voice: "sparkles",
    media: "music",
    system: "shield",
    home: "home",
  };

  const brandVisuals = {
    chatgpt: {
      tint: "#10a37f",
      svg: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3a4.5 4.5 0 0 1 4.1 2.6A4.5 4.5 0 0 1 20 10a4.5 4.5 0 0 1-1.3 3.2A4.5 4.5 0 0 1 16 20.5a4.5 4.5 0 0 1-4-.5 4.5 4.5 0 0 1-4 .5A4.5 4.5 0 0 1 5.3 13.2 4.5 4.5 0 0 1 4 10a4.5 4.5 0 0 1 3.9-4.4A4.5 4.5 0 0 1 12 3Z"/><circle cx="12" cy="12" r="2.2"/></svg>`,
    },
    openai: {
      tint: "#e2e8f0",
      svg: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 3 19.8 7.5 19.8 16.5 12 21 4.2 16.5 4.2 7.5"/><circle cx="12" cy="12" r="3"/></svg>`,
    },
    gemini: {
      tint: "#60a5fa",
      svg: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M12 2.5c.8 5.2 4.3 8.7 9.5 9.5-5.2.8-8.7 4.3-9.5 9.5-.8-5.2-4.3-8.7-9.5-9.5 5.2-.8 8.7-4.3 9.5-9.5Z"/></svg>`,
    },
    grok: {
      tint: "#f8fafc",
      svg: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="8"/><path d="M7.5 16.5 17 7"/></svg>`,
    },
    elevenlabs: {
      tint: "#f5a65b",
      svg: `<svg viewBox="0 0 24 24" fill="currentColor"><rect x="7" y="4.5" width="3.4" height="15" rx="1.4"/><rect x="13.6" y="4.5" width="3.4" height="15" rx="1.4"/></svg>`,
    },
    youtube: {
      tint: "#ff4444",
      svg: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><rect x="2.5" y="5" width="19" height="14" rx="4.5"/><polygon points="10 9 15.5 12 10 15" fill="currentColor" stroke="none"/></svg>`,
    },
    vpn: {
      tint: "#f2994a",
      svg: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 4.5 6.2v5.3c0 4.6 3.2 8.7 7.5 9.8 4.3-1.1 7.5-5.2 7.5-9.8V6.2L12 3Z"/><path d="M9.5 12.2 11.2 14l3.5-3.8"/></svg>`,
    },
  };

  function brandOf(item) {
    const id = item?.id || (item?.key || "").replace(/^svc:/, "");
    return brandVisuals[id] || null;
  }

  let { openKey } = $props();
  let modal = $state();
  let dialog = $state.raw(null);
  let phase = $state("hidden");
  let catalog = $state.raw(null);
  let counts = $derived({
    svc: catalog?.services.filter((service) => service.on).length || 0,
    pl: catalog?.plugins.filter((plugin) => plugin.x.inst).length || 0,
  });

  onMount(() => bindConnections(modal,
    (next) => { catalog = next; },
    (next) => { dialog = next; },
    (next) => { phase = next; },
    openKey,
  ));

  function navigateTabs(event, current) {
    const key = event.key;
    let tab;
    if (key === "Home") tab = "svc";
    else if (key === "End") tab = "pl";
    else if (key === "ArrowLeft" || key === "ArrowRight") tab = current === "svc" ? "pl" : "svc";
    else return;
    event.preventDefault();
    selectConnectionTab(tab);
    event.currentTarget.parentElement.querySelector(`[data-tab="${tab}"]`).focus();
  }
</script>

<SystemNav current="connections" />

<header class={styles.head}>
  <div class={styles.headLeft}>
    <h1>Сервисы и ИИ</h1>
    {#if catalog}
      <div class="hub-tabs" id="cx-tabs" role="tablist" aria-label="Подключения">
        {#each tabs as [id, name, ico] (id)}
          <button type="button" role="tab" id={"cx-tab-" + id} data-tab={id} class="hub-tab" class:on={catalog.tab === id}
            aria-selected={catalog.tab === id} aria-controls="conns" tabindex={catalog.tab === id ? 0 : -1}
            onclick={() => selectConnectionTab(id)} onkeydown={(event) => navigateTabs(event, id)}>
            {@html icon(ico)}
            <span>{name}</span>
            {#if counts[id]}<span class={styles.count}>{counts[id]}</span>{/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>
  <button class="btn {styles.pingBtn}" type="button" data-net onclick={(event) => connNetTest(event.currentTarget)}
    data-tip="Проверить пинг к подключённым облакам">
    {@html icon("radio")}<span>Проверить связь</span>
  </button>
</header>

{#if catalog}
  <div id="conns" class={styles.body} role="tabpanel" aria-labelledby={"cx-tab-" + catalog.tab}>
    {#if catalog.tab === "svc"}
      {#each CONN_GROUPS as [group, title] (group)}
        {@const services = catalog.services.filter((service) => service.group === group)
          .sort((a, b) => Number(b.on) - Number(a.on))}
        {@const connected = services.filter((service) => service.on).length}
        {#if services.length}
          <section class={styles.group}>
            <div class={styles.groupHead}>
              <span class={styles.groupIcon}>{@html icon(groupIcons[group] || "layers")}</span>
              <h2>{title}</h2>
              <span class={styles.groupBadge} class:active={connected > 0}>{connected} / {services.length}</span>
            </div>
            <div class={styles.grid}>
              {#each services as service (service.key)}
                {@const brand = brandOf(service)}
                <article class={styles.card} class:isOn={service.on} class:isBad={service.status.cls === "bad"}
                  style:--brand={brand?.tint || "var(--acc)"} data-item={service.key}>
                  <div class={styles.cardTop}>
                    <span class={styles.logo} aria-hidden="true">
                      {#if brand}
                        {@html brand.svg}
                      {:else if service.icon}
                        {@html icon(service.icon)}
                      {:else}
                        {service.mono}
                      {/if}
                    </span>
                    <div class={styles.cardTitle}>
                      <h3>{service.name}</h3>
                      <p>{service.about}</p>
                    </div>
                  </div>
                  <div class={styles.cardFoot}>
                    <span class={styles.pill} class:ok={service.status.cls === "ok"}
                      class:warn={service.status.cls === "warn"} class:bad={service.status.cls === "bad"}>
                      <span class={styles.dot}></span>
                      <span class={styles.pillText}>{service.status.text}</span>
                    </span>
                    <span class={styles.cta}>
                      {@html icon(service.on ? "tune" : "plus")}
                      <span>{service.on ? "Настроить" : "Подключить"}</span>
                    </span>
                  </div>
                  <button type="button" class={styles.serviceAction} aria-label={service.name}
                    onclick={() => connOpen(service.key)}></button>
                </article>
              {/each}
            </div>
          </section>
        {/if}
      {/each}
    {:else}
      <PluginCatalog {catalog} />
    {/if}
  </div>
{/if}

<div class="modal" class:hidden={phase === "hidden"} class:in={phase === "in"} class:out={phase === "out"}
  id="cn-modal" role="dialog" aria-modal="true" aria-labelledby="cn-mt" bind:this={modal}>
  <div class="modal-bg" role="presentation" data-close onclick={closeConnectionDialog}></div>
  <div class="modal-card cn-card" id="cn-card">
    {#if dialog?.warning}
      <PluginWarning warning={dialog.warning} close={closeConnectionDialog} />
    {:else if dialog?.item}
      {@const item = dialog.item}
      {@const brand = item.kind === "svc" ? brandOf(item) : null}
      {#key item.key}
        <div class="cn-mh">
          <span class={styles.logo} style:--brand={brand?.tint || "var(--acc)"}>
            {#if item.kind === "pl"}
              {@html plIcon(item.icon)}
            {:else if brand}
              {@html brand.svg}
            {:else if item.icon}
              {@html icon(item.icon)}
            {:else}
              {item.mono}
            {/if}
          </span>
          <div class="char-n">
            <h2 id="cn-mt">{item.x?.inst?.name || item.name}</h2>
            <span class={["st", dialog.status.cls]}>{dialog.status.text}</span>
          </div>
          <button class="ib" type="button" data-close aria-label="Закрыть" onclick={closeConnectionDialog}>{@html icon("x")}</button>
        </div>
        {#if item.kind === "svc"}
          <ServiceEditor service={item} state={dialog.state} refresh={dialog.refresh} />
        {:else}
          <PluginSettings plugin={item.x} services={dialog.services}
            training={item.id === "training" ? dialog.state.settings.wake_training : undefined} />
        {/if}
      {/key}
    {/if}
  </div>
</div>
