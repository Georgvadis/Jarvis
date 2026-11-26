<script module>
  const timezones = [
    ["Europe/Kaliningrad", "Калининград (UTC+2)"],
    ["Europe/Moscow", "Москва (UTC+3)"],
    ["Europe/Minsk", "Минск (UTC+3)"],
    ["Europe/Kyiv", "Киев"],
    ["Europe/Samara", "Самара (UTC+4)"],
    ["Asia/Yekaterinburg", "Екатеринбург (UTC+5)"],
    ["Asia/Almaty", "Алматы (UTC+5)"],
    ["Asia/Omsk", "Омск (UTC+6)"],
    ["Asia/Novosibirsk", "Новосибирск (UTC+7)"],
    ["Asia/Krasnoyarsk", "Красноярск (UTC+7)"],
    ["Asia/Irkutsk", "Иркутск (UTC+8)"],
    ["Asia/Yakutsk", "Якутск (UTC+9)"],
    ["Asia/Vladivostok", "Владивосток (UTC+10)"],
    ["Asia/Magadan", "Магадан (UTC+11)"],
    ["Asia/Kamchatka", "Камчатка (UTC+12)"],
    ["Europe/Berlin", "Берлин"],
    ["Europe/London", "Лондон"],
    ["UTC", "UTC"],
  ];

  const languages = [["ru", "Русский"], ["en", "English"], ["uk", "Українська"], ["", "Определять самому"]];
  const soundThemes = [
    ["scifi", "Sci-Fi (Джарвис)"],
    ["soft", "Мягкий"],
    ["retro", "Ретро"],
    ["minimal", "Минимал"],
  ];
  const services = [
    ["alice", "Алиса", "Джарвис передаёт команду штатной Алисе — она умеет всё, что подключено к Яндексу", () => true],
    ["yandexmusic", "Яндекс Музыка", "«Включи музыку», «поставь песню…» Джарвис отдаёт Алисе — играет из Яндекс Музыки. Выключено — всё играет с YouTube", () => true],
    ["smartlife", "Smart Life", "Напрямую по домашней сети — быстрее всего", (state) => state.smartlife?.signed_in],
    ["xiaomi", "Xiaomi Home", "Через облако Xiaomi: пылесос, увлажнитель, лампы Mi Home", (state) => state.xiaomi?.signed_in],
    ["yandex", "Яндекс Умный дом", "Сам Джарвис через API Яндекса, без Алисы — нужен токен", (state) => !!state.keys?.yandex],
    ["homeassistant", "Home Assistant", "Ваш сервер Home Assistant", (state) => state.home_assistant?.configured],
  ];

  const ranges = [
    {
      key: "wake_sensitivity", label: "Чувствительность", min: 1, max: 10, step: 1,
      hint: "Выше — слышит издалека и тихо, но чаще срабатывает зря (например, на разговор в Discord). Ниже — реже ошибается, но звать нужно чётче.",
      low: "осторожно", high: "слышит всё", format: String, message: "Чувствительность сохранена",
    },
    {
      key: "mic_gain_db", label: "Общее усиление микрофона", min: 0, max: 30, step: 3,
      hint: "Для колонок, которые слышат вас только вблизи (так бывает на Станции Мини 2: её обработка отдаёт голос тихо). Усиливает звук для всего: слова активации, ИИ и записей для обучения. Прибавляйте по 6 дБ и проверяйте; слишком много — чаще ложные срабатывания. На Лайт обычно не нужно.",
      low: "как есть", high: "+30 дБ", format: (value) => `+${value} дБ`, message: "Общее усиление микрофона сохранено",
    },
    {
      key: "mic_boost_db", label: "Усиление микрофона для ИИ", min: 0, max: 18, step: 3,
      hint: "Если модель плохо слышит и переспрашивает, хотя вы говорите не тихо (так бывает на Станции Лайт 2: её обработка отдаёт голос тише), прибавьте 6–12 дБ. На слово активации это не влияет. Уровень голоса за каждый разговор пишется в журнал: «микрофон: за разговор голос …».",
      low: "как есть", high: "+18 дБ", format: (value) => `+${value} дБ`, message: "Усиление микрофона сохранено",
    },
    {
      key: "vad_threshold", label: "Порог тишины (VAD)", min: 100, max: 3000, step: 50,
      hint: "Минимальная громкость, при которой классический пайплайн считает звук речью. Поставьте чуть выше фонового шума на индикаторе ниже.",
      low: "тише (100)", high: "громче (3000)", format: String, message: "Порог тишины сохранён",
    },
    {
      key: "vad_silence_ms", label: "Пауза конца фразы", min: 300, max: 2500, step: 100,
      hint: "Сколько ждать тишины после вашей речи перед отправкой в распознавание.",
      low: "быстро (0.3 с)", high: "спокойно (2.5 с)", format: (value) => `${(value / 1000).toFixed(1)} с`, message: "Пауза конца фразы сохранена",
    },
  ];

  const nightDefaults = { enabled: false, from: "23:00", to: "08:00", voice: 20, bright: 10, speaker: 3 };
