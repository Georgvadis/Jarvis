<script module>
  const ranges = [
    [900, "15 мин"],
    [3600, "1 ч"],
    [21600, "6 ч"],
    [86400, "24 ч"],
  ];

  const percentScale = [0, 100];
  const metrics = [
    { key: "cpu", title: "Процессор", unit: "%", subtitle: "загрузка всех ядер", scale: percentScale, skipFirst: true },
    { key: "temp", title: "Температура", unit: "°C", subtitle: "процессор", decimals: 1, zeroIsMissing: true },
    { key: "mem", title: "Память занята", unit: "МБ" },
    { key: "wifi", title: "Сигнал Wi-Fi", unit: "дБм", subtitle: "ближе к нулю — лучше", zeroIsMissing: true },
    { key: "disk", title: "Свободно на /data", unit: "МБ", subtitle: "меньше 5 МБ — обновления могут не встать" },
    { key: "me", title: "Сам Джарвис", unit: "%", subtitle: "сколько процессора колонки занимает Джарвис", scale: percentScale, skipFirst: true },
  ];

  let selectedRange = 3600;
</script>

<script>
  import { onMount } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { getPanel } from "#state/panelContext.js";
  import { S } from "#state/panelState.js";
  import Chart from "./Chart/Chart.svelte";
  import styles from "./Charts.module.css";

  let range = $state(selectedRange);
  let points = $state.raw([]);
  let memoryTotal = $state(0);
  let animation = $state(0);
  let loading = $state(true);
  let loadError = $state("");
  let health = $state.raw(null);
  let healthBusy = $state(false);
  let lastRequest = 0;
  let request;
  let healthController = new AbortController();
  const { onPoll } = getPanel();

  function formatUptime(sec) {
    const s = Math.max(0, Math.floor(sec || 0));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    if (h > 0) return `${h}ч ${m}м`;
    if (m > 0) return `${m}м`;
    return `${s}с`;
  }

  let healthTiles = $derived.by(() => {
    if (!health) return [];
    const sp = health.speaker || {};
    const gpu = health.voice_gpu || {};
    const cn = health.cybernotes || {};
    const pc = health.pc_companion || {};
    const tg = health.telegram || {};
    const vpn = health.vpn || {};

    const loadedModels = [gpu.whisper_loaded && "Whisper", gpu.qwen_loaded && "Qwen"].filter(Boolean).join("+");
    return [
      {
        id: "speaker",
        label: "Колонка",
        ic: "speaker",
        tone: sp.ok ? (sp.mic ? "ok" : "warn") : "bad",
        detail: sp.ok ? `${sp.version || ""} · аптайм ${formatUptime(sp.uptime_s)}${sp.night ? " · ночь" : ""}` : "Офлайн",
      },
      {
        id: "gpu",
        label: "GPU-сервер",
        ic: "cpu",
        tone: gpu.ok ? "ok" : "bad",
        detail: gpu.ok ? `${gpu.latency_ms ?? 0} мс · ${loadedModels || "готов"}` : (gpu.error || "Недоступен"),
      },
      {
        id: "notes",
        label: "CyberNotes",
        ic: "note",
        tone: cn.available ? "ok" : "bad",
        detail: cn.available ? `Доступно · ${cn.total_notes ?? 0} заметок` : "Каталог недоступен",
      },
      {
        id: "pc",
        label: "ПК-компаньон",
        ic: "monitor",
        tone: !pc.configured ? "off" : pc.ok ? "ok" : "bad",
        detail: !pc.configured ? "Не настроен" : pc.ok ? `${pc.latency_ms ?? 0} мс · ${pc.host || pc.os || "онлайн"}` : (pc.error || "Офлайн"),
      },
      {
        id: "tg",
        label: "Telegram-бот",
        ic: "send",
        tone: tg.configured ? "ok" : "off",
        detail: tg.configured ? "Подключён" : "Не настроен",
      },
      {
        id: "vpn",
        label: "Сеть / VPN",
        ic: "globe",
        tone: vpn.ok ? "ok" : "bad",
        detail: vpn.mode === "tunnel" ? "VPN-туннель" : vpn.mode === "blocked" ? (vpn.error || "Заблокирован") : "Напрямую",
      },
    ];
  });

  async function loadHealth(notify = false) {
    if (healthBusy) return;
    healthBusy = true;
    try {
      health = await api("GET", "/api/stack-health", undefined, { signal: healthController.signal });
      if (notify) toast("Состояние стека обновлено");
    } catch (error) {
      if (!healthController.signal.aborted && notify) toast(error.message, true);
    } finally {
      healthBusy = false;
    }
  }

  onMount(() => {
    const unsubscribe = onPoll(refresh);
    void refresh(true);
    void loadHealth(false);
    return () => {
      unsubscribe();
      request?.abort();
      healthController.abort();
    };
  });

  async function refresh(force = false) {
    if (!force && (request || Date.now() - lastRequest < 15000)) return;

    request?.abort();
    const controller = new AbortController();
    const { signal } = controller;
    request = controller;
    lastRequest = Date.now();

    try {
      const data = await api("GET", "/api/stats?range=" + range, undefined, { signal });
      points = data.points || [];
      memoryTotal = S.sys?.mem_total_mb || 0;
      loadError = "";
      if (force) animation++;
    } catch (error) {
      if (!signal.aborted && !points.length) loadError = error.message;
    } finally {
      if (request === controller) {
        request = undefined;
        loading = false;
      }
    }
  }

  function selectRange(seconds) {
    if (seconds !== range) {
      range = selectedRange = seconds;
      points = [];
      loading = true;
      loadError = "";
    }
    void refresh(true);
  }
</script>

<header class="ph ph-row">
  <div>
    <h1>Графики</h1>
    <p class="sub">Раз в 15 секунд за последние сутки. История живёт в памяти колонки и начинается заново после перезапуска Джарвиса</p>
  </div>
  <div class="seg" role="group" aria-label="Период графиков">
    {#each ranges as [seconds, label] (seconds)}
      <button type="button" class:on={seconds === range} aria-pressed={seconds === range}
        onclick={() => selectRange(seconds)}>{label}</button>
    {/each}
  </div>
</header>

{#if loadError}
  <p class="err" role="alert">{loadError}</p>
{/if}

<section class="card health-card" aria-label="Состояние стека">
  <div class="card-head">
    <h2>{@html icon("cpu")} Состояние стека</h2>
    <button type="button" class="btn sm ghost" disabled={healthBusy} onclick={() => void loadHealth(true)}>
      {@html icon("refresh")} Проверить
    </button>
  </div>
  {#if healthTiles.length}
    <div class="health-grid">
      {#each healthTiles as tile (tile.id)}
        <div class="health-tile">
          <div class="health-top">
            <span class="health-ic">{@html icon(tile.ic)}</span>
            <b>{tile.label}</b>
            <span class="health-dot" data-tone={tile.tone} aria-hidden="true"></span>
          </div>
          <small>{tile.detail}</small>
        </div>
      {/each}
    </div>
  {:else}
    <p class="empty-msg">Проверяю узлы…</p>
  {/if}
</section>

<div class={[styles.charts, "stagger"]} aria-busy={loading}>
  {#each metrics as metric (metric.key)}
    <Chart {metric} {points} {animation} ready={!loading && !loadError}
      subtitle={metric.key === "mem" && memoryTotal ? `из ${memoryTotal} МБ` : metric.subtitle} />
  {/each}
</div>
