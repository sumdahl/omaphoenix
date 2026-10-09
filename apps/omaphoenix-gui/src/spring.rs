//! Spring physics per DESIGN.md §6.2, the same model as the Tauri spike's
//! TypeScript engine so the two spikes are compared on equal terms.
//!
//! Designers give response (seconds) and damping ratio; the solver uses
//! stiffness = (2π/response)² and damping = 4π·ratio/response (mass 1).
//! `to()` retargets from the current value *and* velocity (interruptible);
//! reduced motion settles at once.

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub response: f32,
    pub damping: f32,
}

#[allow(
    dead_code,
    reason = "full DESIGN.md token set; not every screen uses every spring"
)]
pub const SNAP: Params = Params {
    response: 0.20,
    damping: 1.0,
};
#[allow(
    dead_code,
    reason = "full DESIGN.md token set; not every screen uses every spring"
)]
pub const GLIDE: Params = Params {
    response: 0.35,
    damping: 1.0,
};
pub const SHEET: Params = Params {
    response: 0.42,
    damping: 0.92,
};

const SUBSTEP: f32 = 1.0 / 240.0;
const REST: f32 = 1e-3;

#[derive(Clone, Debug)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    k: f32,
    c: f32,
    moving: bool,
}

impl Spring {
    pub fn new(value: f32, p: Params) -> Self {
        let mut s = Self {
            value,
            velocity: 0.0,
            target: value,
            k: 0.0,
            c: 0.0,
            moving: false,
        };
        s.tune(p);
        s
    }

    fn tune(&mut self, p: Params) {
        self.k = (std::f32::consts::TAU / p.response).powi(2);
        self.c = 4.0 * std::f32::consts::PI * p.damping / p.response;
    }

    /// Retarget, continuing from the current value and velocity.
    pub fn to(&mut self, target: f32, p: Params, reduced: bool) {
        self.tune(p);
        self.target = target;
        if reduced {
            self.set(target);
        } else {
            self.moving = true;
        }
    }

    /// Jump without animating.
    pub fn set(&mut self, value: f32) {
        self.value = value;
        self.target = value;
        self.velocity = 0.0;
        self.moving = false;
    }

    /// Release a drag with its velocity (DESIGN.md §6.3).
    pub fn fling(&mut self, velocity: f32, target: f32, p: Params, reduced: bool) {
        self.velocity = velocity;
        self.to(target, p, reduced);
    }

    pub fn moving(&self) -> bool {
        self.moving
    }

    /// Advances by `dt` seconds with fixed sub-steps (stable at any frame rate).
    pub fn step(&mut self, dt: f32) {
        if !self.moving {
            return;
        }
        // dt is clamped to 50 ms by the caller: a handful of sub-steps.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "0..=12 sub-steps"
        )]
        let (n, h) = {
            let n = ((dt / SUBSTEP).round() as u32).max(1);
            (n, dt / n as f32)
        };
        for _ in 0..n {
            let a = -self.k * (self.value - self.target) - self.c * self.velocity;
            self.velocity += a * h;
            self.value += self.velocity * h;
        }
        if (self.value - self.target).abs() < REST && self.velocity.abs() < REST {
            self.set(self.target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retarget_keeps_velocity_no_jump() {
        let mut s = Spring::new(0.0, REVEAL);
        s.to(100.0, REVEAL, false);
        for _ in 0..6 {
            s.step(1.0 / 60.0);
        }
        let (v, vel) = (s.value, s.velocity);
        s.to(10.0, REVEAL, false);
        assert_eq!(s.value, v, "retarget must not move the value");
        assert_eq!(s.velocity, vel, "retarget must keep the velocity");
        let before = s.value;
        s.step(1.0 / 60.0);
        assert!(
            (s.value - before).abs() < 10.0,
            "no teleport after retarget"
        );
    }

    #[test]
    fn settles_and_stops() {
        let mut s = Spring::new(0.0, SNAP);
        s.to(1.0, SNAP, false);
        for _ in 0..300 {
            s.step(1.0 / 60.0);
        }
        assert!(!s.moving());
        assert_eq!(s.value, 1.0);
    }
}
