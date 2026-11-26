<script>
  import SystemNav from "#components/Shell/SystemNav.svelte";
  import { act, api, icon, tip } from "#helpers/core.js";
  import { S, reload } from "#state/panelState.js";
  import { wizOpen } from "#state/wizard.js";
  import Backup from "./Backup/Backup.svelte";
  import Updates from "./Updates/Updates.svelte";

  let currentPassword = $state("");
  let newPassword = $state("");
  let changingPassword = $state(false);
  let lanOnly = $state(!!S.settings.lan_only);
  let changingAccess = $state(false);
  let confirmLanOpen = $state(false);
  let checks = $state.raw(S.security);

  async function changePassword(event) {
    event.preventDefault();
    if (changingPassword) return;
    changingPassword = true;
    await act(null, async () => {
      await api("PUT", "/api/password", { old: currentPassword, new: newPassword });
      currentPassword = "";
      newPassword = "";
    }, "Пароль сменён");
    changingPassword = false;
  }

  async function applyAccess(allowedOnlyFromLan) {
    confirmLanOpen = false;
    changingAccess = true;
    await act(null, async () => {
      await api("PUT", "/api/settings", { lan_only: allowedOnlyFromLan });
      lanOnly = allowedOnlyFromLan;
      await reload();
      checks = S.security;
    });
    changingAccess = false;
  }

  async function changeAccess(event) {
    const input = event.currentTarget;
    const allowedOnlyFromLan = input.checked;
    if (!allowedOnlyFromLan) {
      input.checked = lanOnly;
      confirmLanOpen = true;
      return;
    }
    confirmLanOpen = false;
    await applyAccess(true);
    input.checked = lanOnly;
  }
</script>

<SystemNav current="security" />

<header class="ph">
  <h1>Защита и бэкап</h1>
</header>

<section class="sec">
  <div class="sec-h">
    <h2>{@html icon("lock")}Пароль настроек</h2>
  </div>
  <form class="sec-b" onsubmit={changePassword}>
    <div class="grid g2">
      <div class="f">
        <label for="pw-old">Текущий</label>
        <input type="password" id="pw-old" autocomplete="current-password" bind:value={currentPassword} disabled={changingPassword}>
      </div>
      <div class="f">
        <label for="pw-new">Новый {@html tip("Минимум 6 символов.")}</label>
        <input type="password" id="pw-new" autocomplete="new-password" bind:value={newPassword} disabled={changingPassword}>
      </div>
    </div>
    <div><button class="btn" type="submit" disabled={changingPassword}>{@html icon("save")}Сменить пароль</button></div>
  </form>
</section>

<section class="sec">
  <div class="sec-h">
    <h2>{@html icon("shield")}Доступ к настройкам</h2>
  </div>
  <div class="sec-b">
    <label class="sw">
      <input type="checkbox" checked={lanOnly} disabled={changingAccess} onchange={changeAccess}>
      <span class="tr"></span>
      Только из домашней сети
      {@html tip("Настройки открываются только с устройств в вашей Wi-Fi сети. После 5 неверных паролей вход временно блокируется.")}
    </label>
    {#if confirmLanOpen}
      <div class="inline-confirm" role="alert">
        <span>Разрешить вход в настройки не только из домашней сети?</span>
        <button class="btn danger sm" type="button" disabled={changingAccess} onclick={() => applyAccess(false)}>
          {@html icon("check")}Разрешить
        </button>
        <button class="btn ghost sm" type="button" disabled={changingAccess} onclick={() => { confirmLanOpen = false; }}>
          {@html icon("x")}Отмена
        </button>
      </div>
    {/if}
  </div>
</section>

<section class="sec">
  <div class="sec-h">
    <h2>{@html icon("radar")}Служебные входы</h2>
  </div>
  <div class="sec-b">
    <div class="checks">
      {#each checks as check}
        <div class="check">
          <div class="body">
            <b>{check.title}</b>
            <small>{check.hint}</small>
          </div>
          <span class={["st", check.open ? "warn" : "ok"]}>{check.open ? "открыт" : "закрыт"}</span>
        </div>
      {:else}
        <p class="note">Проверять нечего</p>
      {/each}
    </div>
  </div>
</section>

<section class="sec">
  <div class="sec-h">
    <h2>{@html icon("refresh")}Обновления</h2>
  </div>
  <div class="sec-b"><Updates /></div>
</section>

<section class="sec">
  <div class="sec-h">
    <h2>{@html icon("archive")}Резервная копия</h2>
  </div>
  <div class="sec-b"><Backup /></div>
</section>

<section class="sec">
  <div class="sec-h">
    <h2>{@html icon("sparkles")}Знакомство</h2>
  </div>
  <div class="sec-b">
    <div class="row">
      <button class="btn" type="button" onclick={() => wizOpen(0)}>{@html icon("right")}Показать знакомство</button>
      <a class="btn ghost" href="https://github.com/newfpv/YaJarvis" target="_blank" rel="noopener noreferrer">
        {@html icon("ext")}GitHub
      </a>
    </div>
  </div>
</section>
