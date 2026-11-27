<script>
  import { onMount, untrack } from "svelte";
  import { resizeAvatarFile } from "#helpers/avatar.js";
  import { icon, tip, toast } from "#helpers/core.js";
  import { loadWakeProfiles } from "#helpers/wake-training.js";
  import { S } from "#state/panelState.js";

  const colors = [1, 2, 3, 4, 5, 6];

  let { initial, promptText = "", onedit, onprompt } = $props();
  let draft = $state({ ...untrack(() => initial) });
  let text = $state(untrack(() => promptText));
  let dragOver = $state(false);
  let uploading = $state(false);
  let wakeModels = $state.raw([]);
  let wakeTrainingEnabled = $derived(Boolean(S.settings?.wake_training));

  let initialLetter = $derived((draft.name || "?").trim()[0] || "?");

  onMount(() => {
    if (!wakeTrainingEnabled) return;
    const ac = new AbortController();
    loadWakeProfiles(ac.signal)
      .then((res) => {
        if (!ac.signal.aborted) {
          wakeModels = (res.profiles || []).filter((m) => m.id);
        }
      })
      .catch(() => {});
    return () => ac.abort();
  });

  function edit(field, value) {
    if (draft[field] === value) return;
    draft[field] = value;
    onedit({ [field]: value });
  }

  function editPrompt(value) {
    text = value;
    onprompt?.(value);
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
</script>

<section class="card bento-card persona-editor-card" id="profile-basics">
  <div class="persona-editor-hero">
    <div
      class="persona-avatar-picker"
      class:over={dragOver}
      role="region"
      aria-label="Портрет ассистента"
      ondragover={(e) => { e.preventDefault(); dragOver = true; }}
      ondragleave={() => { dragOver = false; }}
      ondrop={onDrop}
    >
      <div class={"persona-avatar-preview lg c" + (draft.color || 1)}>
        {#if draft.avatar}
          <img src={draft.avatar} alt={draft.name || "Аватар"} />
        {:else}
          <span>{initialLetter}</span>
        {/if}
      </div>
      <div class="persona-avatar-controls">
        <div class="row">
          <label class="btn sm primary" class:disabled={uploading}>
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
        <small class="muted">Перетащите картинку или выберите файл · автообрезка 1:1</small>
      </div>
    </div>

    <div class="grid g2">
      <div class="f">
        <label for="p-name">Имя {@html tip("Отображается на карточке, в шапке и в чате. На слово активации не влияет.")}</label>
        <input id="p-name" type="text" maxlength="40" value={draft.name} placeholder="Например, Аянами"
          oninput={(event) => edit("name", event.currentTarget.value)}>
      </div>

      <div class="f">
        <label for="p-tagline">Короткое описание</label>
        <input id="p-tagline" type="text" maxlength="120" value={draft.tagline || ""} placeholder="Например: Тихий и мудрый пилот"
          oninput={(event) => edit("tagline", event.currentTarget.value)}>
      </div>
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

    {#if wakeTrainingEnabled}
      <div class="f">
        <label for="p-wake">Слово активации персонажа {@html tip("Если выбрать обученное слово (например, «Рюк» или «Аянами»), колонка будет слушать его параллельно с другими и сразу откликаться этим персонажем.")}</label>
        <div class="row">
          <select id="p-wake" value={draft.wake_profile || ""} onchange={(event) => edit("wake_profile", event.currentTarget.value)}>
            <option value="">Общее слово колонки</option>
            {#each wakeModels as m (m.id)}
              <option value={m.id}>{m.phrase}</option>
            {/each}
          </select>
          <a class="btn sm ghost" href="/training">{@html icon("mic")} Обучить слово</a>
        </div>
      </div>
    {/if}
  </div>

  <div class="nav-sep"></div>

  <div class="f">
    <div class="persona-char-head">
      <label for="p-prompt-text">Характер и манера речи {@html tip("Как ассистент говорит, тон, обращение, привычки и роль. Передаётся модели в каждом разговоре.")}</label>
      <span class="meta">{text.length} симв.</span>
    </div>
    <textarea
      id="p-prompt-text"
      rows="8"
      maxlength="18000"
      placeholder="Ты — … Опишите манеру речи, характер, тон, длину ответов и особенности персонажа"
      value={text}
      oninput={(event) => editPrompt(event.currentTarget.value)}
    ></textarea>
  </div>
</section>
