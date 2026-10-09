#!/usr/bin/env python3
"""Floor test: idle PSS (all processes) and AT-SPI visibility of a GUI binary."""
import subprocess, sys, time
import gi
gi.require_version("Atspi", "2.0")
from gi.repository import Atspi

binary, label = sys.argv[1], sys.argv[2]
app = subprocess.Popen([binary], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(6)

tree, todo = [], [app.pid]
while todo:
    p = todo.pop()
    tree.append(p)
    todo += [int(x) for x in subprocess.run(["pgrep", "-P", str(p)], capture_output=True, text=True).stdout.split()]
pss = 0
for p in tree:
    try:
        pss += next(int(l.split()[1]) for l in open(f"/proc/{p}/smaps_rollup") if l.startswith("Pss:"))
    except (OSError, StopIteration):
        pass

def walk(node, out, depth=0):
    if node is None or depth > 30:
        return
    try:
        out.append((node.get_role_name(), (node.get_name() or "").strip()))
        for i in range(node.get_child_count()):
            walk(node.get_child_at_index(i), out, depth + 1)
    except Exception:
        pass

found, nodes = None, []
desk = Atspi.get_desktop(0)
for i in range(desk.get_child_count()):
    a = desk.get_child_at_index(i)
    try:
        pid = a.get_process_id()
    except Exception:
        continue
    if pid in tree:
        found = a
        walk(a, nodes)
button = any("button" in r and "snapshot" in n.lower() for r, n in nodes)
text = any("protected" in n.lower() for _, n in nodes)
print(f"{label:6} idle PSS {pss/1024:6.1f} MB | on AT-SPI bus: {'yes' if found else 'NO '} | button named: {'yes' if button else 'no '} | heading text: {'yes' if text else 'no '} | {len(nodes)} nodes")
app.terminate()
try:
    app.wait(timeout=5)
except subprocess.TimeoutExpired:
    app.kill()
