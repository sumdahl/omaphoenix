<script lang="ts">
  // Migrate step "Preview" (DESIGN.md §4, §7.3): a real dry-run list before
  // anything applies. Sheet enters with spring.sheet; rows stagger 18 ms,
  // capped at 8 (§6.2); drag down to dismiss with velocity hand-off (§6.3):
  // a fast flick dismisses, a slow drag springs back. Esc closes. "Apply" is
  // disabled until the preview has loaded.
  import { Spring, SPRINGS, reducedMotion } from "../lib/spring.svelte";

  export interface Row { category: string; count: number; detail: string }
  let { open, rows, onclose }: { open: boolean; rows: Row[] | null; onclose: () => void } = $props();

  const H = 460;
  const y = new Spring(H, SPRINGS.sheet);
  $effect(() => y.to(open ? 0 : H, SPRINGS.sheet));

  let dragging = false;
  let startY = 0;
  let samples: { t: number; y: number }[] = [];

  function down(e: PointerEvent) {
    dragging = true;
    startY = e.clientY - y.value;
    samples = [{ t: performance.now(), y: y.value }];
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!dragging) return;
    const raw = e.clientY - startY;
    // Resist upward overscroll like a rubber band (DESIGN.md §6.3).
    const v = raw < 0 ? -(1 - 1 / ((-raw * 0.55) / H + 1)) * H : raw;
    y.set(v);
    samples.push({ t: performance.now(), y: v });
    if (samples.length > 6) samples.shift();
  }
  function up() {
    if (!dragging) return;
    dragging = false;
    const a = samples[0];
    const b = samples[samples.length - 1];
    const velocity = a && b && b.t > a.t ? ((b.y - a.y) / (b.t - a.t)) * 1000 : 0;
    if (velocity > 900 || y.value > H * 0.4) onclose();
    else y.fling(velocity, 0, SPRINGS.sheet);
  }
  function key(e: KeyboardEvent) {
    if (open && e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={key} />

{#if open || !y.settled}
  <div class="scrim" style:opacity={Math.max(0, 1 - y.value / H) * 0.5} aria-hidden="true" onclick={onclose}></div>
  <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="preview-title"
    style:transform={`translateY(${y.value}px)`}>
    <div class="grip" role="presentation" onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}></div>
    <h2 id="preview-title">Here's what restoring would change</h2>
    <p class="caption">Nothing changes until you apply. Files that get replaced are kept as .pre-restore.</p>
    {#if rows === null}
      <p class="muted loading">Working out the changes…</p>
    {:else}
      <ul>
        {#each rows as r, i (r.category)}
          <li style:animation-delay={reducedMotion() ? "0ms" : `${Math.min(i, 8) * 18}ms`}>
            <span class="cat">{r.category}</span>
            <span class="num count">{r.count}</span>
            <span class="secondary">{r.detail}</span>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="actions">
      <button onclick={onclose}>Back</button>
      <button class="primary" disabled={rows === null}>Apply {rows ? rows.reduce((n, r) => n + r.count, 0) : ""} changes</button>
    </div>
  </div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; background: var(--surface-deep); }
  .sheet {
    position: fixed; left: 50%; bottom: 0; width: min(720px, 100%); height: 460px; margin-left: min(-360px, -50%);
    background: var(--surface-raised); border-radius: var(--radius-sheet) var(--radius-sheet) 0 0;
    padding: 12px 24px 24px; display: flex; flex-direction: column; gap: 8px; will-change: transform;
  }
  .grip { align-self: center; width: 48px; height: 18px; cursor: grab; touch-action: none; position: relative; }
  .grip::after { content: ""; position: absolute; left: 0; right: 0; top: 7px; height: 4px; border-radius: 2px; background: var(--text-muted); }
  ul { list-style: none; margin: 8px 0 0; padding: 0; flex: 1; overflow: auto; }
  li { display: grid; grid-template-columns: 120px 56px 1fr; gap: 12px; padding: 8px 0; animation: rise 260ms cubic-bezier(.2,.8,.2,1) both; }
  @keyframes rise { from { opacity: 0; transform: translateY(6px); } }
  .cat { font-weight: 600; }
  .count { text-align: right; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
