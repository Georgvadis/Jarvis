<script module>
  const defaultPhrase = "Джарвис";
  const positives = 10;
  const negatives = 3;
  const totalSamples = positives + negatives;

  function sampleLabel(index) {
    return index < positives ? `Фраза ${index + 1}` : `Фон ${index - positives + 1}`;
  }

  function sampleInstructions(count, word) {
    const label = word?.trim() || defaultPhrase;
    if (count === totalSamples) return "Все записи готовы — нажмите «Обучить модель».";
    if (count >= positives) return `Фон ${count - positives + 1} из ${negatives}: скажите обычную фразу без «${label}».`;
    return `Дубль ${count + 1} из ${positives}: нажмите «Записать» и скажите «${label}».`;
  }
</script>

<script>
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { api, icon, toast, upload } from "#helpers/core.js";
  import { reload, speaker } from "#state/panelState.js";
  import { PROFILES, loadWakeProfiles, modelLabel, recordWakeSample, selectWakeProfile, wakeSampleUrl } from "#helpers/wake-training.js";
  import styles from "./WakeTraining.module.css";

  const controller = new AbortController();
  const { signal } = controller;
  const results = new SvelteMap();

  let data = $state.raw(null);
  let loadError = $state("");
  let busy = $state("");
  let confirmRemoveId = $state("");
  let phrase = $state(defaultPhrase);
  let assignProfileId = $state("");
  let phraseInput = $state();
  let samples = $state.raw([]);
  let reviewed = $state(0);
  let prompt = $state("");
  let recordingLive = $state(false);
  let previewUrl = $state("");
  let audio = $state();
  const MIC_BARS = 36;
  let micOnline = $state(true);
  let micThreshold = $state(280);
  let micPeak = $state(0);
  let micHistory = $state(Array(MIC_BARS).fill(0));
  let micHeard = $derived(micOnline && micPeak >= micThreshold);

  function peakToPct(val) {
    if (!val || val <= 0) return 6;
    const norm = Math.sqrt(Math.min(val, 6000) / 6000);
    return Math.max(6, Math.min(100, Math.round(norm * 100)));
  }

  let thresholdPct = $derived(peakToPct(micThreshold));

  let assistants = $derived($speaker?.profiles || []);
  let complete = $derived(samples.length === totalSamples);
  let progressPct = $derived(Math.round((samples.length / totalSamples) * 100));
  let instructions = $derived(
    busy === "train" ? "Обучаю и проверяю модель…" : prompt || sampleInstructions(samples.length, phrase),
  );
  let savedCustom = $derived((data?.profiles || []).filter((p) => p.id));

  function assignedTo(wakeId) {
    return assistants.filter((a) => a.wake_profile === wakeId);
  }

  onMount(() => {
    refresh().catch((error) => {
      if (!signal.aborted) loadError = error.message;
    });
    let timer = null;
    let alive = true;
    async function pollMic() {
      if (!alive || signal.aborted) return;
      if (typeof document !== "undefined" && document.hidden) {
        timer = setTimeout(pollMic, 400);
        return;
      }
      try {
        const res = await api("GET", "/api/mic-level", undefined, { signal });
        if (!alive || signal.aborted) return;
        const pk = Number(res?.peak || 0);
        micOnline = res?.online !== false;
        if (res?.threshold) micThreshold = Number(res.threshold);
        micPeak = pk;
        micHistory = [...micHistory.slice(1), pk];
      } catch {
        // ignore transient poll errors
      }
      if (alive && !signal.aborted) timer = setTimeout(pollMic, 110);
    }
    pollMic();
    return () => {
      alive = false;
      if (timer) clearTimeout(timer);
      controller.abort();
      clearPreview();
    };
  });

  async function refresh() {
    data = await loadWakeProfiles(signal);
  }

  async function run(operation, action) {
    if (busy) return;
    busy = operation;
    try {
      await action();
    } catch (error) {
      if (!signal.aborted) toast(error.message, true);
    } finally {
      busy = "";
    }
  }

  function pickAssistant(asst) {
    if (busy || samples.length > 0) return;
    if (assignProfileId === asst.id) {
      assignProfileId = "";
      return;
    }
    assignProfileId = asst.id;
    phrase = asst.name;
  }

  function changeProfile(event) {
    const select = event.currentTarget;
    const id = select.value;
    select.value = data.selected;
    void run("select", async () => {
      await selectWakeProfile(id);
      if (signal.aborted) return;
      data = { ...data, selected: id };
      await reload();
    });
  }

  function removeProfile(profile) {
    confirmRemoveId = "";
    void run("remove", async () => {
      await api("DELETE", PROFILES + "/" + encodeURIComponent(profile.id));
      if (signal.aborted) return;
      results.delete(profile.id);
      await refresh();
      toast("Модель удалена");
    });
  }

  function testProfile(profile) {
    void run("test", async () => {
      audio?.pause();
      results.set(profile.id, { tone: "wait", text: "Приготовьтесь…" });
      try {
        const pcm = await recordWakeSample(signal, 800, () => {
          results.set(profile.id, { tone: "live", text: `Говорите «${profile.phrase}»` });
        });
        const result = await upload("POST", "/api/train/wake/test/" + encodeURIComponent(profile.id), pcm, { signal });
        results.set(profile.id, result.detected
          ? { tone: "ok", text: "Распознано" }
          : { tone: "bad", text: "Не распознано" });
      } catch (error) {
        if (!signal.aborted) results.set(profile.id, { tone: "bad", text: error.message });
      }
    });
  }

  function clearPreview() {
    audio?.pause();
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    previewUrl = "";
  }

  function resetSamples() {
    if (busy) return;
    clearPreview();
    samples = [];
    reviewed = 0;
  }

  function listen(index) {
    clearPreview();
    reviewed = index;
    if (samples[index]) previewUrl = wakeSampleUrl(samples[index]);
  }

  function record(index) {
    const word = phrase.trim();
    if (!word) {
      toast("Введите слово или фразу", true);
      phraseInput?.focus();
      return;
    }
    void run("record", async () => {
      clearPreview();
      prompt = "Приготовьтесь…";
      recordingLive = false;
      try {
        const pcm = await recordWakeSample(signal, 700, () => {
          recordingLive = true;
          prompt = index < positives ? `Говорите «${word}»` : "Скажите обычную фразу";
        });
        signal.throwIfAborted();
        const next = samples.slice();
        next[index] = pcm;
        samples = next;
      } finally {
        recordingLive = false;
        if (!signal.aborted) {
          prompt = "";
          listen(index < samples.length ? index : reviewed);
        }
      }
    });
  }

  async function maybeAssignToAssistant(newWakeId) {
    if (!assignProfileId || !newWakeId) return false;
    const target = assistants.find((a) => a.id === assignProfileId);
    if (!target) return false;
    await api("PUT", "/api/profile", { ...target, wake_profile: newWakeId });
    await reload();
    toast(`Слово обучено и привязано к «${target.name}»`);
    return true;
  }

  function uploadModel(event) {
    const input = event.currentTarget;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    const modelUrl = PROFILES + "?phrase=" + encodeURIComponent(phrase.trim());
    void run("upload", async () => {
      const res = await upload("PUT", modelUrl, file);
      if (signal.aborted) return;
      await refresh();
      const newId = res?.profile?.id || res?.id || "";
      const assigned = await maybeAssignToAssistant(newId);
      phrase = defaultPhrase;
      assignProfileId = "";
      if (!assigned) toast("Модель загружена");
    });
  }

  function train() {
    if (!complete) return;
    const word = phrase.trim();
    const trainUrl = "/api/train/wake/train?phrase=" + encodeURIComponent(word);
    void run("train", async () => {
      audio?.pause();
      const res = await upload("POST", trainUrl, new Blob(samples));
      if (signal.aborted) return;
      clearPreview();
      samples = [];
      reviewed = 0;
      await refresh();
      const newId = res?.profile?.id || res?.id || (data?.profiles || []).find((p) => p.phrase === word && p.id)?.id || "";
      const assigned = await maybeAssignToAssistant(newId);
      phrase = defaultPhrase;
      assignProfileId = "";
      if (!assigned) toast("Модель обучена и сохранена");
    });
  }
