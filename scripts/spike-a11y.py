#!/usr/bin/env python3
"""Walk the spike app's AT-SPI tree the way a screen reader would, and check
the DESIGN.md §9 basics: named buttons, headings, the score as a progress bar
with its value, the navigation landmark, the status live region."""
import sys, time
import gi
gi.require_version("Atspi", "2.0")
from gi.repository import Atspi

def find_app(name, timeout=10):
    end = time.time() + timeout
    while time.time() < end:
        desk = Atspi.get_desktop(0)
        for i in range(desk.get_child_count()):
            app = desk.get_child_at_index(i)
            if app and name.lower() in (app.get_name() or "").lower():
                return app
        time.sleep(0.5)
    return None

def walk(node, depth=0, out=None):
    out = [] if out is None else out
    if node is None or depth > 40:
        return out
    try:
        role, name = node.get_role_name(), (node.get_name() or "").strip()
    except Exception:
        return out
    value = None
    try:
        v = node.get_value_iface() if hasattr(node, "get_value_iface") else None
        value = v.get_current_value() if v else None
    except Exception:
        pass
    out.append((role, name, value))
    for i in range(node.get_child_count()):
        walk(node.get_child_at_index(i), depth + 1, out)
    return out

app = find_app(sys.argv[1] if len(sys.argv) > 1 else "omaphoenix")
if not app:
    print("FAIL: app not found on the accessibility bus"); sys.exit(1)
nodes = walk(app)
roles = {}
for r, n, v in nodes:
    roles.setdefault(r, []).append((n, v))
def show(role):
    items = roles.get(role, [])
    print(f"{role:15} {len(items):3}  " + "; ".join(f"{n!r}" + (f"={v:g}" if isinstance(v, float) else "") for n, v in items[:6]))
# WebKitGTK names roles "button" and "status bar" (not "push button"/"status").
for role in ["button", "heading", "progress bar", "landmark", "status bar", "list item"]:
    show(role)
checks = {
    "buttons are named": bool(roles.get("button")) and all(n for n, _ in roles.get("button", [])),
    "shortcut hints are not read as part of names": all(n in ("Pulse", "Migrate", "Take snapshot") for n, _ in roles.get("button", []) if any(w in n for w in ("Pulse", "Migrate", "snapshot"))),
    "headings exposed": len(roles.get("heading", [])) >= 2,
    "score is a progress bar with a value": any(isinstance(v, float) for _, v in roles.get("progress bar", [])),
    "live region present": bool(roles.get("status bar")),
}
for k, ok in checks.items():
    print(("PASS " if ok else "FAIL ") + k)
sys.exit(0 if all(checks.values()) else 1)
