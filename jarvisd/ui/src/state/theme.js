import { writable } from "svelte/store";

export const ACCENTS = [
  { id: "orange", label: "Камин", hex: "#f2994a" },
  { id: "emerald", label: "Хвоя", hex: "#63b888" },
  { id: "blue", label: "Вечер", hex: "#76a2d6" },
  { id: "violet", label: "Сумерки", hex: "#a892d6" },
  { id: "rose", label: "Угли", hex: "#df786d" },
];

const THEME_KEY = "jarvis.theme";
const ACCENT_KEY = "jarvis.accent";
const VALID_ACCENTS = new Map(ACCENTS.map((a) => [a.id, a.hex]));

export const themeState = writable({ mode: "dark", accent: "orange" });

function applyDom(mode, accent) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (mode === "light") root.dataset.theme = "light";
  else delete root.dataset.theme;
  if (accent && accent !== "orange") root.dataset.accent = accent;
  else delete root.dataset.accent;

  const metaTheme = document.querySelector('meta[name="theme-color"]');
  if (metaTheme) metaTheme.setAttribute("content", mode === "light" ? "#f5ede3" : "#120e0b");
  const metaScheme = document.querySelector('meta[name="color-scheme"]');
  if (metaScheme) metaScheme.setAttribute("content", mode === "light" ? "light" : "dark");

  const hex = VALID_ACCENTS.get(accent) || "#f2994a";
  document.querySelectorAll("#spark-fill stop").forEach((stop) => {
    stop.setAttribute("stop-color", hex);
  });
}

export function initTheme() {
  let mode = "dark";
  let accent = "orange";
  try {
    const savedMode = localStorage.getItem(THEME_KEY);
    if (savedMode === "light" || savedMode === "dark") mode = savedMode;
    const savedAccent = localStorage.getItem(ACCENT_KEY);
    if (savedAccent && VALID_ACCENTS.has(savedAccent)) accent = savedAccent;
  } catch { /* ignore storage errors */ }
  applyDom(mode, accent);
  themeState.set({ mode, accent });
}

export function setThemeMode(mode) {
  const nextMode = mode === "light" ? "light" : "dark";
  try { localStorage.setItem(THEME_KEY, nextMode); } catch { /* ignore */ }
  themeState.update((cur) => {
    applyDom(nextMode, cur.accent);
    return { ...cur, mode: nextMode };
  });
}

export function toggleThemeMode() {
  themeState.update((cur) => {
    const nextMode = cur.mode === "dark" ? "light" : "dark";
    try { localStorage.setItem(THEME_KEY, nextMode); } catch { /* ignore */ }
    applyDom(nextMode, cur.accent);
    return { ...cur, mode: nextMode };
  });
}

export function setAccent(accent) {
  const next = VALID_ACCENTS.has(accent) ? accent : "orange";
  try { localStorage.setItem(ACCENT_KEY, next); } catch { /* ignore */ }
  themeState.update((cur) => {
    applyDom(cur.mode, next);
    return { ...cur, accent: next };
  });
}
