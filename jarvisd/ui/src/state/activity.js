import { writable } from "svelte/store";
import { api, toast } from "#helpers/core.js";

const DEFAULT_ACTIVITY = {
  focus: { running: false, remaining_s: 0, total_min: 0 },
  modes: { mode: "normal", lang: "" },
  games: { active_game: "", melody: {}, detective: {}, party: {} },
  dj: { active: false, station: "", mood: "", current_title: "", likes: [] },
  reader: { running: false, book_id: "", title: "", chapter: 0, chapter_title: "", chapters_count: 0, multi_voice: false },
  ambient: { active: "", sleep_s: 0, volume: 70, presets: [] },
  debate_active: false,
  presence: { state: "unknown" },
  timers: [],
  custom_scenarios: [],
};

export const activity = writable(DEFAULT_ACTIVITY);

let pollTimer = null;
let tickTimer = null;
let inflight = false;
let subscribers = 0;

export function fmtClock(sec) {
  const s = Math.max(0, Math.floor(sec || 0));
  const m = Math.floor(s / 60);
  if (m >= 60) {
    const h = Math.floor(m / 60);
    return `${h}:${String(m % 60).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
  }
  return `${String(m).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

export async function refreshActivity() {
  if (inflight) return;
  inflight = true;
  try {
    const data = await api("GET", "/api/activity");
    activity.update((prev) => ({
      ...prev,
      ...data,
      timers: Array.isArray(data.timers) ? data.timers : [],
      custom_scenarios: Array.isArray(data.custom_scenarios) ? data.custom_scenarios : prev.custom_scenarios,
      ambient: {
        ...prev.ambient,
        ...(data.ambient || {}),
        presets: data.ambient?.presets?.length ? data.ambient.presets : prev.ambient.presets,
      },
    }));
  } catch { /* ignore background errors */ } finally {
    inflight = false;
  }
}

export function startActivityPolling() {
  subscribers++;
  if (pollTimer) return;
  void refreshActivity();
  pollTimer = setInterval(() => {
    if (!document.hidden) void refreshActivity();
  }, 4000);
  tickTimer = setInterval(() => {
    activity.update((cur) => {
      let changed = false;
      const next = { ...cur };
      if (cur.focus?.running && cur.focus.remaining_s > 0) {
        const rem = cur.focus.remaining_s - 1;
        next.focus = { ...cur.focus, remaining_s: rem, running: rem > 0 };
        changed = true;
      }
      if (cur.ambient?.active && cur.ambient.sleep_s > 0) {
        const sleep = cur.ambient.sleep_s - 1;
        next.ambient = { ...cur.ambient, sleep_s: sleep };
        changed = true;
      }
      if (Array.isArray(cur.timers) && cur.timers.length) {
        next.timers = cur.timers
          .map((t) => ({ ...t, remaining_s: Math.max(0, Number(t.remaining_s || 0) - 1) }))
          .filter((t) => t.remaining_s > 0);
        changed = true;
      }
      return changed ? next : cur;
    });
  }, 1000);
}

export function stopActivityPolling() {
  subscribers = Math.max(0, subscribers - 1);
  if (subscribers > 0 && pollTimer) return;
  clearInterval(pollTimer);
  clearInterval(tickTimer);
  pollTimer = null;
  tickTimer = null;
  activity.set(DEFAULT_ACTIVITY);
}

export function resetActivityPolling() {
  subscribers = 0;
  stopActivityPolling();
}

export function summarizeActivity(a) {
  if (!a) return [];
  const items = [];

  if (a.focus?.running) {
    items.push({
      id: "focus",
      kind: "focus",
      icon: "bolt",
      title: `Фокус ${fmtClock(a.focus.remaining_s)}`,
      subtitle: `${a.focus.total_min || 25} мин`,
      href: "/",
      canNext: false,
      stopMethod: "POST",
      stopUrl: "/api/focus",
      stopBody: { minutes: 0 },
    });
  }

  if (Array.isArray(a.timers) && a.timers.length) {
    const t = a.timers[0];
    items.push({
      id: "timer:" + t.id,
      kind: "timer",
      icon: "clock",
      title: `${t.label || "Таймер"} · ${fmtClock(t.remaining_s)}`,
      subtitle: a.timers.length > 1 ? `Ещё таймеров: ${a.timers.length - 1}` : "Таймер",
      href: "/studio?tab=schedule",
      canNext: false,
      stopMethod: "DELETE",
      stopUrl: "/api/schedule/timer/" + encodeURIComponent(t.id),
      stopBody: undefined,
    });
  }

  if (a.reader?.running) {
    items.push({
      id: "reader",
      kind: "reader",
      icon: "book",
      title: a.reader.title || "Читалка",
      subtitle: a.reader.chapter_title || `Глава ${(a.reader.chapter || 0) + 1}`,
      href: "/studio?tab=books",
      canNext: true,
      nextLabel: "След. глава",
      nextUrl: "/api/books/nav",
      nextBody: { delta: 1 },
      stopMethod: "POST",
      stopUrl: "/api/books/stop",
      stopBody: {},
    });
  }

  if (a.dj?.active) {
    items.push({
      id: "dj",
      kind: "dj",
      icon: "disc",
      title: a.dj.station || "AI-DJ",
      subtitle: a.dj.current_title || a.dj.mood || "Эфир",
      href: "/studio?tab=dj",
      canNext: true,
      nextLabel: "След. трек",
      nextUrl: "/api/dj/next",
      nextBody: {},
      stopMethod: "POST",
      stopUrl: "/api/dj/stop",
      stopBody: {},
    });
  }

  const g = a.games;
  if (g?.active_game === "melody") {
    items.push({
      id: "games",
      kind: "games",
      icon: "music",
      title: `Мелодия ${g.melody?.round || 0}/${g.melody?.total || 5}`,
      subtitle: `${g.melody?.theme || ""} · ${g.melody?.score || 0}`,
      href: "/studio?tab=games",
      canNext: true,
      nextLabel: "Пропустить",
      nextUrl: "/api/games/melody/skip",
      nextBody: {},
      stopMethod: "POST",
      stopUrl: "/api/games/stop",
      stopBody: {},
    });
  } else if (g?.active_game === "detective") {
    items.push({
      id: "games",
      kind: "games",
      icon: "eye",
      title: g.detective?.title || "Нуар-детектив",
      subtitle: `Улики ${g.detective?.clues_shown || 0}/${g.detective?.clues_total || 3}`,
      href: "/studio?tab=games",
      canNext: false,
      stopMethod: "POST",
      stopUrl: "/api/games/stop",
      stopBody: {},
    });
  } else if (g?.active_game === "party") {
    items.push({
      id: "games",
      kind: "games",
      icon: "users",
      title: `${g.party?.game === "bunker" ? "Бункер" : "Мафия"} · Р.${g.party?.round || 1}`,
      subtitle: g.party?.phase || "",
      href: "/studio?tab=games",
      canNext: true,
      nextLabel: "След. фаза",
      nextUrl: "/api/games/party/next",
      nextBody: {},
      stopMethod: "POST",
      stopUrl: "/api/games/stop",
      stopBody: {},
    });
  }

  if (a.debate_active) {
    items.push({
      id: "debate",
      kind: "debate",
      icon: "podcast",
      title: "Подкаст / Спор",
      subtitle: "Диалог двух ведущих",
      href: "/studio?tab=podcast",
      canNext: false,
      stopMethod: "POST",
      stopUrl: "/api/stop",
      stopBody: {},
    });
  }

  const mode = a.modes?.mode;
  if (mode && mode !== "normal" && !["melody", "detective", "party"].includes(mode)) {
    const modeMeta = mode === "rpg"
      ? { label: "RPG-спектакль", icon: "shield", href: "/studio?tab=games" }
      : mode === "quiz"
        ? { label: "Викторина", icon: "sparkles", href: "/studio?tab=games" }
        : mode === "tutor"
          ? { label: "Урок", icon: "book", href: "/studio?tab=learn" }
          : mode === "custom"
            ? { label: a.modes?.topic || "Своя игра", icon: "sparkles", href: "/studio?tab=games" }
            : mode === "chat"
              ? { label: "Свободный диалог", icon: "message", href: "/" }
              : { label: a.modes?.lang ? `Перевод (${a.modes.lang})` : "Переводчик", icon: "globe", href: "/studio?tab=podcast" };
    items.push({
      id: "modes",
      kind: "modes",
      icon: modeMeta.icon,
      title: modeMeta.label,
      subtitle: "Голосовой режим",
      href: modeMeta.href,
      canNext: false,
      stopMethod: "POST",
      stopUrl: "/api/modes",
      stopBody: { mode: "off" },
    });
  }

  if (a.ambient?.active) {
    const found = (a.ambient.presets || []).find((p) => p.id === a.ambient.active);
    const name = found?.name || a.ambient.active;
    items.push({
      id: "ambient",
      kind: "ambient",
      icon: "radio",
      title: name,
      subtitle: a.ambient.sleep_s > 0 ? `Сон ${Math.ceil(a.ambient.sleep_s / 60)}м` : "Фон / радио",
      href: "/",
      canNext: false,
      stopMethod: "POST",
      stopUrl: "/api/ambient",
      stopBody: { id: "off" },
    });
  }

  return items;
}

export async function runActivityNext(item) {
  if (!item?.nextUrl) return;
  try {
    const res = await api("POST", item.nextUrl, item.nextBody || {});
    if (res?.message) toast(res.message);
    await refreshActivity();
  } catch (error) {
    toast(error.message, true);
  }
}

export async function stopActivityItem(item) {
  if (!item?.stopUrl) return;
  const method = item.stopMethod || "POST";
  try {
    await api(method, item.stopUrl, method === "DELETE" ? undefined : (item.stopBody || {}));
    toast("Остановлено");
    await refreshActivity();
  } catch (error) {
    toast(error.message, true);
  }
}

export async function runScenario(id) {
  try {
    const res = await api("POST", "/api/scenario", { id });
    toast(res.message || "Сценарий запущен");
    await refreshActivity();
  } catch (error) {
    toast(error.message, true);
  }
}
