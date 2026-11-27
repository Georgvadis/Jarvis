<script>
  import AssistantsNav from "#components/Shell/AssistantsNav.svelte";
  import { resizeAvatarFile } from "#helpers/avatar.js";
  import { api, icon, plural, toast } from "#helpers/core.js";
  import { profilePath } from "#helpers/paths.js";
  import { engineName, isClassicProfile, refreshRoute, reload, speaker, voiceName } from "#state/panelState.js";

  let busyId = $state("");
  let uploadId = $state("");

  let profiles = $derived($speaker?.profiles || []);
  let prompts = $derived($speaker?.prompts || []);
  let activeId = $derived($speaker?.active || "");

  function promptFor(p) {
    return prompts.find((item) => item.id === p.prompt) || null;
  }

  function voiceLabel(p) {
    if (isClassicProfile(p)) {
      const tts = p.tts_engine || (p.engine === "openai" ? "openai" : "elevenlabs");
      if (tts === "openai" && p.voice) return voiceName(p.voice);
      if (p.tts_voice) return p.tts_voice;
      return "";
    }
    return p.voice ? voiceName(p.voice) : "";
  }

  async function activate(p) {
    if (busyId || p.id === activeId) return;
    busyId = p.id;
    try {
      await api("POST", "/api/active", { id: p.id });
      await reload();
      toast(`Активен: ${p.name}`);
      refreshRoute();
    } catch (error) {
      toast(error.message, true);
    } finally {
      busyId = "";
    }
  }

  async function quickUploadAvatar(event, p) {
    const input = event.currentTarget;
    const file = input.files?.[0];
    input.value = "";
    if (!file || uploadId) return;
    uploadId = p.id;
    try {
      const dataUrl = await resizeAvatarFile(file);
      await api("PUT", "/api/profile", { ...p, avatar: dataUrl });
      await reload();
      toast(`Портрет «${p.name}» обновлён`);
    } catch (error) {
      toast(error.message, true);
    } finally {
      uploadId = "";
    }
  }
</script>

<AssistantsNav current="assistants" />

<header class="section-head">
  <div class="mem-head-left">
    <h1>Студия ассистентов</h1>
    <span class="chip">{profiles.length} {plural(profiles.length, "личность", "личности", "личностей")}</span>
  </div>
  <div class="section-head-right">
    <a class="btn primary" href={profilePath("new")}>{@html icon("plus")} Новый ассистент</a>
  </div>
</header>

<div class="persona-grid stagger">
  {#each profiles as p (p.id)}
    {@const isActive = p.id === activeId}
    {@const ch = promptFor(p)}
    {@const vLabel = voiceLabel(p)}
    {@const initial = (p.name || "?").trim()[0] || "?"}
    <article class="card persona-card" class:active={isActive} data-c={p.color || 1}>
      <div class={"persona-cover c" + (p.color || 1)}>
        <a class="persona-cover-link" href={profilePath(p.id)} aria-label={`Настроить «${p.name}»`}>
          {#if p.avatar}
            <img class="persona-cover-img" src={p.avatar} alt={p.name} />
          {:else}
            <span class="persona-monogram" aria-hidden="true">{initial}</span>
          {/if}
        </a>

        <div class="persona-cover-top">
          {#if isActive}
            <span class="persona-badge-live">
              <span class="sdot ok"></span>
              <span>Активен</span>
            </span>
          {:else}
            <span></span>
          {/if}

          <label class="persona-cam-btn" class:busy={uploadId === p.id} title="Загрузить портрет" aria-label={`Загрузить портрет для ${p.name}`}>
            {@html icon("camera")}
            <input type="file" accept="image/*" hidden disabled={Boolean(uploadId)} onchange={(e) => void quickUploadAvatar(e, p)}>
          </label>
        </div>
      </div>

      <div class="persona-body">
        <a class="persona-title-link" href={profilePath(p.id)}>
          <h2>{p.name}</h2>
          {#if p.tagline || ch?.name}
            <span class="persona-sub">{p.tagline || ch?.name}</span>
          {/if}
        </a>

        {#if ch?.text}
          <p class="persona-quote">{ch.text}</p>
        {/if}

        <div class="persona-tags">
          <span class="chip persona-chip">{@html icon("cpu")} {engineName(p)}</span>
          {#if vLabel}
            <span class="chip persona-chip">{@html icon("voice")} {vLabel}</span>
          {/if}
        </div>
      </div>

      <div class="persona-foot">
        {#if isActive}
          <span class="persona-active-lbl">{@html icon("check")} Отвечает сейчас</span>
        {:else}
          <button type="button" class="btn sm" disabled={busyId === p.id} onclick={() => void activate(p)}>
            {@html icon("check")} Активировать
          </button>
        {/if}
        <a class="btn sm ghost" href={profilePath(p.id)}>
          {@html icon("tune")} Настроить
        </a>
      </div>
    </article>
  {/each}

  <a class="persona-card persona-add" href={profilePath("new")}>
    <span class="persona-add-ic">{@html icon("plus")}</span>
    <b>Новая личность</b>
    <small>Портрет, характер, голос и подсветка</small>
  </a>
</div>
