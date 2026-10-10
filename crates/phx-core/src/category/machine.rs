use std::path::Path;

/// The firmware (DMI) machine model, e.g. "Dell Inc. Latitude 7430"; `OMAPHOENIX_HARDWARE` overrides it.
#[must_use]
pub fn hardware() -> String {
    if let Ok(h) = std::env::var("OMAPHOENIX_HARDWARE") {
        return h;
    }
    ["sys_vendor", "product_name", "product_version"]
        .iter()
        .filter_map(|f| std::fs::read_to_string(Path::new("/sys/class/dmi/id").join(f)).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty() && !placeholder_dmi(s))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Firmware fills unset DMI fields with filler text.
fn placeholder_dmi(s: &str) -> bool {
    let l = s.to_lowercase();
    l.contains("to be filled") || l == "default string" || l == "system product name" || l == "none"
}

/// Items that only make sense on their own hardware; `machine:` lines in `selection.conf` add more.
pub const MACHINE_SPECIFIC: &[(&str, &[&str])] = &[
    ("config", &[".config/hypr/monitors.*"]),
    (
        "local",
        &[
            "state/omarchy/toggles/hypr/internal-monitor-scale",
            "state/omarchy/toggles/hypr/touchpad-*",
            "state/omarchy/toggles/hypr/*touchpads*",
        ],
    ),
    ("etc", &["/etc/systemd/system/battery-threshold.service"]),
    (
        "service",
        &[
            "system/battery-threshold.service",
            "system/intel_lpmd.service",
            "system/thermald.service",
        ],
    ),
];
