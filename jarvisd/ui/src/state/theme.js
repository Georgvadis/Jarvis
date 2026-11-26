import { writable } from "svelte/store";

export const ACCENTS = [
  { id: "orange", label: "Оранж" },
  { id: "blue", label: "Кибер-синий" },
  { id: "emerald", label: "Изумруд" },
  { id: "violet", label: "Аметист" },
  { id: "rose", label: "Малиновый" },
];

const THEME_KEY = "jarvis.theme";
const ACCENT_KEY = "jarvis.accent";
const VALID_ACCENTS = new Set(ACCENTS.map((a) => a.id));

export const themeState = writable({ mode: "dark", accent: "orange" });

function applyDom(mode, accent) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (mode === "light") root.dataset.theme = "light";
  else delete root.dataset.theme;
  if (accent && accent !== "orange") root.dataset.accent = accent;
  else delete root.dataset.accent;
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
