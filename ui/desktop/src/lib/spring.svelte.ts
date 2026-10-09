// Spring physics per DESIGN.md §6.2. Designers think in response (seconds)
// and damping ratio; the solver needs stiffness and damping:
//   stiffness = (2π / response)² · mass,  damping = 4π · ratio · mass / response
// Laws (DESIGN.md §6.1): interruptible — `to()` retargets from the current
// value AND velocity; input is never blocked; reduced motion settles at once.

export interface SpringParams {
  readonly response: number;
  readonly damping: number;
}

export const SPRINGS = {
  snap: { response: 0.2, damping: 1.0 },
  glide: { response: 0.35, damping: 1.0 },
  sheet: { response: 0.42, damping: 0.92 },
  reveal: { response: 0.5, damping: 0.85 },
  ember: { response: 0.7, damping: 0.7 },
} as const satisfies Record<string, SpringParams>;

let reduced = false;
export function setReducedMotion(on: boolean): void {
  reduced = on;
}
export function reducedMotion(): boolean {
  return reduced;
}

const active = new Set<Spring>();
let raf = 0;
let last = 0;

/** Called once per animation frame with the JS work time spent in springs. */
type FrameHook = (workMs: number) => void;
const hooks: FrameHook[] = [];
export function onSpringFrame(hook: FrameHook): void {
  hooks.push(hook);
}

function loop(now: number): void {
  // Clamp: a backgrounded window must not fling springs on return.
  const dt = last ? Math.min((now - last) / 1000, 1 / 20) : 1 / 60;
  last = now;
  const t0 = performance.now();
  for (const s of active) s.step(dt);
  const work = performance.now() - t0;
  for (const h of hooks) h(work);
  if (active.size > 0) {
    raf = requestAnimationFrame(loop);
  } else {
    raf = 0;
    last = 0;
  }
}

const SUBSTEP = 1 / 240; // fixed sub-steps keep stiff springs stable at any frame rate
const REST = 1e-3;

export class Spring {
  value = $state(0);
  velocity = 0;
  target = 0;
  #k = 0;
  #c = 0;

  constructor(initial: number, params: SpringParams = SPRINGS.glide) {
    this.value = initial;
    this.target = initial;
    this.#tune(params);
  }

  #tune(p: SpringParams): void {
    this.#k = (2 * Math.PI / p.response) ** 2;
    this.#c = (4 * Math.PI * p.damping) / p.response;
  }

  /** Retarget. Continues from where the spring is, with the speed it has. */
  to(target: number, params?: SpringParams): void {
    if (params) this.#tune(params);
    this.target = target;
    if (reduced) {
      this.value = target;
      this.velocity = 0;
      active.delete(this);
      return;
    }
    active.add(this);
    if (!raf) raf = requestAnimationFrame(loop);
  }

  /** Jump without animating (initial layout, reduced motion). */
  set(value: number): void {
    this.value = value;
    this.target = value;
    this.velocity = 0;
    active.delete(this);
  }

  /** Hand a gesture's release velocity to the spring (DESIGN.md §6.3). */
  fling(velocity: number, target: number, params?: SpringParams): void {
    this.velocity = velocity;
    this.to(target, params);
  }

  step(dt: number): void {
    const n = Math.max(1, Math.round(dt / SUBSTEP));
    const h = dt / n;
    let x = this.value;
    let v = this.velocity;
    for (let i = 0; i < n; i++) {
      const a = -this.#k * (x - this.target) - this.#c * v;
      v += a * h;
      x += v * h;
    }
    if (Math.abs(x - this.target) < REST && Math.abs(v) < REST) {
      x = this.target;
      v = 0;
      active.delete(this);
    }
    this.velocity = v;
    this.value = x;
  }

  get settled(): boolean {
    return !active.has(this);
  }
}
