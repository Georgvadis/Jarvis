<script>
  import { onMount, untrack } from "svelte";
  import { goto } from "$app/navigation";
  import AssistantsNav from "#components/Shell/AssistantsNav.svelte";
  import { api, icon, setDirty, toast } from "#helpers/core.js";
  import { avatar, refreshRoute, reload, S } from "#state/panelState.js";
  import { profilePath } from "#helpers/paths.js";
  import { createProfile, profilePayload } from "./profileDraft.js";
  import { availableEngines } from "./ProfileBrain/options.js";
  import ProfileBasics from "./ProfileBasics/ProfileBasics.svelte";
  import ProfileBrain from "./ProfileBrain/ProfileBrain.svelte";
  import ProfileLighting from "./ProfileLighting/ProfileLighting.svelte";
  import ProfileVoice from "./ProfileVoice/ProfileVoice.svelte";

  let { base, isNew } = $props();
  const initial = createProfile(untrack(() => base), S);
  const active = initial.id === S.active;
  const engines = availableEngines();
  const controller = new AbortController();
  const { signal } = controller;
  let profile = $state({ ...initial, led: { ...initial.led } });
  let savedName = $state(initial.name);
  let promptText = $state(untrack(() => (S.prompts || []).find((p) => p.id === initial.prompt)?.text || ""));
  let promptEdited = $state(false);
  let dirty = $state(false);
  let working = $state("");
  let confirmDelete = $state(false);
  let ptab = $state("brain");
  let revision = 0;
  let basics = $state();
  let brain = $state();
  let voice = $state();
  let ttsProvider = $derived(profile.engine === "classic" ? profile.tts_engine : profile.engine === "eleven_realtime" ? "elevenlabs" : profile.engine);
  let engineReady = $derived(engines.some(({ id }) => id === profile.engine));

  onMount(() => () => controller.abort());

  $effect(() => {
    const section = voice;
    const engine = profile.engine;
    const provider = ttsProvider;
    const ready = engineReady;
    if (section) untrack(() => section.configure(engine, provider, ready, engines.length > 0));
  });

  function mark() {
    revision++;
    dirty = true;
    setDirty(true);
  }

  function clearDirty() {
    dirty = false;
    setDirty(false);
  }

  function edit(changes, edited = true) {
    Object.assign(profile, changes);
    if (edited) mark();
  }

  function editLighting(changes) {
    Object.assign(profile.led, changes);
    mark();
  }

  function editPromptText(nextText) {
    promptText = nextText;
    promptEdited = true;
    mark();
  }

  function previewModels(engine) {
    const models = brain.getValues();
    if (engine === "classic") return { mode: "classic", tts_engine: profile.tts_engine, tts_model: models.tts };
    return { tts_model: "", live_model: models.live };
  }

  async function run(action, task) {
    if (working) return;
    working = action;
    try {
      await task();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      working = "";
    }
  }

  function save() {
    return run("save", async () => {
      const trimmedName = profile.name.trim();
      const trimmedPrompt = promptText.trim();
      if (!trimmedName) throw new Error("Укажите имя ассистента");
      if (trimmedPrompt && (promptEdited || isNew)) {
        const sharedByOther = Boolean(
          profile.prompt && (S.profiles || []).some((item) => item.id !== profile.id && item.prompt === profile.prompt),
        );
        const promptExists = Boolean(profile.prompt && (S.prompts || []).some((item) => item.id === profile.prompt));
        const targetPromptId = !isNew && promptExists && !sharedByOther ? profile.prompt : "";
        const res = await api("PUT", "/api/prompt", {
          id: targetPromptId,
          name: trimmedName,
          text: trimmedPrompt,
        }, { signal });
        if (res?.id) profile.prompt = res.id;
      }
      const payload = profilePayload($state.snapshot(profile), brain.getValues());
      const savedRevision = revision;
      const { id } = await api("PUT", "/api/profile", payload, { signal });
      await reload();
      if (signal.aborted) return;
      promptEdited = false;
      toast("Сохранено");
      if (isNew) {
        clearDirty();
        await goto(profilePath(id));
        return;
      }
      const stored = S.profiles.find((item) => item.id === id);
      const unchanged = revision === savedRevision;
      if (stored) {
        savedName = stored.name;
        basics?.updateSavedName(stored.name, unchanged);
      }
      if (unchanged) {
        if (stored) {
          profile.name = stored.name;
          brain.updateIdle(stored.idle_s);
        }
        clearDirty();
      }
    });
  }

  function activate() {
    return run("activate", async () => {
      await api("POST", "/api/active", { id: profile.id }, { signal });
      await reload();
      if (signal.aborted) return;
      toast(`Теперь отвечает «${profile.name}»`);
      refreshRoute();
    });
  }

  function remove() {
    confirmDelete = false;
    return run("delete", async () => {
      await api("DELETE", "/api/profile?id=" + encodeURIComponent(profile.id), undefined, { signal });
      await reload();
      if (signal.aborted) return;
      clearDirty();
      toast("Удалено");
      await goto("/", { replaceState: true });
    });
  }
