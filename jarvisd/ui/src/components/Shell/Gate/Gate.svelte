<script>
  import { tick } from "svelte";
  import { icon } from "#helpers/core.js";

  let { access, login, retry = null } = $props();
  let visible = $derived(access.status === "gate" || access.status === "loading" || access.status === "error");
  let setup = $derived(Boolean(access.setup));
  let input = $state();
  let password = $state("");
  let confirmation = $state("");
  let error = $state("");
  let pending = $state(false);

  $effect(() => {
    if (access.status === "gate") void tick().then(() => input?.focus());
    else password = confirmation = error = "";
  });

  async function submit(event) {
    event.preventDefault();
    if (pending) return;
    error = "";
    if (setup && password !== confirmation) {
      error = "Пароли не совпадают";
      return;
    }
    pending = true;
    try {
      await login(setup, password);
    } catch (err) {
      error = err.message;
    } finally {
      pending = false;
    }
  }
</script>

<div id="gate" class="gate" class:hidden={!visible}>
  {#if access.status === "loading"}
    <div class="gate-card" role="status" aria-live="polite">
      <div class="gate-brand">
        <span class="brand-mark" aria-hidden="true">
          <svg viewBox="0 0 32 32" width="22" height="22" fill="none" aria-hidden="true">
            <circle cx="16" cy="16" r="11.5" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-dasharray="14 5 7 5"/>
            <circle cx="16" cy="16" r="6.8" stroke="currentColor" stroke-opacity=".55" stroke-width="1.4" stroke-dasharray="8 4"/>
            <circle cx="16" cy="16" r="3" fill="currentColor"/>
          </svg>
        </span>
        <span>Jarvis</span>
      </div>
      <p class="gate-text"><span class="spin"></span> Подключение к колонке…</p>
    </div>
  {:else if access.status === "error"}
    <div class="gate-card" role="alert">
      <div class="gate-brand">
        <span class="brand-mark" aria-hidden="true">
          <svg viewBox="0 0 32 32" width="22" height="22" fill="none" aria-hidden="true">
            <circle cx="16" cy="16" r="11.5" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-dasharray="14 5 7 5"/>
            <circle cx="16" cy="16" r="6.8" stroke="currentColor" stroke-opacity=".55" stroke-width="1.4" stroke-dasharray="8 4"/>
            <circle cx="16" cy="16" r="3" fill="currentColor"/>
          </svg>
        </span>
        <span>Jarvis</span>
      </div>
      <p class="err">{access.message || "Не удалось подключиться к колонке"}</p>
      {#if retry}
        <button class="btn primary" type="button" onclick={() => void retry()}>
          {@html icon("refresh")}<span>Повторить</span>
        </button>
      {/if}
    </div>
  {:else}
  <form id="gate-form" class="gate-card" autocomplete="on" onsubmit={submit}>
    <div class="gate-brand">
      <span class="brand-mark" aria-hidden="true">
        <svg viewBox="0 0 32 32" width="22" height="22" fill="none" aria-hidden="true">
          <circle cx="16" cy="16" r="11.5" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-dasharray="14 5 7 5"/>
          <circle cx="16" cy="16" r="6.8" stroke="currentColor" stroke-opacity=".55" stroke-width="1.4" stroke-dasharray="8 4"/>
          <circle cx="16" cy="16" r="3" fill="currentColor"/>
        </svg>
      </span>
      <span>Jarvis</span>
    </div>
    <p id="gate-text" class="gate-text">{setup ? "Задайте пароль для панели" : "Вход в панель"}</p>
    <input id="gate-pass" type="password" autocomplete={setup ? "new-password" : "current-password"}
      placeholder="Пароль" required bind:this={input} bind:value={password}>
    <input id="gate-pass2" type="password" autocomplete="new-password" placeholder="Пароль ещё раз"
      class:hidden={!setup} bind:value={confirmation}>
    <button class="btn primary" id="gate-btn" type="submit" disabled={pending}>
      {@html icon(setup ? "save" : "logIn")}
      <span>{setup ? "Сохранить пароль" : "Войти"}</span>
    </button>
    <p id="gate-err" class="err">{error}</p>
    <details id="gate-forgot" class="gate-forgot" class:hidden={setup}>
      <summary>Забыли пароль?</summary>
      <p>Нажмите на колонке кнопки громкости по очереди 8 раз: <b>+ − + − + − + −</b>, между нажатиями не дольше полутора секунд. Колонка скажет, что пароль сброшен, — в течение 30 минут обновите эту страницу и задайте новый.</p>
    </details>
  </form>
  {/if}
</div>
