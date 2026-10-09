<script lang="ts">
  import { onMount } from "svelte";
  import ScoreRing from "./components/ScoreRing.svelte";
  import Timeline, { type Snap } from "./components/Timeline.svelte";
  import PreviewSheet, { type Row } from "./components/PreviewSheet.svelte";
  import { Spring, SPRINGS, setReducedMotion } from "./lib/spring.svelte";
  import { applyTheme, benchMode, desktopReducedMotion, loadTheme, onThemeChanged, type Theme } from "./lib/platform";
  import { runBench } from "./lib/bench";

  // Spike data: the real values come from the Rust core later.
  let section = $state<"pulse" | "migrate">("pulse");
  let score = $state(72);
  let snaps = $state<Snap[]>([
    { id: "c4ed652", when: "14:32 · 2h ago", note: "+2 plugins, theme → blade-runner-2049" },
    { id: "8a9416f", when: "09:10 · 7h ago", note: "hypr bindings changed" },
    { id: "10b9447", when: "yesterday", note: "+ollama, +flea-bin" },
  ]);
  let sheetOpen = $state(false);
  let rows = $state<Row[] | null>(null);
  let announce = $state("");
  let lastTheme = $state<{ accent: string; at: number } | null>(null);

  // A focus ring that moves between controls with spring.snap (§7.1).
  const fx = new Spring(0, SPRINGS.snap), fy = new Spring(0, SPRINGS.snap);
  const fw = new Spring(0, SPRINGS.snap), fh = new Spring(0, SPRINGS.snap);
  let focusVisible = $state(false);
  function follow(e: FocusEvent) {
    const el = e.target as HTMLElement;
    if (!el.matches("button, [tabindex]")) return;
    const r = el.getBoundingClientRect();
    const first = !focusVisible;
    focusVisible = true;
    for (const [s, v] of [[fx, r.left - 3], [fy, r.top - 3], [fw, r.width + 6], [fh, r.height + 6]] as const) {
      first ? s.set(v) : s.to(v, SPRINGS.snap);
    }
  }

  let counter = 0;
  export function takeSnapshot() {
    counter++;
    const id = `new${counter}`;
    snaps = [{ id, when: "just now", note: counter === 1 ? "Nothing changed since 14:32" : `${counter} files changed` }, ...snaps].slice(0, 6);
    announce = "Snapshot taken.";
  }
  export function setScore(v: number) { score = v; }
  export function openPreview() {
    section = "migrate";
    sheetOpen = true;
    rows = null;
    setTimeout(() => {
      rows = [
        { category: "packages", count: 67, detail: "install from the repos and AUR" },
        { category: "plugins", count: 41, detail: "14 from git, 27 copied" },
        { category: "themes", count: 57, detail: "active: blade-runner-2049" },
        { category: "config", count: 1430, detail: "files; 63 replaced, kept as .pre-restore" },
        { category: "/etc", count: 7, detail: "needs your password" },
        { category: "services", count: 11, detail: "enabled, not started" },
        { category: "skipped", count: 3, detail: "monitor layout, touchpad: other hardware" },
      ];
    }, 350);
  }
  export function closePreview() { sheetOpen = false; }
  export function go(s: "pulse" | "migrate") { section = s; }
  export function themeInfo() { return lastTheme; }

  function key(e: KeyboardEvent) {
    if (sheetOpen) return;
    if (e.key === "1") go("pulse");
    else if (e.key === "2") go("migrate");
    else if (e.key === "s" && section === "pulse") takeSnapshot();
  }

  onMount(async () => {
    const t = await loadTheme();
    if (t) applyTheme(t);
    setReducedMotion((await desktopReducedMotion()) || matchMedia("(prefers-reduced-motion: reduce)").matches);
    onThemeChanged((nt: Theme) => {
      document.documentElement.classList.add("theme-xfade");
      applyTheme(nt);
      lastTheme = { accent: getComputedStyle(document.documentElement).getPropertyValue("--accent").trim(), at: performance.now() };
      setTimeout(() => document.documentElement.classList.remove("theme-xfade"), 220);
    });
    if (await benchMode()) void runBench({ takeSnapshot, setScore, openPreview, closePreview, go, themeInfo });
  });
