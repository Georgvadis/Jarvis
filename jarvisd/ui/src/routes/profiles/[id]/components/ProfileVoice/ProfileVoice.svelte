<script module>
  const defaultVoices = { openai: "cedar", gemini: "Charon", chatgpt: "cove", grok: "rex" };
  const groupKeys = { gemini: "gemini_voice_groups", chatgpt: "chatgpt_voice_groups", grok: "grok_voice_groups" };
  const manualPreview = Symbol();

  function voiceGroups(state, engine) {
    return state[groupKeys[engine] || "voice_groups"] || [];
  }
</script>

<script>
  import { onMount, untrack } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { api, icon, jelly, tip, toast, toastProgress, upload } from "#helpers/core.js";
  import { reload, S } from "#state/panelState.js";
  import styles from "./ProfileVoice.module.css";

  let { initial, onedit, previewModels } = $props();
  const controller = new AbortController();
  const { signal } = controller;
  const initialProfile = untrack(() => initial);
  let draft = $state({ ...initialProfile });
  const ttsSelections = new Map([[initialProfile.ttsProvider, initialProfile.ttsVoice || ""]]);
  const pluginVoices = new SvelteMap();
  let engine = $state("");
  let ttsProvider = $state("");
  let ready = $state(false);
  let hasOptions = $state(false);
  let customVoices = $state.raw(S.live_voices);
  let elevenVoices = $state.raw(null);
  let elevenLoading = $state(false);
  let elevenError = $state("");
  let previewVoice = $state(null);
  let previewPhase = $state("");
  let previewRequest;
  let previewTimer;
  let previewProgressToast;
  const phaseLabels = {
    queued: "Ожидание предыдущей пробы…",
    checking_speaker: "Проверяю, свободна ли колонка…",
    checking_cache: "Проверяю сохранённый образец…",
    connecting: "Подключаю голосовой движок…",
    generating: "Генерирую образец — это может занять до 25 секунд…",
    playing: "Воспроизвожу на колонке…",
  };
  let name = $state("");
  let prompt = $state("");
  let savingVoice = $state(false);
  let qwenName = $state("");
  let qwenRefText = $state("");
  let qwenFile = $state(null);
  let qwenUploading = $state(false);
  let confirmDeleteId = $state("");
  let volSlider;
  let speedSlider;
  let lastVol = untrack(() => draft.volume);
  let lastSpeed = untrack(() => draft.ttsSpeed || 1.0);

  $effect(() => {
    const v = draft.volume;
    if (v !== lastVol) {
      lastVol = v;
      volSlider?.set(v, true);
    }
  });

  $effect(() => {
    const s = draft.ttsSpeed || 1.0;
    if (s !== lastSpeed) {
      lastSpeed = s;
      speedSlider?.set(Math.round(s * 100), true);
    }
  });

  let classic = $derived(engine === "classic");
  let eleven = $derived(ttsProvider === "elevenlabs");
  let pluginTts = $derived(classic && !eleven && ttsProvider !== "openai");
  let isLocalQwen = $derived(pluginTts && ttsProvider === "local-rvc-voice");
  let pluginVoicesState = $derived(pluginVoices.get(ttsProvider));
  let selected = $derived(classic || eleven ? draft.ttsVoice : draft.voice);
  let groups = $derived(voiceGroups(S, classic ? ttsProvider : engine).map((group) => ({
    name: group.name,
    items: group.voices.map((item) => ({ id: item.ID, name: item.name || item.ID, note: item.Note })),
  })));
  let customItems = $derived(customVoices.map((item) => ({ id: item.id, name: item.name, note: item.prompt, custom: true })));

  onMount(() => () => {
    controller.abort();
    previewRequest?.abort();
    clearInterval(previewTimer);
    previewProgressToast?.clear();
    previewPhase = "";
    if (previewRequest) void api("POST", "/api/preview/cancel", {}).catch(() => {});
  });

  export function configure(nextEngine, provider, available, optionsExist) {
    if (engine !== nextEngine || ttsProvider !== provider || !available) {
      previewRequest?.abort();
      if (previewRequest) void api("POST", "/api/preview/cancel", {}).catch(() => {});
      clearInterval(previewTimer);
      previewProgressToast?.clear();
      previewProgressToast = undefined;
      previewVoice = null;
      previewPhase = "";
    }
    engine = nextEngine;
    ttsProvider = provider;
    ready = available;
    hasOptions = optionsExist;
    if (!available) return;
    if (nextEngine === "classic" || nextEngine === "eleven_realtime") {
      choose(ttsSelections.get(provider) || "", false);
    }
    if (provider === "elevenlabs") {
      if (elevenVoices === null && !elevenError) void refreshEleven();
      return;
    }
    if (nextEngine === "classic" && provider !== "openai") {
      void loadPluginVoices(provider);
      return;
    }
    selectDefaultVoice(false);
  }

  function selectDefaultVoice(dirty) {
    if (engine === "openai" && customVoices.some((item) => item.id === draft.voice)) return;
    const current = String(selected || "").toLowerCase();
    const match = voiceGroups(S, classic ? ttsProvider : engine).flatMap((group) => group.voices)
      .find((item) => item.ID.toLowerCase() === current);
    choose(match?.ID || (classic ? "alloy" : defaultVoices[engine] || "cedar"), dirty);
  }

  export async function refreshEleven() {
    if (!ready || ttsProvider !== "elevenlabs" || elevenLoading) return;
    elevenLoading = true;
    elevenError = "";
    try {
      const data = await api("GET", "/api/elevenlabs/voices", undefined, { signal });
      elevenVoices = (data.voices || []).map((item) => ({
        id: item.voice_id,
        name: item.name,
        note: [item.category, item.labels?.language].filter(Boolean).join(" · "),
      }));
    } catch (error) {
      if (!signal.aborted) elevenError = error.message;
    } finally {
      elevenLoading = false;
    }
  }

  async function loadPluginVoices(provider) {
    if (pluginVoices.has(provider)) return;
    pluginVoices.set(provider, { loading: true, items: [] });
    try {
      const { voices } = await api("GET", "/api/tts/voices?provider=" + encodeURIComponent(provider), undefined, { signal });
      const items = new Map();
      for (const item of voices || []) {
        const id = item.id || item.voice_id || item.ID;
        if (id) items.set(id, { id, name: item.name || id, note: item.note || item.Note });
      }
      pluginVoices.set(provider, { items: [...items.values()] });
    } catch (error) {
      if (!signal.aborted) pluginVoices.set(provider, { error: error.message });
    }
  }

  function choose(id, dirty = true) {
    if (engine === "classic" || engine === "eleven_realtime") {
      ttsSelections.set(ttsProvider, id);
      if (draft.ttsVoice === id) return;
      draft.ttsVoice = id;
      onedit({ tts_voice: id }, dirty);
    } else {
      if (draft.voice === id) return;
      draft.voice = id;
      onedit({ voice: id }, dirty);
    }
  }

  async function preview(id, trigger = id, label = id) {
    previewRequest?.abort();
    previewProgressToast?.clear();
    clearInterval(previewTimer);
    const request = new AbortController();
    const caption = String(label || "").trim();
    const formatStatus = (text) => (caption ? `${caption}: ${text}` : text);
    const notice = toastProgress(formatStatus(phaseLabels.queued));
    previewRequest = request;
    previewProgressToast = notice;
    previewVoice = trigger;
    previewPhase = "queued";
    let lastPhase = "queued";
    try {
      let polling = false;
      const poll = async () => {
        if (polling || request.signal.aborted || previewRequest !== request) return;
        polling = true;
        try {
          const status = await api("GET", "/api/preview/status", undefined, { signal: request.signal });
          const voiceMatches = !id || String(status.voice || "").toLowerCase() === String(id).toLowerCase();
          if (previewRequest === request && voiceMatches && phaseLabels[status.phase]
              && status.phase !== lastPhase) {
            lastPhase = status.phase;
            previewPhase = status.phase;
            notice.update(formatStatus(phaseLabels[status.phase]));
          }
        } catch { /* The main preview request shows any actionable error. */ }
        finally { polling = false; }
      };
      previewTimer = setInterval(poll, 450);
      const response = await api("POST", "/api/preview", { voice: id, engine, ...previewModels(engine) }, { signal: request.signal });
      if (previewRequest === request && !response.interrupted) {
        notice.finish(response.cached ? "Образец из кеша прозвучал на колонке" : "Голос записан в кеш и прозвучал на колонке");
      }
    } catch (error) {
      if (!request.signal.aborted && previewRequest === request) notice.finish(error.message, true);
    } finally {
      if (previewRequest === request) {
        clearInterval(previewTimer);
        previewTimer = undefined;
        previewRequest = undefined;
        previewProgressToast = undefined;
        previewVoice = null;
        previewPhase = "";
        notice.clear();
      }
    }
  }

  async function syncCustomVoices() {
    try {
      await reload();
      if (!signal.aborted) customVoices = S.live_voices;
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    }
  }

  async function createVoice(event) {
    event.preventDefault();
    if (savingVoice) return;
    savingVoice = true;
    const previous = draft.voice;
    const values = { name: name.trim(), prompt: prompt.trim() };
    try {
      const { id } = await api("POST", "/api/voices", values, { signal });
      customVoices = [...customVoices, { id, ...values }];
      if (engine === "openai" && draft.voice === previous) choose(id);
      toast("Голос создан — послушайте его кнопкой рядом");
      await syncCustomVoices();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      savingVoice = false;
    }
  }

  async function deleteVoice(id) {
    if (savingVoice || !confirm("Удалить этот голос из списка?")) return;
    savingVoice = true;
    try {
      await api("DELETE", "/api/voices?id=" + encodeURIComponent(id), undefined, { signal });
      customVoices = customVoices.filter((item) => item.id !== id);
      if (engine === "openai" && draft.voice === id) selectDefaultVoice(true);
      await syncCustomVoices();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      savingVoice = false;
    }
  }

  function mountVolume(node) {
    volSlider = jelly(node, {
      label: "Громкость голоса", min: 10, max: 100, step: 5, format: (v) => `${v}%`, value: draft.volume,
      onInput: (volume) => {
        if (volume === draft.volume) return;
        lastVol = volume;
        draft.volume = volume;
        onedit({ volume });
      },
      onChange: (volume) => {
        if (volume === draft.volume) return;
        lastVol = volume;
        draft.volume = volume;
        onedit({ volume });
      },
    });
    return { destroy: () => { volSlider.destroy(); volSlider = undefined; } };
  }

  function mountSpeed(node) {
    const initialPct = Math.round((draft.ttsSpeed || 1.0) * 100);
    speedSlider = jelly(node, {
      label: "Скорость речи", min: 80, max: 130, step: 5, format: (v) => `${(v / 100).toFixed(2)}×`, value: initialPct,
      onInput: (pct) => {
        const s = Math.round(pct) / 100;
        if (s === draft.ttsSpeed) return;
        lastSpeed = s;
        draft.ttsSpeed = s;
        onedit({ tts_speed: s });
      },
      onChange: (pct) => {
        const s = Math.round(pct) / 100;
        if (s === draft.ttsSpeed) return;
        lastSpeed = s;
        draft.ttsSpeed = s;
        onedit({ tts_speed: s });
      },
    });
    return { destroy: () => { speedSlider.destroy(); speedSlider = undefined; } };
  }

  async function uploadQwenVoice(event) {
    event.preventDefault();
    if (qwenUploading) return;
    const vname = qwenName.trim();
    if (!vname) return toast("Введите название голоса", true);
    if (!qwenFile) return toast("Выберите аудиофайл (5–15 сек)", true);
    if (qwenFile.size > 15 * 1024 * 1024) return toast("Файл больше 15 МБ", true);
    qwenUploading = true;
    try {
      const qs = new URLSearchParams({
        name: vname,
        filename: qwenFile.name || "sample.wav",
        ref_text: qwenRefText.trim(),
        language: "Russian",
      }).toString();
      const res = await upload("POST", "/api/local-voices/upload-raw?" + qs, qwenFile, { signal });
      if (signal.aborted) return;
      qwenName = "";
      qwenRefText = "";
      qwenFile = null;
      const input = event.currentTarget.querySelector('input[type="file"]');
      if (input) input.value = "";
      toast(res.ref_text ? `Голос «${res.name}» добавлен (текст распознан)` : `Голос «${res.name}» добавлен`);
      pluginVoices.delete(ttsProvider);
      await loadPluginVoices(ttsProvider);
      if (res.id) choose(res.id);
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      qwenUploading = false;
    }
  }

  async function deleteQwenVoice(id) {
    if (qwenUploading) return;
    qwenUploading = true;
    confirmDeleteId = "";
    try {
      await api("DELETE", "/api/local-voices/" + encodeURIComponent(id), undefined, { signal });
      if (signal.aborted) return;
      toast("Голос удалён");
      pluginVoices.delete(ttsProvider);
      await loadPluginVoices(ttsProvider);
      if (draft.ttsVoice === id) choose("");
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      qwenUploading = false;
    }
  }
