<script lang="ts">
  // DESIGN.md §6.4 "Snapshot taken": the newest dot drops in from above with
  // spring.sheet; older entries slide down one slot; one soft accent pulse
  // runs along the rail (opacity only, once). Interruptible: rapid snapshots
  // retarget the springs instead of queueing animations.
  import { Spring, SPRINGS, reducedMotion } from "../lib/spring.svelte";

  export interface Snap { id: string; when: string; note: string }
  let { snaps }: { snaps: Snap[] } = $props();

  const ROW = 44;
  const drop = new Spring(0, SPRINGS.sheet);
  let pulse = $state(0);
  let seen = 0;
  $effect(() => {
    if (snaps.length > seen && seen > 0) {
      drop.set(drop.value - ROW); // start one row up, from wherever it is now
      drop.to(0, SPRINGS.sheet);
      if (!reducedMotion()) pulse++;
    }
    seen = snaps.length;
  });
</script>

<section class="timeline" aria-label="Snapshot timeline">
  {#key pulse}<div class="rail" class:pulse={pulse > 0} aria-hidden="true"></div>{/key}
  <ol style:transform={`translateY(${drop.value}px)`}>
    {#each snaps as s, i (s.id)}
      <li data-id={s.id} style:opacity={i === 0 ? Math.max(0, 1 + drop.value / ROW) : 1}>
        <span class="dot" class:now={i === 0} aria-hidden="true"></span>
        <span class="when num">{s.when}</span>
        <span class="note secondary">{s.note}</span>
      </li>
    {/each}
  </ol>
</section>

<style>
  .timeline { position: relative; overflow: hidden; height: 220px; }
  .rail { position: absolute; left: 11px; top: 0; bottom: 0; width: 2px; background: var(--surface-sunken); }
  .rail.pulse { animation: pulse 600ms ease-out 1; }
  @keyframes pulse { 0% { background: var(--accent); } 100% { background: var(--surface-sunken); } }
  ol { list-style: none; margin: 0; padding: 0; will-change: transform; }
  li { display: flex; align-items: center; gap: 12px; height: 44px; }
  .dot { width: 10px; height: 10px; margin-left: 7px; border-radius: 50%; background: var(--text-muted); position: relative; }
  .dot.now { background: var(--accent); }
  .when { width: 128px; }
</style>
