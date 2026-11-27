<script>
  import { tick } from "svelte";
  import { goto } from "$app/navigation";
  import { api, icon, toast } from "#helpers/core.js";
  import { profilePath } from "#helpers/paths.js";
  import { activity, refreshActivity, runScenario } from "#state/activity.js";
  import { refreshRoute, reload } from "#state/panelState.js";
  import { themeState, toggleThemeMode } from "#state/theme.js";

  let { open = false, onclose, profiles = [], activeProfileId = "" } = $props();
  let query = $state("");
  let selected = $state(0);
  let inputEl = $state();

  let items = $derived.by(() => {
    const list = [
      { id: "nav-home", group: "Навигация", label: "Гостиная", hint: "Очаг, сценарии, беседа и уют", icon: "flame", action: () => goto("/") },
      { id: "nav-assistants", group: "Навигация", label: "Студия ассистентов", hint: "Личности, портреты и характеры", icon: "users", action: () => goto("/assistants") },
      { id: "nav-player", group: "Навигация", label: "Медиа", hint: "Плеер, YouTube, AI-DJ, фон", icon: "music", action: () => goto("/player") },
      { id: "nav-dj", group: "Навигация", label: "AI-DJ", hint: "Медиа · Умный радиоэфир", icon: "disc", action: () => goto("/player?tab=dj") },
      { id: "nav-ambient", group: "Навигация", label: "Камин и радио", hint: "Медиа · Фоновые звуки", icon: "radio", action: () => goto("/player?tab=ambient") },
      { id: "nav-studio", group: "Навигация", label: "Студия", hint: "Подкаст, книги, игры, автоматизации", icon: "layers", action: () => goto("/studio") },
      { id: "nav-books", group: "Навигация", label: "Библиотека книг", hint: "Студия · Книги", icon: "book", action: () => goto("/studio?tab=books") },
      { id: "nav-games", group: "Навигация", label: "Голосовые игры", hint: "Студия · Мелодия, Детектив, Мафия", icon: "gamepad", action: () => goto("/studio?tab=games") },
      { id: "nav-schedule", group: "Навигация", label: "Таймеры и сценарии", hint: "Студия · Автоматизации", icon: "clock", action: () => goto("/studio?tab=schedule") },
      { id: "nav-memory", group: "Навигация", label: "Знания и память", hint: "Умный поиск и факты", icon: "brain", action: () => goto("/memory") },
      { id: "nav-notes", group: "Навигация", label: "Заметки CyberNotes", hint: "Знания · Заметки", icon: "note", action: () => goto("/memory?tab=notes") },
      { id: "nav-chars", group: "Навигация", label: "Характеры", hint: "Ассистенты · Системные роли", icon: "smile", action: () => goto("/characters") },
      { id: "nav-speaker", group: "Навигация", label: "Настройки · Колонка", hint: "Звук, микрофон, свет и ночь", icon: "speaker", action: () => goto("/speaker") },
      { id: "nav-speaker-int", group: "Навигация", label: "Настройки · Интеграции", hint: "ПК-компаньон, присутствие, Ollama, брифинг", icon: "link", action: () => goto("/speaker?tab=integrations") },
      { id: "nav-conn", group: "Навигация", label: "Настройки · Сервисы и ИИ", hint: "Ключи, YouTube, VPN и плагины", icon: "puzzle", action: () => goto("/connections") },
      { id: "nav-charts", group: "Навигация", label: "Настройки · Мониторинг", hint: "Узлы, CPU, память, температура", icon: "activity", action: () => goto("/charts") },
      { id: "nav-logs", group: "Навигация", label: "Настройки · Журнал", hint: "Логи событий", icon: "terminal", action: () => goto("/logs") },
      { id: "nav-sec", group: "Навигация", label: "Настройки · Бэкап и доступ", hint: "Пароль и резервная копия", icon: "shield", action: () => goto("/security") },

      { id: "sc-work", group: "Сценарии", label: "Режим работы", hint: "Фокус 25м + тихий Lo-Fi", icon: "briefcase", action: () => runScenario("work") },
      { id: "sc-evening", group: "Сценарии", label: "Вечерний режим", hint: "AI-DJ или джаз", icon: "moon", action: () => runScenario("evening") },
      { id: "sc-sleep", group: "Сценарии", label: "Режим сна", hint: "Тихий океан · таймер 20м", icon: "moon", action: () => runScenario("sleep") },
      { id: "sc-morning", group: "Сценарии", label: "Утренний режим", hint: "Джаз · средняя громкость", icon: "coffee", action: () => runScenario("morning") },
      ...($activity.custom_scenarios || []).map((sc) => ({
        id: `sc-${sc.id}`,
        group: "Сценарии",
        label: sc.name,
        hint: sc.phrase ? `«${sc.phrase}»` : `Шагов: ${sc.steps_count || 1}`,
        icon: sc.icon || "sparkles",
        action: () => runScenario(sc.id),
      })),

      {
        id: "act-talk", group: "Действия", label: "Слушать прямо сейчас", hint: "Активировать микрофон", icon: "mic",
        action: async () => { await api("POST", "/api/talk"); toast("Слушаю…"); },
      },
      {
        id: "act-timer-5", group: "Действия", label: "Таймер на 5 минут", hint: "Быстрый обратный отсчёт", icon: "clock",
        action: async () => {
          await api("POST", "/api/schedule/timer", { minutes: 5, label: "Таймер" });
          await refreshActivity();
          toast("Таймер на 5 мин");
        },
      },
      {
        id: "act-timer-15", group: "Действия", label: "Таймер на 15 минут", hint: "Быстрый обратный отсчёт", icon: "clock",
        action: async () => {
          await api("POST", "/api/schedule/timer", { minutes: 15, label: "Таймер" });
          await refreshActivity();
          toast("Таймер на 15 мин");
        },
      },
      {
        id: "act-stop", group: "Действия", label: "Остановить всё", hint: "Выключить звук, режимы, книги", icon: "stop",
        action: async () => { await api("POST", "/api/stop"); await refreshActivity(); toast("Всё остановлено"); },
      },
      {
        id: "act-screen", group: "Действия", label: "Посмотреть на экран ПК", hint: "Анализ скриншота", icon: "monitor",
        action: async () => {
          const r = await api("POST", "/api/pc/screen", { question: "Кратко опиши, что сейчас на экране." });
          toast(r.text || "Готово");
        },
      },
      {
        id: "act-vram", group: "Действия", label: "Освободить VRAM GPU", hint: "Выгрузить Whisper/Qwen", icon: "cpu",
        action: async () => { await api("POST", "/api/gpu-unload"); toast("VRAM освобождена"); },
      },
      {
        id: "act-theme", group: "Действия",
        label: $themeState.mode === "dark" ? "Включить светлую тему" : "Включить тёмную тему",
        hint: "Оформление", icon: $themeState.mode === "dark" ? "sun" : "moon",
        action: () => toggleThemeMode(),
      },
    ];

    for (const p of profiles) {
      const isCur = p.id === activeProfileId;
      list.push({
        id: "prof-" + p.id,
        group: "Ассистенты",
        label: p.name,
        hint: isCur ? "Активный · открыть настройки" : `Переключиться на ${p.name}`,
        icon: "sparkles",
        action: async () => {
          if (isCur) {
            await goto(profilePath(p.id));
          } else {
            await api("POST", "/api/active", { id: p.id });
            await reload();
            toast(`Активен: ${p.name}`);
            refreshRoute();
          }
        },
      });
    }
    return list;
  });

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((item) =>
      item.label.toLowerCase().includes(q)
      || item.hint.toLowerCase().includes(q)
      || item.group.toLowerCase().includes(q)
    );
  });

  $effect(() => {
    if (open) {
      query = "";
      selected = 0;
      void tick().then(() => inputEl?.focus());
    }
  });

  $effect(() => {
    if (selected >= filtered.length) selected = Math.max(0, filtered.length - 1);
  });

  async function run(item) {
    if (!item) return;
    onclose?.();
    try {
      await item.action();
    } catch (error) {
      toast(error.message, true);
    }
  }

  function onKeydown(event) {
    if (!open) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (filtered.length) selected = (selected + 1) % filtered.length;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      if (filtered.length) selected = (selected - 1 + filtered.length) % filtered.length;
    } else if (event.key === "Enter") {
      event.preventDefault();
      void run(filtered[selected]);
    } else if (event.key === "Escape") {
      event.preventDefault();
      onclose?.();
    }
  }
</script>

{#if open}
  <div class="cmdk-bg" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) onclose?.(); }}>
    <div class="cmdk" role="dialog" aria-modal="true" aria-label="Быстрые команды" tabindex="-1" onkeydown={onKeydown}>
      <div class="cmdk-in">
        {@html icon("search")}
        <input
          bind:this={inputEl}
          type="text"
          placeholder="Перейти в раздел, включить сценарий, сменить ассистента…"
          bind:value={query}
          aria-label="Поиск команды"
        >
        <span class="cmdk-kbd">Esc</span>
      </div>
      <div class="cmdk-list">
        {#each filtered as item, idx (item.id)}
          {#if idx === 0 || filtered[idx - 1].group !== item.group}
            <div class="cmdk-grp">{item.group}</div>
          {/if}
          <button
            type="button"
            class="cmdk-item"
            class:on={idx === selected}
            onclick={() => void run(item)}
            onmouseenter={() => { selected = idx; }}
          >
            {@html icon(item.icon)}
            <span>{item.label}</span>
            <small>{item.hint}</small>
          </button>
        {:else}
          <p class="empty">Ничего не найдено</p>
        {/each}
      </div>
    </div>
  </div>
{/if}
