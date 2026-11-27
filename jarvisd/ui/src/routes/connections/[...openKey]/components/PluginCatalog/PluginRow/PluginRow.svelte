<script>
  import { connOpen, installConnectionPlugin, removeConnectionPlugin, setConnectionPluginEnabled } from "#state/connections.js";
  import { icon } from "#helpers/core.js";
  import { plIcon, plSize } from "#state/plugins.js";
  import styles from "./PluginRow.module.css";

  const iconTints = {
    voice: "#f5a65b",
    music: "#ff4444",
    mic: "#38bdf8",
    link: "#60a5fa",
    shield: "#a78bfa",
    home: "#34d399",
    puzzle: "#f2994a",
  };

  let { plugin } = $props();
  let installed = $derived(plugin.x.inst);
  let tint = $derived(iconTints[plugin.icon] || "#f2994a");
  let busy = $state(false);

  async function run(action) {
    if (busy) return;
    busy = true;
    try { await action(); }
    finally { busy = false; }
  }

  function changeEnabled(event) {
    const input = event.currentTarget;
    const enabled = input.checked;
    input.checked = installed.enabled;
    void run(() => setConnectionPluginEnabled(installed.id, enabled));
  }
</script>

<article class={styles.card} class:isOn={installed && installed.enabled} class:isOff={installed && !installed.enabled}
  class:isBad={plugin.status.cls === "bad"} style:--brand={tint} data-item={plugin.key}>
  <div class={styles.top}>
    <span class={styles.logo} aria-hidden="true">{@html plIcon(plugin.icon)}</span>
    <div class={styles.info}>
      <div class={styles.titleRow}>
        <button class={styles.title} type="button" onclick={() => connOpen(plugin.key)}
          aria-label={plugin.name + (installed ? " — настроить" : " — подробнее")}>
          {plugin.name}
        </button>
        {#if installed?.version}
          <span class={styles.ver}>v{installed.version}</span>
        {:else if plugin.x.size}
          <span class={styles.ver}>{plSize(plugin.x.size)}</span>
        {/if}
      </div>
      {#if plugin.about}
        <p class={styles.about}>{plugin.about}</p>
      {/if}
    </div>
    {#if installed}
      <label class="sw {styles.sw}" data-tip={installed.enabled ? "Выключить" : "Включить"}>
        <input type="checkbox" data-pl-on={installed.id} checked={installed.enabled} disabled={busy}
          aria-label={(installed.enabled ? "Выключить " : "Включить ") + plugin.name} onchange={changeEnabled}>
        <span class="tr"></span>
      </label>
    {/if}
  </div>

  <div class={styles.foot}>
    {#if installed}
      <span class={styles.pill} class:ok={plugin.status.cls === "ok"}
        class:warn={plugin.status.cls === "warn"} class:bad={plugin.status.cls === "bad"}>
        <span class={styles.dot}></span>
        <span class={styles.pillText}>{plugin.status.text}</span>
      </span>
      <div class={styles.actions}>
        <button class="btn sm ghost" type="button" onclick={() => connOpen(plugin.key)}>
          {@html icon("tune")}<span>Настроить</span>
        </button>
        <button class="ib danger {styles.del}" type="button" disabled={busy}
          data-tip="Удалить плагин" aria-label={"Удалить " + plugin.name}
          onclick={() => run(() => removeConnectionPlugin(installed.id))}>
          {@html icon("trash")}
        </button>
      </div>
    {:else}
      <span class={styles.pill}>
        <span class={styles.pillText}>{plugin.x.official ? "Официальный" : "Сообщество"}</span>
      </span>
      <button class="btn sm primary {styles.install}" type="button" disabled={busy}
        onclick={() => run(() => installConnectionPlugin(plugin.x))}>
        {#if busy}<span class="spin"></span>Ставлю…{:else}{@html icon("plus")}<span>Установить</span>{/if}
      </button>
    {/if}
  </div>
</article>
