<script>
  import { untrack } from "svelte";
  import { resizeAvatarFile } from "#helpers/avatar.js";
  import { api, icon, tip, toast } from "#helpers/core.js";
  import { reload, S } from "#state/panelState.js";

  const colors = [1, 2, 3, 4, 5, 6];
  let { initial, prompts, onedit } = $props();
  let draft = $state({ ...untrack(() => initial) });
  let promptList = $state.raw(untrack(() => prompts || []));
  let dragOver = $state(false);
  let uploading = $state(false);
  let editingPrompt = $state(false);
  let promptDraftText = $state("");
  let savingPrompt = $state(false);

  let selectedPrompt = $derived(promptList.find((p) => p.id === draft.prompt) || null);
  let initialLetter = $derived((draft.name || "?").trim()[0] || "?");

  function edit(field, value) {
    if (draft[field] === value) return;
    draft[field] = value;
    onedit({ [field]: value });
  }

  export function updateSavedName(name, updateDraft) {
    if (updateDraft) draft.name = name;
  }

  async function pickFile(file) {
    if (!file || uploading) return;
    uploading = true;
    try {
      const dataUrl = await resizeAvatarFile(file);
      edit("avatar", dataUrl);
      toast("Портрет загружен — нажмите «Сохранить»");
    } catch (error) {
      toast(error.message, true);
    } finally {
      uploading = false;
    }
  }

  function onFileChange(event) {
    const input = event.currentTarget;
    const file = input.files?.[0];
    input.value = "";
    if (file) void pickFile(file);
  }

  function onDrop(event) {
    event.preventDefault();
    dragOver = false;
    const file = event.dataTransfer?.files?.[0];
    if (file) void pickFile(file);
  }

  function startEditPrompt() {
    if (!selectedPrompt) return;
    promptDraftText = selectedPrompt.text || "";
    editingPrompt = true;
  }

  async function savePromptText() {
    if (!selectedPrompt || savingPrompt) return;
    const text = promptDraftText.trim();
    if (!text) return toast("Введите описание характера", true);
    savingPrompt = true;
    try {
      await api("PUT", "/api/prompt", { id: selectedPrompt.id, name: selectedPrompt.name, text });
      await reload();
      promptList = S.prompts || promptList;
      editingPrompt = false;
      toast("Характер обновлён");
    } catch (error) {
      toast(error.message, true);
    } finally {
      savingPrompt = false;
    }
  }
</script>

<section class="card bento-card" id="profile-basics">
  <h2>{@html icon("sparkles")} Личность и образ</h2>
  <div class="studio-stack">
    <div
      class="persona-avatar-picker"
      class:over={dragOver}
      role="region"
      aria-label="Портрет ассистента"
      ondragover={(e) => { e.preventDefault(); dragOver = true; }}
      ondragleave={() => { dragOver = false; }}
      ondrop={onDrop}
    >
      <div class={"persona-avatar-preview c" + (draft.color || 1)}>
        {#if draft.avatar}
          <img src={draft.avatar} alt={draft.name || "Аватар"} />
        {:else}
          <span>{initialLetter}</span>
        {/if}
      </div>
      <div class="persona-avatar-controls">
        <div class="row">
          <label class="btn sm" class:disabled={uploading}>
            {@html icon("camera")}
            <span>{uploading ? "Обработка…" : draft.avatar ? "Сменить фото" : "Загрузить фото"}</span>
            <input type="file" accept="image/*" hidden disabled={uploading} onchange={onFileChange}>
          </label>
          {#if draft.avatar}
            <button type="button" class="btn sm ghost danger" disabled={uploading} onclick={() => edit("avatar", "")}>
              {@html icon("trash")} Убрать
            </button>
          {/if}
        </div>
        <small class="muted">JPG, PNG или WebP · автообрезка 1:1</small>
      </div>
    </div>

    <div class="f">
      <label for="p-name">Имя {@html tip("Отображается на карточке, в шапке и в чате. На слово активации не влияет.")}</label>
      <input id="p-name" type="text" maxlength="40" value={draft.name} placeholder="Например, Джарвис"
        oninput={(event) => edit("name", event.currentTarget.value)}>
    </div>

    <div class="f">
      <label for="p-tagline">Роль или короткое описание</label>
      <input id="p-tagline" type="text" maxlength="120" value={draft.tagline || ""} placeholder="Например: Саркастичный дворецкий"
        oninput={(event) => edit("tagline", event.currentTarget.value)}>
    </div>

    <div class="f">
      <span class="lbl">Оттенок карточки</span>
      <div class="colors" id="p-color" role="group" aria-label="Цвет ассистента">
        {#each colors as color (color)}
          <button class={"av c" + color} class:on={draft.color === color} data-c={color} type="button"
            aria-label={"Цвет " + color} aria-pressed={draft.color === color} onclick={() => edit("color", color)}></button>
        {/each}
      </div>
    </div>

    <div class="f">
      <label for="p-prompt">Характер {@html tip("Системный промпт: кто такой ассистент и как он общается.")}</label>
      <div class="row">
        <select id="p-prompt" value={draft.prompt} onchange={(event) => { editingPrompt = false; edit("prompt", event.currentTarget.value); }}>
          {#each promptList as prompt (prompt.id)}
            <option value={prompt.id}>{prompt.name}</option>
          {/each}
        </select>
        {#if selectedPrompt && !editingPrompt}
          <button type="button" class="ib" title="Изменить текст характера" aria-label="Изменить текст характера" onclick={startEditPrompt}>
            {@html icon("edit")}
          </button>
        {/if}
      </div>
    </div>

    {#if editingPrompt && selectedPrompt}
      <div class="persona-prompt-edit">
        <textarea rows="5" aria-label="Текст характера" bind:value={promptDraftText} disabled={savingPrompt}></textarea>
        <div class="row">
          <button type="button" class="btn sm primary" disabled={savingPrompt} onclick={() => void savePromptText()}>
            {@html icon("save")} Сохранить характер
          </button>
          <button type="button" class="btn sm ghost" disabled={savingPrompt} onclick={() => { editingPrompt = false; }}>
            {@html icon("x")} Отмена
          </button>
        </div>
      </div>
    {:else if selectedPrompt?.text}
      <div class="persona-prompt-preview">
        <p>{selectedPrompt.text}</p>
      </div>
    {/if}

    <div>
      <a class="btn sm ghost" href="/characters">{@html icon("smile")} Все характеры</a>
    </div>
  </div>
</section>
