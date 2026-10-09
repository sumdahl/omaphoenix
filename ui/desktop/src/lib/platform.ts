// Bridge to the Rust side. Falls back gracefully in a plain browser (vite dev
// without Tauri), so the UI can be worked on anywhere.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface Theme {
  mode: "dark" | "light" | string;
  tokens: Record<string, string>;
  font: string;
}

const inTauri = "__TAURI_INTERNALS__" in window;

export async function loadTheme(): Promise<Theme | null> {
  return inTauri ? invoke<Theme | null>("theme") : null;
}

export async function desktopReducedMotion(): Promise<boolean> {
  return inTauri ? invoke<boolean>("reduced_motion") : false;
}

export async function benchMode(): Promise<boolean> {
  return inTauri ? invoke<boolean>("bench_mode") : new URLSearchParams(location.search).has("bench");
}

export async function sendBenchReport(report: unknown): Promise<void> {
  if (inTauri) await invoke("bench_report", { report });
  else console.info("bench report", report);
}

export function onThemeChanged(fn: (t: Theme) => void): void {
  if (inTauri) void listen<Theme>("theme-changed", (e) => fn(e.payload));
}

/** Applies DESIGN.md §5.1 tokens as CSS custom properties. */
export function applyTheme(t: Theme): void {
  const root = document.documentElement;
  for (const [name, value] of Object.entries(t.tokens)) root.style.setProperty(name, value);
  root.dataset.mode = t.mode;
  root.style.setProperty("--font", `"${t.font}", ui-monospace, monospace`);
}
