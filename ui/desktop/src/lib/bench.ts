// Bench mode for the GUI-stack spike (issue #12). Scripted interactions that
// stress DESIGN.md §6: rapid retargets mid-flight, overlapping animations,
// a sheet closed while still opening, fast section switches, and a live
// theme change. It measures what reached the screen:
//   - frame intervals (pacing, dropped frames)
//   - JS work per frame in the spring loop
//   - continuity: per-frame change of rendered values; a teleport (an
//     animation restarting instead of continuing) shows as one huge step
import { onSpringFrame } from "./spring.svelte";
import { sendBenchReport } from "./platform";

interface Api {
  takeSnapshot(): void;
  setScore(v: number): void;
  openPreview(): void;
  closePreview(): void;
  go(s: "pulse" | "migrate"): void;
  themeInfo(): { accent: string; at: number } | null;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function stats(xs: number[]) {
  const s = [...xs].sort((a, b) => a - b);
  const q = (p: number) => s[Math.min(s.length - 1, Math.floor(p * s.length))] ?? 0;
  return { n: s.length, p50: q(0.5), p95: q(0.95), p99: q(0.99), max: s[s.length - 1] ?? 0 };
}

/** Samples a rendered numeric value every frame while `active`. */
function track(read: () => number | null) {
  const values: number[] = [];
  let on = true;
  const tick = () => {
    const v = read();
    if (v !== null && Number.isFinite(v)) values.push(v);
    if (on) requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  return {
    stop() {
      on = false;
      let maxStep = 0;
      for (let i = 1; i < values.length; i++) maxStep = Math.max(maxStep, Math.abs(values[i]! - values[i - 1]!));
      const range = values.length ? Math.max(...values) - Math.min(...values) : 0;
      return { frames: values.length, range, maxStep, maxStepShare: range ? maxStep / range : 0 };
    },
  };
}

const translateY = (sel: string) => () => {
  const el = document.querySelector<HTMLElement>(sel);
  if (!el) return null;
  const m = /translateY\(([-\d.]+)px\)/.exec(el.style.transform);
  return m ? Number(m[1]) : 0;
};

export async function runBench(api: Api): Promise<void> {
  const intervals: number[] = [];
  const work: number[] = [];
  let last = 0;
  let running = true;
  const frame = (t: number) => {
    if (last) intervals.push(t - last);
    last = t;
    if (running) requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
  onSpringFrame((ms) => work.push(ms));

  await sleep(1500);

  // 1. Score ring: two retargets mid-flight.
  const ring = track(() => {
    const c = document.querySelector<SVGCircleElement>(".ring .fill");
    return c ? Number(c.getAttribute("stroke-dashoffset")) : null;
  });
  api.setScore(30);
  await sleep(120);
  api.setScore(95);
  await sleep(150);
  api.setScore(55);
  await sleep(1500);
  const ringCont = ring.stop();

  // 2. Three snapshots 80 ms apart: overlapping drop-ins. Measured on what
  // the eye follows: the on-screen position of an existing row, which must
  // glide down without jumps (the list's own offset jumps by design, FLIP).
  const tl = track(() => document.querySelector('.timeline li[data-id="c4ed652"]')?.getBoundingClientRect().top ?? null);
  for (let i = 0; i < 3; i++) {
    api.takeSnapshot();
    await sleep(80);
  }
  await sleep(1200);
  const tlCont = tl.stop();

  // 3. Sheet: close while still opening, reopen, settle, close.
  api.go("migrate");
  await sleep(300);
  const sheet = track(translateY(".sheet"));
  api.openPreview();
  await sleep(200);
  api.closePreview();
  await sleep(100);
  api.openPreview();
  await sleep(1500);
  api.closePreview();
  await sleep(900);
  const sheetCont = sheet.stop();

  // 4. Fast section switches (layout churn).
  for (let i = 0; i < 8; i++) {
    api.go(i % 2 ? "migrate" : "pulse");
    await sleep(60);
  }
  api.go("pulse");
  await sleep(600);

  // 5. Live theme switch: the harness swaps the theme file; wait for it.
  const before = api.themeInfo();
  const deadline = performance.now() + 15000;
  while (performance.now() < deadline && api.themeInfo() === before) await sleep(100);
  await sleep(400);

  running = false;
  const median = stats(intervals).p50;
  const dropped = intervals.filter((d) => d > median * 1.5).length;
  await sendBenchReport({
    frames: { interval_ms: stats(intervals), dropped, dropped_share: intervals.length ? dropped / intervals.length : 0 },
    spring_work_ms: stats(work),
    continuity: { score_ring: ringCont, timeline: tlCont, sheet: sheetCont },
    theme: api.themeInfo(),
    window: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio },
    ua: navigator.userAgent,
  });
}
