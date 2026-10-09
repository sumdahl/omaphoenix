#!/usr/bin/env python3
"""Idle memory of the desktop app, per process, as PSS (shared pages split
fairly between processes). Usage: spike-mem.py [ENV=VALUE ...] to try knobs."""
import os, subprocess, sys, time
from pathlib import Path

binary = Path(__file__).resolve().parent.parent / "target/release/omaphoenix-desktop"
env = dict(os.environ, **dict(a.split("=", 1) for a in sys.argv[1:]))
app = subprocess.Popen([str(binary)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(6)

def children(pid):
    out = subprocess.run(["pgrep", "-P", str(pid)], capture_output=True, text=True).stdout.split()
    return [int(p) for p in out]

tree, todo = [], [app.pid]
while todo:
    p = todo.pop()
    tree.append(p)
    todo += children(p)
total = 0
for p in tree:
    try:
        pss = next(int(l.split()[1]) for l in open(f"/proc/{p}/smaps_rollup") if l.startswith("Pss:"))
        name = open(f"/proc/{p}/comm").read().strip()
    except (OSError, StopIteration):
        continue
    total += pss
    print(f"  {name:18} {pss/1024:7.1f} MB")
print(f"  {'TOTAL':18} {total/1024:7.1f} MB")
app.terminate()
app.wait(timeout=10)