</script>

<svelte:window onkeydown={key} onfocusin={follow} />

<div class="shell">
  <nav aria-label="Sections">
    <div class="brand"><span class="ember" aria-hidden="true"></span>OmaPhoenix</div>
    <button class:on={section === "pulse"} aria-current={section === "pulse" ? "page" : undefined} aria-keyshortcuts="1" onclick={() => go("pulse")}>
      <span class="k num" aria-hidden="true">1</span> Pulse
    </button>
    <button class:on={section === "migrate"} aria-current={section === "migrate" ? "page" : undefined} aria-keyshortcuts="2" onclick={() => go("migrate")}>
      <span class="k num" aria-hidden="true">2</span> Migrate
    </button>
  </nav>

  <main>
    {#if section === "pulse"}
      <header><h1>You're protected.</h1><p class="secondary">Last snapshot 2h ago. Automatic snapshots every 30 min.</p></header>
      <div class="grid">
        <section class="card" aria-labelledby="health-h">
          <h2 id="health-h">Health</h2>
          <div class="health"><ScoreRing {score} />
            <ul class="facts">
              <li><span class="marker attention"></span> 2 worth a look</li>
              <li><span class="marker fine"></span> 38 fine</li>
              <li><span class="marker info"></span> 4 notes</li>
            </ul>
          </div>
        </section>
        <section class="card" aria-labelledby="tl-h">
          <h2 id="tl-h">Timeline</h2>
          <Timeline {snaps} />
          <button class="primary" aria-keyshortcuts="s" onclick={takeSnapshot}>Take snapshot <span class="k num" aria-hidden="true">s</span></button>
        </section>
      </div>
    {:else}
      <header><h1>Migrate</h1><p class="secondary">Step 3 of 5 · Preview what this machine will get.</p></header>
      <section class="card">
        <p>Snapshot <span class="num">c4ed652</span> from a Dell Inc. Latitude 7430, taken 2h ago.</p>
        <button class="primary" onclick={openPreview}>Preview changes</button>
      </section>
    {/if}
  </main>
</div>

<PreviewSheet open={sheetOpen} {rows} onclose={closePreview} />

{#if focusVisible}
  <div class="focus" aria-hidden="true"
    style:transform={`translate(${fx.value}px, ${fy.value}px)`} style:width={`${fw.value}px`} style:height={`${fh.value}px`}></div>
{/if}
<div class="sr-only" role="status" aria-live="polite">{announce}</div>

<style>
  .shell { display: grid; grid-template-columns: 200px 1fr; height: 100%; }
  nav { background: var(--surface-sunken); padding: 16px 12px; display: flex; flex-direction: column; gap: 4px; }
  nav button { text-align: left; background: transparent; display: flex; gap: 10px; align-items: center; }
  nav button.on { background: var(--selection); }
  .brand { font-weight: 600; font-size: 15px; padding: 4px 8px 16px; display: flex; gap: 8px; align-items: center; }
  .ember { width: 10px; height: 14px; border-radius: 50% 50% 50% 50% / 60% 60% 40% 40%; background: var(--accent); }
  .k { font-size: 12px; color: var(--text-muted); border: 1px solid color-mix(in srgb, var(--text-muted) 40%, transparent); border-radius: 4px; padding: 0 5px; }
  main { padding: 32px; overflow: auto; display: flex; flex-direction: column; gap: 24px; }
  header { display: flex; flex-direction: column; gap: 6px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 24px; }
  .card { display: flex; flex-direction: column; gap: 12px; }
  .health { display: flex; gap: 24px; align-items: center; }
  .facts { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  .facts li { display: flex; gap: 8px; align-items: center; }
  .focus { position: fixed; left: 0; top: 0; border: 2px solid var(--accent); border-radius: 8px; pointer-events: none; }
  .sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
  :global(.theme-xfade *) { transition: background-color 200ms, color 200ms, border-color 200ms, stroke 200ms; }
</style>