</script>

{#if loadError}
  <p class="err" role="alert">{loadError}</p>
{:else if !data}
  <div class="card plg-empty" role="status"><span class="spin"></span><p class="muted">Загружаю фразы…</p></div>
{:else}
  <div class="bento-2">
    <section class="card bento-card">
      <h2>{@html icon("mic")} Сохранённые слова</h2>
      <div class={["studio-stack", styles.fields]}>
        <div class="f">
          <label for="wake-pick">Общее слово колонки</label>
          <select id="wake-pick" value={data.selected} disabled={!!busy} onchange={changeProfile}>
            {#each data.profiles as profile (profile.id)}
              <option value={profile.id}>{modelLabel(profile)}</option>
            {/each}
          </select>
        </div>

        {#if savedCustom.length}
          <div class={styles.savedList}>
            {#each savedCustom as profile (profile.id)}
              {@const res = results.get(profile.id)}
              {@const linked = assignedTo(profile.id)}
              {@const inUse = profile.id === data.selected || linked.length > 0}
              <div class={styles.savedRow}>
                <div class={styles.savedMeta}>
                  <b class={styles.savedPhrase}>{profile.phrase}</b>
                  <div class={styles.savedBadges}>
                    {#if profile.id === data.selected}
                      <span class="chip">Общее</span>
                    {/if}
                    {#each linked as a (a.id)}
                      <span class="chip">{a.name}</span>
                    {/each}
                    {#if res}
                      <span class={["badge", res.tone === "ok" ? "ok" : res.tone === "bad" ? "bad" : "acc"]}>{res.text}</span>
                    {/if}
                  </div>
                </div>
                <div class={styles.savedActs}>
                  {#if data.training}
                    <button class="btn sm ghost" type="button" disabled={!!busy} onclick={() => testProfile(profile)}>
                      {@html icon("mic")} Проверить
                    </button>
                  {/if}
                  {#if !inUse}
                    {#if confirmRemoveId === profile.id}
                      <button class="ib danger" type="button" disabled={!!busy} aria-label="Подтвердить удаление" onclick={() => removeProfile(profile)}>
                        {@html icon("check")}
                      </button>
                      <button class="ib" type="button" disabled={!!busy} aria-label="Отмена" onclick={() => { confirmRemoveId = ""; }}>
                        {@html icon("x")}
                      </button>
                    {:else}
                      <button class="ib danger" type="button" disabled={!!busy} title="Удалить модель" aria-label="Удалить модель" onclick={() => { confirmRemoveId = profile.id; }}>
                        {@html icon("trash")}
                      </button>
                    {/if}
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <p class="empty-msg">Обученные слова появятся здесь — их можно привязать к конкретным персонажам.</p>
        {/if}
      </div>
    </section>

    <section class="card bento-card">
      <h2>{@html icon("sparkles")} Обучить слово</h2>
      <div class={["studio-stack", styles.fields]}>
        {#if assistants.length}
          <div class="f">
            <span class="lbl">Для персонажа</span>
            <div class={styles.asstPicks}>
              {#each assistants as asst (asst.id)}
                <button
                  type="button"
                  class={styles.asstPick}
                  class:on={assignProfileId === asst.id}
                  disabled={!!busy || samples.length > 0}
                  onclick={() => pickAssistant(asst)}
                >
                  <span class={"av sm c" + (asst.color || 1)} class:av-img={Boolean(asst.avatar)}>
                    {#if asst.avatar}<img src={asst.avatar} alt="">{:else}{(asst.name || "?").trim()[0] || "?"}{/if}
                  </span>
                  <span>{asst.name}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}

        <div class="f">
          <label for="wake-phrase">Слово или фраза</label>
          <input id="wake-phrase" type="text" maxlength="48" autocomplete="off" placeholder="Например: Рюк или Аянами"
            bind:this={phraseInput} bind:value={phrase} readonly={!!busy || samples.length > 0}>
        </div>

        {#if data.training}
          <div class={styles.recorder}>
            <div class={styles.stage} class:live={recordingLive} class:done={complete}>
              <div class={styles.stageTop}>
                <span class={styles.stageIcon}>{@html icon(complete ? "check" : "mic")}</span>
                <p class={styles.stageText} role="status">{instructions}</p>
                <span class={styles.stageCount}>{samples.length}/{totalSamples}</span>
              </div>

              <div class={styles.micBox} aria-label="Уровень микрофона колонки">
                <div class={styles.micHead}>
                  <span class={styles.micDot} class:on={micHeard} class:off={!micOnline}></span>
                  <span class={styles.micLabel}>
                    {!micOnline ? "Микрофон недоступен" : micHeard ? "Слышу голос" : "Микрофон колонки"}
                  </span>
                  <span class={styles.micVal}>{micPeak}</span>
                </div>
                <div class={styles.micWave}>
                  <span class={styles.micThresh} style:bottom={`${thresholdPct}%`} title={`Порог речи: ${micThreshold}`}></span>
                  {#each micHistory as val, i (i)}
                    <span
                      class={styles.micBar}
                      class:hot={val >= micThreshold}
                      style:height={`${peakToPct(val)}%`}
                    ></span>
                  {/each}
                </div>
              </div>

              <div class={styles.slots} role="group" aria-label="Записанные дубли">
                {#each Array.from({ length: totalSamples }, (_, idx) => idx) as idx (idx)}
                  {@const recorded = idx < samples.length}
                  {@const isNeg = idx >= positives}
                  <button
                    type="button"
                    class={styles.slot}
                    class:rec={recorded}
                    class:neg={isNeg}
                    class:sel={recorded && reviewed === idx}
                    disabled={!recorded || !!busy}
                    title={sampleLabel(idx)}
                    aria-label={sampleLabel(idx)}
                    onclick={() => listen(idx)}
                  ></button>
                {/each}
              </div>

              <div class={styles.bar}><span style:width={`${progressPct}%`}></span></div>
            </div>

            <div class="row">
              {#if !complete}
                <button class="btn primary" type="button" disabled={!!busy} onclick={() => record(samples.length)}>
                  {@html icon("mic")}
                  <span>{busy === "record" ? "Записываю…" : samples.length < positives ? `Записать слово (${samples.length + 1}/${positives})` : `Записать фон (${samples.length - positives + 1}/${negatives})`}</span>
                </button>
              {:else}
                <button class="btn primary" type="button" disabled={!!busy} onclick={train}>
                  {@html icon("sparkles")}
                  <span>{busy === "train" ? "Обучаю…" : "Обучить модель"}</span>
                </button>
              {/if}

              {#if samples.length > 0}
                <button class="btn ghost" type="button" disabled={!!busy} onclick={() => record(reviewed)} title={`Перезаписать ${sampleLabel(reviewed)}`}>
                  {@html icon("refresh")} Перезаписать ({sampleLabel(reviewed)})
                </button>
                <button class="ib danger" type="button" disabled={!!busy} title="Сбросить все записи" aria-label="Сбросить все записи" onclick={resetSamples}>
                  {@html icon("trash")}
                </button>
              {/if}
            </div>

            {#if previewUrl}
              <div class={styles.preview}>
                <span class="meta">{sampleLabel(reviewed)}</span>
                <audio bind:this={audio} src={previewUrl} controls preload="metadata"></audio>
              </div>
            {/if}
          </div>
        {/if}

        <div class="nav-sep"></div>

        <div class={styles.uploadRow}>
          <label class="btn sm ghost" class:disabled={!!busy || samples.length > 0}>
            {@html icon("upload")}
            <span>Загрузить готовую .bin модель</span>
            <input type="file" accept=".bin,application/octet-stream" hidden disabled={!!busy || samples.length > 0} onchange={uploadModel}>
          </label>
        </div>
      </div>
    </section>
  </div>
{/if}