</script>

<script>
  import { onMount, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { api, copyText, icon, jelly, tip, toast } from "#helpers/core.js";
  import { S, reload } from "#state/panelState.js";
  import { loadWakeProfiles, modelLabel, selectWakeProfile } from "#helpers/wake-training.js";
  import Lighting from "./Lighting/Lighting.svelte";
  import NightMode from "./NightMode/NightMode.svelte";
  import styles from "./Speaker.module.css";

  const controller = new AbortController();
  const { signal } = controller;
  const pending = new SvelteSet();
  let writes = Promise.resolve();
  let state = $state.raw(S);
  let profiles = $state.raw(null);
  let profileError = $state("");
  let selected = $state("");
  let rangesInEdit = $state({});
  let wakeReplies = $state.raw(S.wake_replies || null);
  let showWakeConfirm = $state(false);
  let wakeBusy = $state(false);
  let wakeTimer = null;
  let micPeak = $state(0);
  let micTimer = null;
  let pcUrlInput = $state(untrack(() => S.settings.pc_agent_url || ""));
  let pcTokenInput = $state("");
  let pcStatus = $state("");
  let pcScreenText = $state("");
  let phoneIpInput = $state(untrack(() => S.settings.presence_phone_ip || ""));
  let awayMinInput = $state(untrack(() => S.settings.presence_away_min || 30));
  let greetingInput = $state(untrack(() => S.settings.presence_greeting || ""));
  let presenceInfo = $state({ state: "unknown" });
  let notesInfo = $state({ available: false, total_notes: 0 });
  let settings = $derived(state.settings);
  let vadThreshold = $derived(rangesInEdit.vad_threshold ?? settings.vad_threshold ?? 600);
  let word = $derived(profiles?.find((profile) => profile.id === selected)?.phrase || settings.wake_word || "Джарвис");
  let timezone = $derived(settings.timezone || "UTC");
  let language = $derived(settings.language ?? "ru");
  let night = $derived({ ...nightDefaults, ...settings.night });

  function stopWakePoll() {
    if (wakeTimer) {
      clearTimeout(wakeTimer);
      wakeTimer = null;
    }
  }

  async function pollWakeReplies() {
    stopWakePoll();
    try {
      const next = await api("GET", "/api/wake-replies", undefined, { signal });
      if (signal.aborted) return;
      wakeReplies = next;
      if (next?.running) {
        wakeTimer = setTimeout(() => void pollWakeReplies(), 1200);
      } else {
        await refresh();
      }
    } catch {
      if (!signal.aborted) wakeTimer = setTimeout(() => void pollWakeReplies(), 2500);
    }
  }

  onMount(() => {
    if (settings.wake_training) {
      loadWakeProfiles(signal).then((result) => {
        profiles = result.profiles;
        selected = result.selected;
      }).catch((error) => {
        if (!signal.aborted) profileError = error.message;
      });
    }
    if (wakeReplies?.running) {
      wakeTimer = setTimeout(() => void pollWakeReplies(), 1000);
    }
    const pollMic = async () => {
      try {
        const res = await api("GET", "/api/mic-level", undefined, { signal });
        if (!signal.aborted && typeof res?.peak === "number") micPeak = res.peak;
      } catch { /* ignore */ }
      if (!signal.aborted) micTimer = setTimeout(() => void pollMic(), 250);
    };
    micTimer = setTimeout(() => void pollMic(), 250);
    void loadExtraStatuses();
    return () => {
      stopWakePoll();
      clearTimeout(micTimer);
      controller.abort();
    };
  });

  async function loadExtraStatuses() {
    try {
      const [pr, ns] = await Promise.all([
        api("GET", "/api/presence", undefined, { signal }),
        api("GET", "/api/notes/status", undefined, { signal }),
      ]);
      if (!signal.aborted) {
        if (pr) presenceInfo = pr;
        if (ns) notesInfo = ns;
      }
    } catch { /* ignore */ }
  }

  async function refresh() {
    await reload();
    state = S;
    if (S.wake_replies) wakeReplies = S.wake_replies;
  }

  async function toggleWakeReply(value) {
    await save("wake_reply_enabled", () => ({ wake_reply_enabled: value }), value ? "Быстрый отклик включён" : "Быстрый отклик выключен");
    if (value && wakeReplies && wakeReplies.can_generate && wakeReplies.ready < wakeReplies.total && !wakeReplies.running) {
      showWakeConfirm = true;
    } else if (!value) {
      showWakeConfirm = false;
    }
  }

  async function startWakeGenerate() {
    if (wakeBusy) return;
    wakeBusy = true;
    showWakeConfirm = false;
    try {
      wakeReplies = await api("POST", "/api/wake-replies/generate", undefined, { signal });
      if (wakeReplies?.running) {
        toast("Генерация откликов запущена");
        wakeTimer = setTimeout(() => void pollWakeReplies(), 1000);
      } else {
        toast("Все отклики уже готовы в кеше");
      }
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      wakeBusy = false;
    }
  }

  async function cancelWakeGenerate() {
    if (wakeBusy) return;
    wakeBusy = true;
    try {
      wakeReplies = await api("POST", "/api/wake-replies/cancel", undefined, { signal });
      toast("Останавливаю генерацию откликов");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      wakeBusy = false;
    }
  }

  async function clearWakeCache() {
    if (wakeBusy || !confirm("Очистить сохранённые быстрые отклики для текущего ассистента?")) return;
    wakeBusy = true;
    showWakeConfirm = false;
    try {
      wakeReplies = await api("DELETE", "/api/wake-replies", undefined, { signal });
      toast("Кеш откликов очищен");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      wakeBusy = false;
    }
  }

  function enqueue(key, write) {
    if (pending.has(key)) return writes;
    pending.add(key);
    const operation = writes.then(write).catch((error) => {
      if (!signal.aborted) toast(error.message, true);
    }).finally(() => pending.delete(key));
    writes = operation;
    return operation;
  }

  function save(key, getChanges, message) {
    return enqueue(key, async () => {
      const changes = getChanges();
      await api("PUT", "/api/settings", changes);
      const next = { ...settings, ...changes };
      if (changes.home_use) next.home_use = { ...settings.home_use, ...changes.home_use };
      state = { ...state, settings: next };
      await refresh();
      if (!signal.aborted) toast(typeof message === "function" ? message(changes) : message);
    });
  }

  function saveToggle(key, value, onMessage, offMessage) {
    void save(key, () => ({ [key]: value }), value ? onMessage : offMessage);
  }

  function saveService(id, name, value) {
    void save(id, () => ({ home_use: { [id]: value } }), `${name}: ${value ? "используется" : "не используется"}`);
  }

  function saveText(event, key) {
    const input = event.currentTarget;
    const value = input.value.trim();
    input.value = value;
    const message = key === "city" ? "Город сохранён" : value ? `Колонка стоит: ${value}` : "Комната колонки не задана";
    void save(key, () => ({ [key]: value }), message);
  }

  function saveTimezone(event) {
    const input = event.currentTarget;
    const value = input.value;
    input.value = timezone;
    void save("timezone", () => ({ timezone: value }), "Часовой пояс сохранён — время в журнале тоже по нему");
  }

  function saveLanguage(value, label) {
    void save("language", () => ({ language: value }), value ? `Язык: ${label}` : "Язык определяется сам");
  }

  function mountRange(node, range) {
    const { key, label, min, max, step, format, message } = range;
    const current = () => rangesInEdit[key] ?? settings[key] ?? min;
    const slider = jelly(node, {
      label, min, max, step, format, value: current(),
      disabled: () => pending.has(key),
      onInput: (value) => { rangesInEdit[key] = value; },
      async onChange(value) {
        if (pending.has(key)) return;
        rangesInEdit[key] = value;
        await save(key, () => ({ [key]: value }), message);
        delete rangesInEdit[key];
        if (node.isConnected) slider.set(settings[key] ?? min);
      },
    });
    return { destroy: () => slider.destroy() };
  }

  function changeProfile(event) {
    const input = event.currentTarget;
    const id = input.value;
    input.value = selected;
    void enqueue("wake_profile", async () => {
      await selectWakeProfile(id);
      selected = id;
      await refresh();
    });
  }

  function saveNight(key, value) {
    return save("night." + key, () => ({ night: { ...night, [key]: value } }), ({ night }) =>
      night.enabled ? `Ночной режим: ${night.from}–${night.to}` : "Ночной режим выключен");
  }

  function generateNotifyToken() {
    const bytes = new Uint8Array(12);
    crypto.getRandomValues(bytes);
    const token = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
    void save("notify_token", () => ({ notify_token: token }), "Токен уведомлений обновлён");
  }

  async function copyNotifyCurl() {
    const tok = settings.notify_token || "<ТОКЕН>";
    const host = typeof location !== "undefined" ? location.origin : "http://192.168.1.20:8080";
    const cmd = `curl -X POST "${host}/api/notify" -H "Authorization: Bearer ${tok}" -H "Content-Type: application/json" -d '{"text":"Готово!"}'`;
    try {
      await copyText(cmd);
      toast("Команда curl скопирована");
    } catch {
      toast("Не удалось скопировать", true);
    }
  }

  async function savePcSettings() {
    const body = { pc_agent_url: pcUrlInput.trim() };
    if (pcTokenInput.trim()) body.pc_agent_token = pcTokenInput.trim();
    await save("pc_agent", () => body, "Настройки ПК-компаньона сохранены");
    pcTokenInput = "";
  }

  async function pingPc() {
    pcStatus = "Проверяю…";
    try {
      const r = await api("GET", "/api/pc/ping", undefined, { signal });
      pcStatus = `${r.host || "ПК"} (${r.os || ""}) · ${r.capture_backend || "ok"}`;
      toast("ПК на связи");
    } catch (e) {
      pcStatus = "";
      toast(e.message, true);
    }
  }

  async function testPcScreen() {
    pcScreenText = "Анализирую экран…";
    try {
      const r = await api("POST", "/api/pc/screen", {}, { signal });
      pcScreenText = r.text || "";
    } catch (e) {
      pcScreenText = "";
      toast(e.message, true);
    }
  }

  async function savePresence() {
    await save("presence", () => ({
      presence_phone_ip: phoneIpInput.trim(),
      presence_away_min: Number(awayMinInput) || 30,
      presence_greeting: greetingInput.trim(),
    }), "Присутствие сохранено");
    void loadExtraStatuses();
  }

  async function testPresence() {
    try {
      await api("POST", "/api/presence/test", {}, { signal });
      toast("Приветствие озвучено");
    } catch (e) {
      toast(e.message, true);
    }
  }
</script>

{#snippet settingSection(title, description, fields, ic = "")}
  <section class="sec">
    <div class="sec-h"><h2>{#if ic}{@html icon(ic)}{/if}{title}</h2>{#if description}<p>{description}</p>{/if}</div>
    <div class="sec-b">{@render fields()}</div>
  </section>
{/snippet}

{#snippet toggle(label, key, checked, onChange, hint = "")}
  <label class="sw">
    <input type="checkbox" {checked} disabled={pending.has(key)} onchange={(event) => {
      const input = event.currentTarget;
      const next = input.checked;
      input.checked = checked;
      onChange(next);
    }}>
    <span class="tr"></span>
    {label} {#if hint}{@html tip(hint)}{/if}
  </label>
{/snippet}

<header class="ph">
  <h1>Колонка</h1>
  <p class="sub">Изменения сохраняются сразу</p>
</header>

{#snippet homeFields()}
  <div class={styles.services}>
    {#each services as [id, name, description, ready] (id)}
      {@const available = !!ready(state)}
      {@const checked = available && settings.home_use?.[id] !== false}
      <label class={["sw hu", { off: !available }]}>
        <input type="checkbox" {checked} disabled={!available || pending.has(id)}
          onchange={(event) => {
            const input = event.currentTarget;
            const next = input.checked;
            input.checked = checked;
            saveService(id, name, next);
          }}>
        <span class="tr"></span>
        <span class="hu-t">
          <b>{name}</b>
          <small>{description}{#if !available} · <a href="/connections">сначала подключите</a> — тогда галочка включится сама{/if}</small>
        </span>
      </label>
    {/each}
  </div>
{/snippet}
{@render settingSection("Умный дом и музыка",
  "Каналы управления домом и источник музыки",
  homeFields, "home")}

{#snippet locationFields()}
  <div class="f">
    <label for="s-room">Комната, где стоит колонка {@html tip("«Выключи свет», «включи кондиционер» без названия комнаты — Джарвис сделает это здесь, как Алиса.")}</label>
    <input id="s-room" type="text" list="s-rooms" value={settings.speaker_room || ""} disabled={pending.has("speaker_room")}
      placeholder="Например, Зал" autocomplete="off" onchange={(event) => saveText(event, "speaker_room")}>
    <datalist id="s-rooms">
      {#each state.rooms || [] as room}<option value={room}></option>{/each}
    </datalist>
  </div>
  <div class="grid g2">
    <div class="f">
      <label for="s-city">Город</label>
      <input id="s-city" type="text" value={settings.city || ""} disabled={pending.has("city")}
        placeholder="Например, Москва" onchange={(event) => saveText(event, "city")}>
    </div>
    <div class="f">
      <label for="s-tz">Часовой пояс</label>
      <select id="s-tz" value={timezone} disabled={pending.has("timezone")} onchange={saveTimezone}>
        {#if !timezones.some(([id]) => id === timezone)}
          <option value={timezone}>{timezone}</option>
        {/if}
        {#each timezones as [id, label] (id)}<option value={id}>{label}</option>{/each}
      </select>
    </div>
  </div>
{/snippet}
{@render settingSection("Локация",
  "Комната, город и часовой пояс",
  locationFields, "globe")}

{#snippet wakeFields()}
  {#if settings.wake_training}
    <div class="f">
      <label for="s-wake-profile">Выбранная фраза</label>
      <select id="s-wake-profile" value={selected} disabled={!profiles || pending.has("wake_profile")} onchange={changeProfile}>
        {#each profiles || [] as profile (profile.id)}
          <option value={profile.id}>{modelLabel(profile)}</option>
        {:else}
          <option value={selected}>{word}</option>
        {/each}
      </select>
      {#if profileError}<p class="err" role="alert">{profileError}</p>{/if}
      <a href="/training">Обучение слова</a>
    </div>
  {/if}
  {@render toggle(`Откликаться на «${word}»`, "wake_enabled", !!settings.wake_enabled,
    (value) => saveToggle("wake_enabled", value, "Джарвис снова слушает", "Джарвис больше не откликается на слово"),
    "Выключите, если Джарвис нужен только по кнопке «Поговорить». Алиса работает в любом случае.")}
  {@render toggle(`Перебивать ответ словом «${word}»`, "wake_interrupt", settings.wake_interrupt !== false,
    (value) => saveToggle("wake_interrupt", value, "Слово активации перебивает ответ", "Слово активации не перебивает ответ"),
    "Если колонка иногда обрывает свой ответ на первом слове при высоком усилении микрофона, выключите этот тумблер. Команда «стоп» продолжает работать.")}
  {@render toggle("Быстрый голосовой отклик", "wake_reply_enabled", settings.wake_reply_enabled !== false,
    (value) => void toggleWakeReply(value),
    "После слова активации Джарвис сразу говорит «слушаю», «угу» или другую короткую фразу из локального кеша. В фоне фразы автоматически не генерируются, чтобы не тратить лимит API.")}
  {#if wakeReplies && (settings.wake_reply_enabled !== false || wakeReplies.ready > 0 || wakeReplies.running)}
    {@const missingCount = Math.max(0, (wakeReplies.total || 12) - (wakeReplies.ready || 0))}
    <div class="wr-box">
      <div class="wr-head">
        <span>
          {#if wakeReplies.running}
            Генерирую отклики: <b>{wakeReplies.done} из {wakeReplies.total}</b> ({wakeReplies.engine_name} · {wakeReplies.voice})
          {:else}
            В кеше: <b>{wakeReplies.ready} из {wakeReplies.total}</b> фраз ({wakeReplies.engine_name} · {wakeReplies.voice})
          {/if}
        </span>
      </div>
      {#if wakeReplies.error}
        <p class="err" role="alert">{wakeReplies.error}</p>
      {/if}
      {#if !wakeReplies.can_generate && wakeReplies.missing}
        <p class="note">{wakeReplies.missing}</p>
      {:else if wakeReplies.running}
        <div class="row">
          <button type="button" class="btn sm ghost" disabled={wakeBusy} onclick={cancelWakeGenerate}>Остановить</button>
        </div>
      {:else if showWakeConfirm && missingCount > 0}
        <div class="wr-warn">
          <p>
            Будет сгенерировано <b>{missingCount}</b> коротких фраз голосом «{wakeReplies.voice}» через <b>{wakeReplies.engine_name}</b>.
            Это потратит запросы и может затронуть лимит (особенно на бесплатном Gemini или подписке ChatGPT).
            При достижении лимита генерация сразу остановится, а готовые фразы сохранятся и переживут обновления.
          </p>
          <div class="row">
            <button type="button" class="btn sm primary" disabled={wakeBusy} onclick={startWakeGenerate}>Сгенерировать сейчас</button>
            <button type="button" class="btn sm ghost" disabled={wakeBusy} onclick={() => { showWakeConfirm = false; }}>Не сейчас</button>
          </div>
        </div>
      {:else}
        <div class="row">
          {#if missingCount > 0}
            <button type="button" class="btn sm" disabled={wakeBusy} onclick={() => { showWakeConfirm = true; }}>
              {wakeReplies.ready > 0 ? `Догенерировать (${missingCount} фраз)` : `Сгенерировать отклики (${missingCount} фраз)`}
            </button>
          {/if}
          {#if wakeReplies.ready > 0}
            <button type="button" class="btn sm ghost" disabled={wakeBusy} onclick={clearWakeCache}>Очистить кеш</button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
  {@render toggle("Звук «услышал / думаю»", "thinking_sound", settings.thinking_sound !== false,
    (value) => saveToggle("thinking_sound", value, "Звук отклика включён", "Звук отклика выключен"),
    "Короткий сигнал сразу после окончания вашей фразы в классическом пайплайне.")}
  {#if settings.thinking_sound !== false}
    <div class="f">
      <span class="lbl">Звуковая тема отклика</span>
      <div class="seg" role="group" aria-label="Звуковая тема отклика">
        {#each soundThemes as [id, label] (id)}
          {@const currentTheme = settings.sound_theme || "scifi"}
          <button type="button" class:on={currentTheme === id} aria-pressed={currentTheme === id}
            disabled={pending.has("sound_theme")}
            onclick={() => {
              void save("sound_theme", () => ({ sound_theme: id }), `Звук: ${label}`);
              void api("POST", "/api/sound-preview", { theme: id }, { signal }).catch(() => {});
            }}>{label}</button>
        {/each}
      </div>
    </div>
  {/if}
  {@render toggle("Авто-режим «Шёпот»", "whisper_mode", settings.whisper_mode !== false,
    (value) => saveToggle("whisper_mode", value, "Авто-режим «Шёпот» включён", "Авто-режим «Шёпот» выключен"),
    "Если вы говорите тихо или шёпотом, Джарвис сам приглушает ответ и отвечает коротко и спокойно.")}
  {@render toggle("Живые эмоции и SFX", "expressive_mode", settings.expressive_mode !== false,
    (value) => saveToggle("expressive_mode", value, "Эмоции и эффекты включены", "Эмоции и эффекты выключены"),
    "Интонации персонажей и короткие звуковые эффекты (звонок, фанфары, гром) в живой речи.")}
  {@render toggle("Озвучивать ошибки Алисой", "voice_error_alice", settings.voice_error_alice !== false,
    (value) => saveToggle("voice_error_alice", value, "Алиса сообщит, если голосовой ИИ не ответит", "Голосовые уведомления об ошибках выключены"),
    "Если голосовой движок не смог подключиться или упёрся в лимит после всех автопереключений, штатная Алиса коротко назовёт причину. Ночью и при команде «Стоп» не срабатывает.")}
  {#if !settings.wake_word}
    {@render toggle("Проверять имя перед ответом", "wake_check", !!settings.wake_check,
      (value) => saveToggle("wake_check", value, "Сначала проверяю имя, потом отвечаю", "Отклик сразу, как у Алисы"),
      "Выключено — как у Алисы: услышал слово, сразу загорается кольцо и Джарвис слушает. Включено — сначала облако проверяет, что прозвучало именно «Джарвис»: ложных разговоров меньше, но отклик на полсекунды-секунду позже.")}
  {/if}
  {#each ranges as range (range.key)}
    {@const value = rangesInEdit[range.key] ?? settings[range.key] ?? range.min}
    <div class="f">
      <span class="lbl">
        <span id={"label-" + range.key}>{range.label}</span>
        {@html tip(range.hint)}
        <b class="val" aria-hidden="true">{range.format(value)}</b>
      </span>
      <div use:mountRange={range} role="slider" aria-labelledby={"label-" + range.key}
        aria-valuemin={range.min} aria-valuemax={range.max} aria-valuenow={value}
        aria-disabled={pending.has(range.key)}></div>
      <div class={styles.scale}><span>{range.low}</span><span>{range.high}</span></div>
    </div>
  {/each}
  <div class={styles.vu} aria-live="off">
    <div class={styles.vuHead}>
      <span>Уровень микрофона</span>
      <b class="val">{micPeak} / порог {vadThreshold}</b>
    </div>
    <div class={styles.vuTrack}>
      <div class={styles.vuFill} class:hot={micPeak > vadThreshold} style:width={`${Math.min(100, (micPeak / 3000) * 100)}%`}></div>
      <div class={styles.vuMark} style:left={`${Math.min(100, (vadThreshold / 3000) * 100)}%`}></div>
    </div>
  </div>
  {#if !state.speaker.wake_model}
    <p class="note">Модель слова ещё не установлена — пока Джарвиса можно позвать только кнопкой «Поговорить» на главной.</p>
  {/if}
{/snippet}
{@render settingSection("Микрофон и активация", "Слово активации, усиление и порог тишины", wakeFields, "mic")}

{#snippet languageFields()}
  <div class="seg" role="group" aria-label="Язык общения">
    {#each languages as [id, label] (id)}
      <button type="button" class:on={language === id} aria-pressed={language === id}
        disabled={pending.has("language")} onclick={() => saveLanguage(id, label)}>{label}</button>
    {/each}
  </div>
{/snippet}
{@render settingSection("Язык",
  "Язык распознавания и ответов",
  languageFields, "globe")}

<NightMode {night} now={settings.night_now} leds={state.speaker.leds} {pending} save={saveNight} />

{#snippet lightingFields()}
  <Lighting />
{/snippet}
{@render settingSection("Подсветка",
  "Световое кольцо колонки",
  lightingFields, "sun")}

{#snippet notifyFields()}
  <div class="f">
    <label for="s-notify-token">Токен для <code>POST /api/notify</code> {@html tip("Позволяет скриптам, iOS Командам, qBittorrent или Uptime Kuma озвучивать уведомления на колонке (ночью пропускается, если не передать force: true).")}</label>
    <div class="row">
      <input id="s-notify-token" type="text" readonly value={settings.notify_token || "не задан"} class="mono">
      <button type="button" class="btn" disabled={pending.has("notify_token")} onclick={generateNotifyToken}>{@html icon("refresh")}Создать</button>
      <button type="button" class="btn primary" disabled={!settings.notify_token} onclick={copyNotifyCurl}>{@html icon("copy")}Копировать curl</button>
    </div>
  </div>
{/snippet}
{@render settingSection("HTTP-уведомления",
  "Вебхук для озвучки событий из домашней сети",
  notifyFields, "broadcast")}

{#snippet pcFields()}
  <div class="grid g2">
    <div class="f">
      <label for="s-pc-url">URL агента на ПК</label>
      <input id="s-pc-url" type="text" maxlength="160" placeholder="http://192.168.1.50:8770" bind:value={pcUrlInput}>
    </div>
    <div class="f">
      <label for="s-pc-tok">Токен доступа</label>
      <input id="s-pc-tok" type="password" maxlength="256" placeholder={settings.pc_agent_token_hint || "Токен из jarvis_pc.py"} bind:value={pcTokenInput}>
    </div>
  </div>
  <div class="row">
    <button type="button" class="btn primary" disabled={pending.has("pc_agent")} onclick={savePcSettings}>{@html icon("save")}Сохранить</button>
    <button type="button" class="btn" disabled={!settings.pc_agent_url} onclick={pingPc}>{@html icon("refresh")}Проверить</button>
    <button type="button" class="btn" disabled={!settings.pc_agent_url} onclick={testPcScreen}>{@html icon("eye")}Экран</button>
  </div>
  {#if pcStatus}<p class="note">{pcStatus}</p>{/if}
  {#if pcScreenText}<p class="note">{pcScreenText}</p>{/if}
{/snippet}
{@render settingSection("ПК-компаньон (Экран)",
  "Зрение на экран и голосовые макросы через jarvis_pc.py",
  pcFields, "monitor")}

{#snippet presenceFields()}
  <div class="grid g2">
    <div class="f">
      <label for="s-pres-ip">IP телефона в Wi-Fi</label>
      <input id="s-pres-ip" type="text" maxlength="40" placeholder="192.168.1.50" bind:value={phoneIpInput}>
    </div>
    <div class="f">
      <label for="s-pres-min">Порог ухода (мин)</label>
      <input id="s-pres-min" type="number" min="5" max="240" bind:value={awayMinInput}>
    </div>
  </div>
  <div class="f">
    <label for="s-pres-greet">Своя фраза приветствия (пусто — сгенерирует ИИ)</label>
    <input id="s-pres-greet" type="text" maxlength="200" placeholder="С возвращением домой!" bind:value={greetingInput}>
  </div>
  <div class="row">
    <button type="button" class="btn primary" disabled={pending.has("presence")} onclick={savePresence}>{@html icon("save")}Сохранить</button>
    <button type="button" class="btn" onclick={testPresence}>{@html icon("play", "solid")}Тест</button>
    {#if presenceInfo.configured}
      <span class="pill-stat" class:on={presenceInfo.state === "home"}>
        {presenceInfo.state === "home" ? "Дома" : presenceInfo.state === "away" ? "Не дома" : "Проверка"}
      </span>
    {/if}
  </div>
{/snippet}
{@render settingSection("Живое присутствие",
  "Автоприветствие при возвращении домой без датчиков",
  presenceFields, "radar")}

{#snippet cybernotesFields()}
  {@render toggle("Голосовые заметки CyberNotes", "cybernotes_enabled", settings.cybernotes_enabled !== false,
    (value) => saveToggle("cybernotes_enabled", value, "Интеграция с CyberNotes включена", "Интеграция с CyberNotes выключена"),
    "Команды «запиши идею...» и поиск по вашей базе знаний.")}
  <p class="note">
    {notesInfo.available ? `Подключено · заметок в базе: ${notesInfo.total_notes}` : "Каталог CyberNotes недоступен через jarvis-voice"}
  </p>
{/snippet}
{@render settingSection("CyberNotes",
  "База знаний и заметки голосом",
  cybernotesFields, "note")}
