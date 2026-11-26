<script>
  import { untrack } from "svelte";
  import { icon, tip } from "#helpers/core.js";

  const colors = [1, 2, 3, 4, 5, 6];
  let { initial, prompts, onedit } = $props();
  let draft = $state({ ...untrack(() => initial) });

  function edit(field, value) {
    if (draft[field] === value) return;
    draft[field] = value;
    onedit({ [field]: value });
  }

  export function updateSavedName(name, updateDraft) {
    if (updateDraft) draft.name = name;
  }
</script>

<section class="card bento-card" id="profile-basics">
  <h2>{@html icon("sparkles")} Образ и характер</h2>
  <div class="studio-stack">
    <div class="f">
      <label for="p-name">Имя {@html tip("Отображается в шапке и списке ассистентов. На слово активации не влияет.")}</label>
      <input id="p-name" type="text" maxlength="40" value={draft.name} placeholder="Например, Джарвис"
        oninput={(event) => edit("name", event.currentTarget.value)}>
    </div>
    <div class="f">
      <span class="lbl">Цвет аватара</span>
      <div class="colors" id="p-color" role="group" aria-label="Цвет ассистента">
        {#each colors as color (color)}
          <button class={"av c" + color} class:on={draft.color === color} data-c={color} type="button"
            aria-label={"Цвет " + color} aria-pressed={draft.color === color} onclick={() => edit("color", color)}></button>
        {/each}
      </div>
    </div>
    <div class="f">
      <label for="p-prompt">Характер {@html tip("Системный промпт: кто такой ассистент и как он общается.")}</label>
      <select id="p-prompt" value={draft.prompt} onchange={(event) => edit("prompt", event.currentTarget.value)}>
        {#each prompts as prompt (prompt.id)}
          <option value={prompt.id}>{prompt.name}</option>
        {/each}
      </select>
    </div>
    <div>
      <a class="btn sm ghost" href="/characters">{@html icon("smile")} Редактировать характеры</a>
    </div>
  </div>
</section>
