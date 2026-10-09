<script lang="ts">
  // DESIGN.md §6.4 "Audit score": the ring fills from its previous value with
  // spring.reveal; the number rolls; a grade change cross-fades only after
  // the ring settles. Severity colour sits on the ring only (§5.1).
  import { Spring, SPRINGS } from "../lib/spring.svelte";

  let { score }: { score: number } = $props();
  const ring = new Spring(0, SPRINGS.reveal);
  $effect(() => ring.to(score, SPRINGS.reveal));

  const grade = (s: number) => (s >= 90 ? "A" : s >= 80 ? "B" : s >= 70 ? "C" : s >= 60 ? "D" : "F");
  const tone = (s: number) => (s >= 80 ? "var(--state-fine)" : s >= 60 ? "var(--state-attention)" : "var(--state-risk)");
  const R = 52;
  const C = 2 * Math.PI * R;
  const shown = $derived(Math.max(0, Math.min(100, ring.value)));
  const settledGrade = $derived(ring.settled ? grade(score) : grade(Math.round(shown)));
</script>

<div class="ring" role="progressbar" aria-label="Audit score" aria-valuemin="0" aria-valuemax="100"
  aria-valuenow={score} aria-valuetext={`${score} out of 100, grade ${grade(score)}`}>
  <svg viewBox="0 0 120 120" aria-hidden="true">
    <circle cx="60" cy="60" r={R} class="track" />
    <circle cx="60" cy="60" r={R} class="fill" stroke={tone(shown)}
      stroke-dasharray={C} stroke-dashoffset={C * (1 - shown / 100)} />
  </svg>
  <div class="label" aria-hidden="true">
    <span class="score num">{Math.round(shown)}</span>
    {#key settledGrade}<span class="grade">{settledGrade}</span>{/key}
  </div>
</div>

<style>
  .ring { position: relative; width: 132px; height: 132px; }
  svg { width: 100%; height: 100%; transform: rotate(-90deg); }
  circle { fill: none; stroke-width: 9; }
  .track { stroke: var(--surface-sunken); }
  .fill { stroke-linecap: round; }
  .label { position: absolute; inset: 0; display: grid; place-content: center; text-align: center; }
  .score { font-size: 40px; font-weight: 600; line-height: 1; }
  .grade { font-size: 13px; color: var(--text-secondary); animation: fade 180ms ease-out; }
  @keyframes fade { from { opacity: 0; } }
</style>