</script>

<AssistantsNav current={isNew ? "p:new" : "p:" + initial.id} />

<header class="profile-head">
  <div class="profile-head-who">
    <span id="pav" class="ringbox">{@html avatar(profile, true)}</span>
    <div>
      <h1>{isNew ? "Новый ассистент" : savedName}</h1>
      <p class="sub">{isNew ? "Профиль ассистента" : active ? "Активный" : "Не активен"}</p>
    </div>
  </div>
  <div class="section-head-right">
    {#if !isNew && !active}
      <button class="btn sm" type="button" id="p-activate" disabled={Boolean(working)} onclick={activate}
        data-tip="Этот ассистент будет отвечать на «Джарвис»">{@html icon("check")}Сделать активным</button>
      {#if confirmDelete}
        <div class="inline-confirm" role="alert">
          <span>Удалить «{profile.name}»?</span>
          <button class="btn danger sm" type="button" id="p-del-confirm" disabled={Boolean(working)} onclick={remove}>
            {@html icon("check")}Удалить
          </button>
          <button class="btn ghost sm" type="button" disabled={Boolean(working)} onclick={() => { confirmDelete = false; }}>
            {@html icon("x")}Отмена
          </button>
        </div>
      {:else}
        <button class="btn sm ghost danger" type="button" id="p-del" disabled={Boolean(working)} onclick={() => { confirmDelete = true; }}>{@html icon("trash")}Удалить</button>
      {/if}
    {/if}
    <div class="seg asst-sub-seg" role="tablist" aria-label="Разделы профиля">
      <button type="button" role="tab" aria-selected={ptab === "brain"} class:on={ptab === "brain"} onclick={() => { ptab = "brain"; }}>
        {@html icon("cpu")} Основное и мозг
      </button>
      <button type="button" role="tab" aria-selected={ptab === "voice"} class:on={ptab === "voice"} onclick={() => { ptab = "voice"; }}>
        {@html icon("voice")} Голос и подсветка
      </button>
    </div>
  </div>
</header>

<div class="profile-bento" class:hidden={ptab !== "brain"}>
  <ProfileBasics bind:this={basics} initial={{
    name: initial.name,
    tagline: initial.tagline,
    avatar: initial.avatar,
    color: initial.color,
    prompt: initial.prompt,
  }} {promptText} onedit={edit} onprompt={editPromptText} />
  <ProfileBrain bind:this={brain} {initial} {profile} {engines} {ttsProvider} {engineReady}
    onedit={edit} onchange={mark} onrefreshvoices={() => voice?.refreshEleven()} />
</div>
<div class="profile-bento-voice" class:hidden={ptab !== "voice"}>
  <ProfileVoice bind:this={voice} initial={{
    voice: initial.voice, ttsVoice: initial.tts_voice,
    ttsProvider: initial.engine === "eleven_realtime" ? "elevenlabs" : initial.tts_engine,
    volume: initial.volume,
    ttsSpeed: initial.tts_speed || 1.0,
  }} onedit={edit} {previewModels} />
  <ProfileLighting initial={initial.led} previewAvailable={Boolean(S.speaker.leds)}
    globallyEnabled={S.settings.led?.enabled !== false} onedit={editLighting} />
</div>

{#if dirty || isNew}
  <div class="savebar savebar-pill" role="region" aria-label="Сохранение изменений">
    {#if dirty && !isNew}
      <span class="dirty" id="dirty">Есть несохранённые изменения</span>
    {/if}
    <button class="btn sm primary" type="button" id="p-save" disabled={Boolean(working)} onclick={save}>
      {@html icon(isNew ? "plus" : "save")}{isNew ? "Создать" : "Сохранить"}
    </button>
  </div>
{/if}
