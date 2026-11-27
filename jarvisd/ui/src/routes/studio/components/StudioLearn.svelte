<script>
  import { onMount } from "svelte";
  import { api, icon, toast } from "#helpers/core.js";
  import { activity, refreshActivity } from "#state/activity.js";

  const STARTER_COURSES = [
    { label: "Английский B1", topic: "Английский язык для повседневного общения", level: "A2 → B1", goal: "Свободно говорить и понимать на слух" },
    { label: "Английский для IT", topic: "Английский для IT и собеседований", level: "B1", goal: "Уверенно проходить интервью и созвоны" },
    { label: "Японский с нуля", topic: "Разговорный японский язык", level: "С нуля", goal: "Базовые фразы, вежливая речь и путешествия" },
    { label: "История мира", topic: "Ключевые эпохи мировой истории", level: "Базовый", goal: "Понимать связи событий и эпох" },
  ];

  const LEVELS = ["С нуля", "A1", "A2", "A2 → B1", "B1", "B2", "C1", "Продвинутый"];

  let courses = $state([]);
  let nowTs = $state(0);
  let loading = $state(true);
  let busy = $state(false);
  let genBusy = $state(false);
  let selectedId = $state("");
  let showCreator = $state(false);
  let confirmDelete = $state(false);
  let subTab = $state("lessons"); // "lessons" | "cards"

  let newTopic = $state("");
  let newLevel = $state("A2 → B1");
  let newGoal = $state("");

  let cardFront = $state("");
  let cardBack = $state("");
  let revealed = $state({});

  let activeMode = $derived($activity.modes || {});
  let tutorLive = $derived(activeMode.mode === "tutor");
  let selected = $derived(courses.find((c) => c.id === selectedId) || courses[0] || null);
  let doneCount = $derived(selected ? (selected.lessons || []).filter((l) => l.status === "done").length : 0);
  let totalLessons = $derived(selected ? (selected.lessons || []).length : 0);
  let progressPct = $derived(totalLessons ? Math.round((doneCount / totalLessons) * 100) : 0);
  let dueCount = $derived(
    selected ? (selected.cards || []).filter((c) => !c.next_review || c.next_review <= (nowTs || Date.now() / 1000)).length : 0
  );

  onMount(() => {
    const ac = new AbortController();
    (async () => {
      try {
        const res = await api("GET", "/api/courses", undefined, { signal: ac.signal });
        courses = Array.isArray(res?.courses) ? res.courses : [];
        nowTs = Number(res?.now || 0);
        if (courses.length) {
          selectedId = activeMode.course_id && courses.some((c) => c.id === activeMode.course_id)
            ? activeMode.course_id
            : courses[0].id;
        } else {
          showCreator = true;
        }
      } catch (err) {
        if (!ac.signal.aborted) toast(err.message || "Не удалось загрузить курсы", true);
      } finally {
        if (!ac.signal.aborted) loading = false;
      }
    })();
    return () => ac.abort();
  });

  function applyStarter(s) {
    newTopic = s.topic;
    newLevel = s.level;
    newGoal = s.goal;
    showCreator = true;
  }

  async function generateCourse(event) {
    event?.preventDefault();
    if (!newTopic.trim()) {
      toast("Укажите предмет или тему курса", true);
      return;
    }
    if (genBusy || busy) return;
    genBusy = true;
    try {
      const res = await api("POST", "/api/courses/generate", {
        topic: newTopic.trim(),
        level: newLevel,
        goal: newGoal.trim(),
      });
      if (Array.isArray(res?.courses)) courses = res.courses;
      if (res?.course?.id) selectedId = res.course.id;
      newTopic = "";
      newGoal = "";
      showCreator = false;
      toast("Программа курса готова");
    } catch (err) {
      toast(err.message || "Не удалось составить курс", true);
    } finally {
      genBusy = false;
    }
  }

  async function startLesson(course, lessonIdx) {
    if (busy) return;
    busy = true;
    try {
      const res = await api("POST", `/api/courses/${encodeURIComponent(course.id)}/start`, {
        lesson_idx: lessonIdx,
      });
      if (res?.course) {
        courses = courses.map((c) => (c.id === res.course.id ? res.course : c));
      }
      await refreshActivity();
      toast(`Урок ${lessonIdx + 1} запущен на колонке`);
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function finishLesson(course, sendTelegram = false) {
    if (busy) return;
    busy = true;
    try {
      const idx = tutorLive && activeMode.course_id === course.id
        ? Number(activeMode.lesson_idx ?? course.current_lesson)
        : course.current_lesson;
      const res = await api("POST", `/api/courses/${encodeURIComponent(course.id)}/finish`, {
        lesson_idx: idx,
        telegram: sendTelegram,
      });
      if (Array.isArray(res?.courses)) courses = res.courses;
      await refreshActivity();
      toast(sendTelegram ? "Урок сохранён и отправлен в Telegram" : "Прогресс урока сохранён");
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function exportTelegram(course) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", `/api/courses/${encodeURIComponent(course.id)}/export`, {
        lesson_idx: course.current_lesson,
      });
      toast("Конспект отправлен в Telegram");
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function startBlitz(course) {
    if (busy) return;
    busy = true;
    try {
      await api("POST", `/api/courses/${encodeURIComponent(course.id)}/blitz`, {});
      await refreshActivity();
      toast("Блиц-экзамен запущен на колонке");
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function deleteCourse(course) {
    if (busy) return;
    busy = true;
    try {
      const res = await api("DELETE", `/api/courses/${encodeURIComponent(course.id)}`);
      courses = Array.isArray(res?.courses) ? res.courses : [];
      confirmDelete = false;
      selectedId = courses[0]?.id || "";
      if (!courses.length) showCreator = true;
      toast("Курс удалён");
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function addCard(event) {
    event?.preventDefault();
    if (!selected || !cardFront.trim() || !cardBack.trim() || busy) return;
    busy = true;
    try {
      const res = await api("POST", `/api/courses/${encodeURIComponent(selected.id)}/card`, {
        op: "add",
        front: cardFront.trim(),
        back: cardBack.trim(),
      });
      if (Array.isArray(res?.courses)) courses = res.courses;
      cardFront = "";
      cardBack = "";
      toast("Карточка добавлена");
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function reviewCard(idx, ok) {
    if (!selected || busy) return;
    busy = true;
    try {
      const res = await api("POST", `/api/courses/${encodeURIComponent(selected.id)}/card`, {
        op: "review",
        index: idx,
        ok,
      });
      if (Array.isArray(res?.courses)) courses = res.courses;
      revealed = { ...revealed, [idx]: false };
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }

  async function deleteCard(idx) {
    if (!selected || busy) return;
    busy = true;
    try {
      const res = await api("POST", `/api/courses/${encodeURIComponent(selected.id)}/card`, {
        op: "delete",
        index: idx,
      });
      if (Array.isArray(res?.courses)) courses = res.courses;
    } catch (err) {
      toast(err.message, true);
    } finally {
      busy = false;
    }
  }
</script>

<div class="studio-stack">
  <section class="card">
    <div class="card-head">
      <h2>{@html icon("graduation")} Курсы</h2>
      <button type="button" class="btn sm ghost" onclick={() => { showCreator = !showCreator; }}>
        {@html icon(showCreator ? "x" : "plus")} {showCreator ? "Скрыть" : "Новый курс"}
      </button>
    </div>

    {#if courses.length}
      <div class="chip-row" style="margin-bottom: {showCreator ? '14px' : '0'};">
        {#each courses as c (c.id)}
          {@const isLive = tutorLive && activeMode.course_id === c.id}
          <button
            type="button"
            class="chip"
            class:on={selected?.id === c.id}
            onclick={() => { selectedId = c.id; confirmDelete = false; }}
          >
            {#if isLive}<span class="sdot"></span>{/if}
            {c.title}
          </button>
        {/each}
      </div>
    {/if}

    {#if showCreator}
      <form class="studio-stack learn-create" onsubmit={generateCourse}>
        <div class="chip-row">
          {#each STARTER_COURSES as st (st.label)}
            <button type="button" class="chip" disabled={genBusy} onclick={() => applyStarter(st)}>
              {@html icon("sparkles")} {st.label}
            </button>
          {/each}
        </div>

        <div class="row sched-row-fields">
          <div class="f">
            <label for="crs-topic">Что изучаем</label>
            <input id="crs-topic" type="text" maxlength="120" placeholder="Напр.: Английский язык, Rust, История" bind:value={newTopic} disabled={genBusy}>
          </div>
          <div class="f" style="max-width: 180px;">
            <label for="crs-level">Уровень</label>
            <select id="crs-level" bind:value={newLevel} disabled={genBusy}>
              {#each LEVELS as lv (lv)}
                <option value={lv}>{lv}</option>
              {/each}
            </select>
          </div>
        </div>

        <div class="f">
          <label for="crs-goal">Цель (необяз.)</label>
          <input id="crs-goal" type="text" maxlength="200" placeholder="Напр.: уверенно говорить в поездках и на созвонах" bind:value={newGoal} disabled={genBusy}>
        </div>

        <div class="row">
          <button type="submit" class="btn primary" disabled={genBusy}>
            {@html icon("wand")} {genBusy ? "Составляю программу…" : "Создать курс с ИИ"}
          </button>
        </div>
      </form>
    {/if}
  </section>

  {#if loading}
    <div class="card plg-empty"><span class="spin"></span><p class="muted">Загружаю курсы…</p></div>
  {:else if selected}
    <section class="card">
      <div class="card-head">
        <div>
          <h2>{selected.title}</h2>
          <div class="chip-row" style="margin-top: 6px;">
            <span class="badge">{selected.level}</span>
            <span class="badge">{doneCount}/{totalLessons} уроков</span>
            {#if selected.bilingual}
              <span class="badge acc">Whisper RU+EN</span>
            {/if}
            {#if dueCount > 0}
              <span class="badge warn">{dueCount} на повторение</span>
            {/if}
          </div>
        </div>
        <div class="row" style="flex-wrap: wrap;">
          {#if tutorLive && activeMode.course_id === selected.id}
            <button type="button" class="btn sm primary" disabled={busy} onclick={() => finishLesson(selected, false)}>
              {@html icon("check")} Завершить и сохранить
            </button>
            <button type="button" class="btn sm ghost" disabled={busy} onclick={() => finishLesson(selected, true)} title="Сохранить и отправить конспект в Telegram">
              {@html icon("send")} В Telegram
            </button>
          {:else}
            <button type="button" class="btn sm primary" disabled={busy} onclick={() => startLesson(selected, selected.current_lesson)}>
              {@html icon("play", "solid")} Продолжить (урок {selected.current_lesson + 1})
            </button>
            <button type="button" class="btn sm ghost" disabled={busy} onclick={() => startBlitz(selected)} title="Быстрая проверка карточек голосом">
              {@html icon("bolt")} Блиц
            </button>
            <button type="button" class="btn sm ghost" disabled={busy} onclick={() => exportTelegram(selected)} title="Отправить конспект в Telegram">
              {@html icon("send")} Конспект
            </button>
          {/if}
          {#if confirmDelete}
            <button type="button" class="btn sm danger" disabled={busy} onclick={() => deleteCourse(selected)}>{@html icon("check")}</button>
            <button type="button" class="btn sm ghost" disabled={busy} onclick={() => { confirmDelete = false; }}>{@html icon("x")}</button>
          {:else}
            <button type="button" class="ib danger" disabled={busy} title="Удалить курс" onclick={() => { confirmDelete = true; }}>
              {@html icon("trash")}
            </button>
          {/if}
        </div>
      </div>

      <div class="learn-bar"><span style:width={`${progressPct}%`}></span></div>

      <div class="hub-tabs" style="margin-top: 12px;">
        <button type="button" class="hub-tab" class:on={subTab === "lessons"} onclick={() => { subTab = "lessons"; }}>
          {@html icon("list")} Программа ({totalLessons})
        </button>
        <button type="button" class="hub-tab" class:on={subTab === "cards"} onclick={() => { subTab = "cards"; }}>
          {@html icon("layers")} Карточки ({selected.cards?.length || 0})
        </button>
      </div>

      {#if subTab === "lessons"}
        <div class="learn-list">
          {#each selected.lessons || [] as lesson, idx (idx)}
            {@const isRunning = tutorLive && activeMode.course_id === selected.id && Number(activeMode.lesson_idx) === idx}
            <div class="learn-row" class:done={lesson.status === "done"} class:live={isRunning}>
              <div class="learn-num">{idx + 1}</div>
              <div class="learn-body">
                <div class="learn-top">
                  <strong>{lesson.title}</strong>
                  {#if isRunning}
                    <span class="badge acc">Идёт урок</span>
                  {:else if lesson.status === "done"}
                    <span class="badge ok">Пройден</span>
                  {:else if idx === selected.current_lesson}
                    <span class="badge">Текущий</span>
                  {/if}
                </div>
                {#if lesson.goal}
                  <div class="learn-meta">{lesson.goal}</div>
                {/if}
                {#if lesson.key_points?.length}
                  <div class="learn-pts">{lesson.key_points.join(" · ")}</div>
                {/if}
                {#if lesson.notes}
                  <div class="learn-notes">{lesson.notes}</div>
                {/if}
              </div>
              <div class="learn-act">
                <button
                  type="button"
                  class="btn sm {lesson.status === 'done' ? 'ghost' : 'primary'}"
                  disabled={busy}
                  onclick={() => startLesson(selected, idx)}
                >
                  {@html icon(lesson.status === "done" ? "refresh" : "play", lesson.status === "done" ? "" : "solid")}
                  {lesson.status === "done" ? "Повторить" : "Начать"}
                </button>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="studio-stack" style="margin-top: 12px;">
          <form class="row" onsubmit={addCard}>
            <input type="text" maxlength="120" placeholder="Слово / вопрос" bind:value={cardFront} disabled={busy}>
            <input type="text" maxlength="200" placeholder="Перевод / ответ" bind:value={cardBack} disabled={busy}>
            <button type="submit" class="btn sm primary" disabled={busy}>{@html icon("plus")} Добавить</button>
          </form>

          {#if selected.weak_spots?.length}
            <div class="chip-row">
              <span class="lbl">На закрепление:</span>
              {#each selected.weak_spots as w (w)}
                <span class="chip">{w}</span>
              {/each}
            </div>
          {/if}

          {#if selected.cards?.length}
            <div class="learn-list">
              {#each selected.cards as card, idx (idx)}
                <div class="learn-row">
                  <div class="learn-body">
                    <div class="learn-top">
                      <strong>{card.front}</strong>
                      <span class="badge">{card.interval_days}д</span>
                    </div>
                    {#if revealed[idx]}
                      <div class="learn-notes">{card.back}</div>
                    {/if}
                  </div>
                  <div class="row">
                    {#if !revealed[idx]}
                      <button type="button" class="btn sm ghost" onclick={() => { revealed = { ...revealed, [idx]: true }; }}>
                        {@html icon("eye")} Ответ
                      </button>
                    {:else}
                      <button type="button" class="btn sm primary" disabled={busy} onclick={() => reviewCard(idx, true)}>
                        {@html icon("check")} Знаю
                      </button>
                      <button type="button" class="btn sm ghost" disabled={busy} onclick={() => reviewCard(idx, false)}>
                        {@html icon("refresh")} Ещё
                      </button>
                    {/if}
                    <button type="button" class="ib danger" disabled={busy} aria-label="Удалить" onclick={() => deleteCard(idx)}>
                      {@html icon("trash")}
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {:else}
            <p class="empty-msg">Карточки появятся после прохождения уроков или их можно добавить вручную.</p>
          {/if}
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .learn-create {
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .learn-bar {
    height: 6px;
    border-radius: 999px;
    background: var(--glass-inset-hover, var(--raise));
    overflow: hidden;
    margin-top: 12px;
  }
  .learn-bar span {
    display: block;
    height: 100%;
    background: var(--acc);
    border-radius: 999px;
    transition: width .25s ease;
  }
  .learn-list {
    display: grid;
    gap: 8px;
    margin-top: 12px;
  }
  .learn-row {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px;
    border-radius: 14px;
    background: var(--glass-inset, var(--raise));
    border: 1px solid var(--glass-border, var(--line));
  }
  .learn-row.live {
    border-color: color-mix(in srgb, var(--acc) 60%, var(--line));
  }
  .learn-row.done {
    opacity: .88;
  }
  .learn-num {
    width: 28px;
    height: 28px;
    border-radius: 9px;
    background: var(--glass-inset-hover, var(--card));
    display: grid;
    place-items: center;
    font: 700 12.5px/1 var(--mono);
    color: var(--fg2);
    flex: none;
  }
  .learn-body {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 4px;
  }
  .learn-top {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .learn-meta {
    font-size: 13px;
    color: var(--fg2);
  }
  .learn-pts {
    font-size: 12px;
    color: var(--fg3);
  }
  .learn-notes {
    font-size: 12.5px;
    color: var(--acc-ink, var(--fg));
    padding: 6px 8px;
    border-radius: 8px;
    background: var(--acc-soft, var(--card));
    margin-top: 2px;
  }
  .learn-act {
    flex: none;
  }
  button.chip.on {
    background: var(--acc-soft, var(--raise));
    border-color: color-mix(in srgb, var(--acc) 55%, var(--line));
    color: var(--acc-ink, var(--fg));
  }
</style>
