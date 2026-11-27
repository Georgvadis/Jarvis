<script>
  import { untrack } from "svelte";
  import { resizeAvatarFile } from "#helpers/avatar.js";
  import { icon, tip, toast } from "#helpers/core.js";

  const colors = [1, 2, 3, 4, 5, 6];
  const presets = [
    ["Дворецкий", "Ты — Джарвис, голосовой помощник в умной колонке. Говоришь по-русски, коротко и по делу, с лёгкой иронией, как дворецкий из фильма. Обращаешься к хозяину на «ты»."],
    ["Кореш", "Ты — близкий друг: общаешься на «ты», с юмором и подколами, живо и по-простому. Говоришь коротко и честно: не знаешь — так и скажи."],
    ["Учёный", "Ты — спокойный и точный учёный. Объясняешь просто, но без ошибок, приводишь одну яркую деталь или цифру. Отвечаешь в 2–3 предложения, если не просят подробнее."],
    ["Лаконичный", "Отвечай максимально коротко: одно предложение, без вступлений и вежливых оборотов. Только суть."],
    ["Детский", "Ты — добрый помощник для ребёнка. Говоришь просто, тепло и весело, без сложных слов и без страшного. Хвалишь за интересные вопросы."],
  ];

  let { initial, promptText = "", onedit, onprompt } = $props();
  let draft = $state({ ...untrack(() => initial) });
  let text = $state(untrack(() => promptText));
  let dragOver = $state(false);
  let uploading = $state(false);
  let pendingPreset = $state(null);

  let initialLetter = $derived((draft.name || "?").trim()[0] || "?");

  function edit(field, value) {
    if (draft[field] === value) return;
    draft[field] = value;
    onedit({ [field]: value });
  }

  function editPrompt(value) {
    text = value;
    pendingPreset = null;
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

  function applyPreset(presetName, presetText) {
    if (text.trim() && text.trim() !== presetText) {
      pendingPreset = { name: presetName, text: presetText };
      return;
    }
    editPrompt(presetText);
  }

  function confirmApplyPreset() {
    if (!pendingPreset) return;
    editPrompt(pendingPreset.text);
  }
</script>

<section class="card bento-card" id="profile-basics">
  <h2>{@html icon("sparkles")} Личность и характер</h2>
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
      <label for="p-tagline">Короткое описание</label>
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
      <div class="persona-char-head">
        <label for="p-prompt-text">Характер {@html tip("Как ассистент говорит, тон, обращение, привычки и роль. Передаётся модели в каждом разговоре.")}</label>
        <span class="meta">{text.length} симв.</span>
      </div>
      <div class="persona-presets" role="group" aria-label="Быстрые шаблоны характера">
        {#each presets as [presetName, presetText] (presetName)}
          <button type="button" class="chip persona-preset-btn" onclick={() => applyPreset(presetName, presetText)}>
            {presetName}
          </button>
        {/each}
      </div>
      {#if pendingPreset}
        <div class="inline-confirm" role="alert">
          <span>Заменить характер шаблоном «{pendingPreset.name}»?</span>
          <button class="btn danger sm" type="button" onclick={confirmApplyPreset}>
            {@html icon("check")} Заменить
          </button>
          <button class="btn ghost sm" type="button" onclick={() => { pendingPreset = null; }}>
            {@html icon("x")} Отмена
          </button>
        </div>
      {/if}
      <textarea
        id="p-prompt-text"
        rows="6"
        maxlength="18000"
        placeholder="Ты — … Опишите манеру речи, характер, тон, длину ответов и особенности персонажа"
        value={text}
        oninput={(event) => editPrompt(event.currentTarget.value)}
      ></textarea>
    </div>
  </div>
</section>
