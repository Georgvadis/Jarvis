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
};

export const activity = writable(DEFAULT_ACTIVITY);

let pollTimer = null;
let tickTimer = null;
let inflight = false;
let subscribers = 0;

export function fmtClock(sec) {
  const s = Math.max(0, Math.floor(sec || 0));
  const m = Math.floor(s / 60);
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
  pollTimer = setInterval(() => void refreshActivity(), 4000);
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
      stopUrl: "/api/focus",
      stopBody: { minutes: 0 },
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
      stopUrl: "/api/stop",
      stopBody: {},
    });
  }

  const mode = a.modes?.mode;
  if (mode && mode !== "normal" && !["melody", "detective", "party"].includes(mode)) {
    const label = mode === "rpg"
      ? "RPG-спектакль"
      : mode === "quiz"
        ? "Викторина"
        : `Перевод (${a.modes?.lang || ""})`;
    items.push({
      id: "modes",
      kind: "modes",
      icon: mode === "translator" ? "globe" : "shield",
      title: label,
      subtitle: "Голосовой режим",
      href: "/studio?tab=podcast",
      canNext: false,
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
  try {
    await api("POST", item.stopUrl, item.stopBody || {});
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