</script>

{#snippet options(items)}
  <div class="voices">
    {#each items as item (item.id)}
      <div class="voice {styles.voice}" class:on={selected === item.id} data-v={item.id}>
        <button class="vn {styles.choice}" type="button" aria-label={"Выбрать голос " + item.name}
          aria-pressed={selected === item.id} onclick={() => choose(item.id)}>
          <b>{item.name}</b><small>{item.note || ""}</small>
        </button>
        <button class="ib {styles.action}" type="button" data-play={item.id}
          title={previewVoice === item.id ? (previewPhase === "playing" ? "Говорит на колонке" : "Готовлю образец на колонке…") : "Послушать на колонке"}
          aria-label={previewVoice === item.id ? (previewPhase === "playing" ? "Воспроизводится голос " : "Готовится голос ") + item.name : "Послушать голос " + item.name} aria-busy={previewVoice === item.id}
          disabled={previewVoice === item.id} onclick={() => preview(item.id, item.id, item.name)}>
          {#if previewVoice === item.id && previewPhase === "playing"}
            <span class={styles.equalizer} aria-hidden="true"><i></i><i></i><i></i></span>
          {:else if previewVoice === item.id}
            <span class={styles.spinner} aria-hidden="true"></span>
          {:else}
            {@html icon("play", "solid")}
          {/if}
        </button>
        {#if item.custom}
          <button class="ib {styles.action}" type="button" data-delv={item.id} title="Удалить голос"
            aria-label={"Удалить голос " + item.name} disabled={savingVoice} onclick={() => deleteVoice(item.id)}>
            {@html icon("x")}
          </button>
        {:else if isLocalQwen}
          {#if confirmDeleteId === item.id}
            <button class="ib danger {styles.action}" type="button" title="Подтвердить удаление"
              aria-label={"Подтвердить удаление " + item.name} disabled={qwenUploading} onclick={() => deleteQwenVoice(item.id)}>
              {@html icon("check")}
            </button>
            <button class="ib {styles.action}" type="button" title="Отмена"
              aria-label="Отменить удаление" disabled={qwenUploading} onclick={() => { confirmDeleteId = ""; }}>
              {@html icon("x")}
            </button>
          {:else}
            <button class="ib danger {styles.action}" type="button" title="Удалить голос"
              aria-label={"Удалить голос " + item.name} disabled={qwenUploading} onclick={() => { confirmDeleteId = item.id; }}>
              {@html icon("trash")}
            </button>
          {/if}
        {/if}
      </div>
    {/each}
  </div>
{/snippet}

<section class="sec" id="profile-voice">
  <div class="sec-h">
    <h2>{@html icon("voice")}Голос</h2>
    <p>Выбор голоса, прослушивание на колонке и громкость</p>
  </div>
  <div class="sec-b">
    <div id="voices" class="vgroups">
      {#if !ready}
        <p class="note">{hasOptions ? "Выберите движок в разделе «Мозг»." : "Голос появится после настройки плагина."}</p>
      {:else if eleven}
        {#if elevenLoading}
          <p class="note" role="status">Загружаю голоса вашего аккаунта ElevenLabs…</p>
        {:else if elevenError}
          <p class="err" role="status">{elevenError}</p>
        {:else if elevenVoices?.length}
          {@render options(elevenVoices)}
        {:else}
          <p class="note">В аккаунте нет доступных голосов.</p>
        {/if}
      {:else if pluginTts}
        {#if !pluginVoicesState || pluginVoicesState.loading}
          <p class="note" role="status">Загружаю голоса плагина…</p>
        {:else if pluginVoicesState.error}
          <p class="err" role="status">{pluginVoicesState.error}</p>
        {:else if pluginVoicesState.items.length}
          {@render options(pluginVoicesState.items)}
        {:else}
          <p class="note">Плагин не передал список голосов. Введите ID голоса ниже или оставьте пустым.</p>
        {/if}
      {:else}
        {#if engine === "openai" && customItems.length}
          <div class="vgroup"><div class="gh">Мои голоса</div>{@render options(customItems)}</div>
        {/if}
        {#each groups as group}
          <div class="vgroup"><div class="gh">{group.name}</div>{@render options(group.items)}</div>
        {/each}
      {/if}
    </div>
    {#if ready && pluginTts}
      <div id="p-tts-voice-box" class="row">
        <div class="f half">
          <label for="p-tts-voice-custom">ID голоса (или оставьте пустым по умолчанию)</label>
          <input id="p-tts-voice-custom" type="text" maxlength="128" value={draft.ttsVoice}
            placeholder="Например, голос из плагина" autocomplete="off"
            oninput={(event) => choose(event.currentTarget.value.trim())}>
        </div>
        <button class="btn" id="p-tts-voice-test" type="button" aria-busy={previewVoice === manualPreview}
          disabled={previewVoice === manualPreview}
          onclick={() => preview(draft.ttsVoice, manualPreview, draft.ttsVoice)}>
          {#if previewVoice === manualPreview && previewPhase === "playing"}
            <span class={styles.equalizer} aria-hidden="true"><i></i><i></i><i></i></span>
          {:else if previewVoice === manualPreview}
            <span class={styles.spinner} aria-hidden="true"></span>
          {/if}
          Послушать
        </button>
      </div>
    {/if}
    {#if isLocalQwen}
      <details class="qwen-upload">
        <summary>{@html icon("upload")} Клонировать свой голос (Qwen3-TTS)</summary>
        <form class="inner" onsubmit={uploadQwenVoice}>
          <div class="grid g2">
            <div class="f">
              <label for="qv-name">Название голоса</label>
              <input id="qv-name" type="text" maxlength="60" placeholder="Например, Тони" bind:value={qwenName} disabled={qwenUploading}>
            </div>
            <div class="f">
              <label for="qv-file">Аудио-образец (5–15 с)</label>
              <input id="qv-file" type="file" accept="audio/*,.wav,.mp3,.ogg,.m4a" disabled={qwenUploading}
                onchange={(e) => { qwenFile = e.currentTarget.files?.[0] || null; }}>
            </div>
          </div>
          <div class="f">
            <label for="qv-text">Текст образца {@html tip("Оставьте пустым — сервер автоматически распознает фразу через Whisper.")}</label>
            <input id="qv-text" type="text" maxlength="400" placeholder="Необязательно — распознается автоматически" bind:value={qwenRefText} disabled={qwenUploading}>
          </div>
          <div>
            <button class="btn primary" type="submit" disabled={qwenUploading}>
              {@html icon("upload")}{qwenUploading ? "Загружаю и обрабатываю…" : "Загрузить голос"}
            </button>
          </div>
        </form>
      </details>
    {/if}
    <details id="v-custom" class:hidden={!ready || engine !== "openai"}>
      <summary>Создать свой голос по описанию</summary>
      <form class="inner" onsubmit={createVoice}>
        <div class="f">
          <label for="v-name">Название</label>
          <input type="text" id="v-name" placeholder="Например, Бархатный" bind:value={name}>
        </div>
        <div class="f">
          <label for="v-prompt">Описание {@html tip("Тембр, возраст, манера, темп, эмоции. Подражать конкретным людям и персонажам OpenAI не даёт, но описать манеру можно. Создание бесплатное.")}</label>
          <textarea id="v-prompt" placeholder="Низкий спокойный мужской голос, лет сорока, говорит неторопливо, с лёгкой иронией…" bind:value={prompt}></textarea>
        </div>
        <div><button class="btn" id="v-create" type="submit" disabled={savingVoice}>Создать голос</button></div>
      </form>
    </details>
    <div class="f">
      <span class="lbl">Громкость голоса {@html tip("Насколько голос Джарвиса громче или тише остального звука колонки. Голосом: «Джарвис, громче/тише».")}<b id="p-vol-v" class="val">{draft.volume}%</b></span>
      <div id="p-vol" use:mountVolume role="slider" aria-label="Громкость голоса" aria-valuemin="10" aria-valuemax="100" aria-valuenow={draft.volume}></div>
    </div>
    {#if classic}
      <div class="f">
        <span class="lbl">Скорость речи {@html tip("Темп озвучки в классическом пайплайне (работает с любым TTS без искажения тона голоса).")}<b class="val">{(draft.ttsSpeed || 1.0).toFixed(2)}×</b></span>
        <div id="p-speed" use:mountSpeed role="slider" aria-label="Скорость речи" aria-valuemin="80" aria-valuemax="130" aria-valuenow={Math.round((draft.ttsSpeed || 1.0) * 100)}></div>
      </div>
    {/if}
  </div>
</section>
