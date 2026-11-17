"""Time explore-rust against the Python explore: the median of 9 alternating runs of each, and each one's peak memory.

  python3 -I timing.py [--bin BIN] [--py PY] [--runs N]

PY defaults to scripts/explore. Peak memory comes from one more run of each: the largest
VmHWM of any process in its tree, rg and ast-grep included. A child's ru_maxrss would not do, since it keeps the
forked parent's resident set from before exec; nor would a cgroup's peak, which counts the page cache of every file
read.

MEMORY_CASES search a large tree, once each: the case that caught the Python script holding rg's whole output.
"""
import argparse
import os
import re
import statistics
import subprocess
import time

HERE = os.path.dirname(os.path.abspath(__file__))
T, H, R = "/home/laimk/git/dsrs/taupos-self-interface", "/home/laimk/git/helix-trader", "/tmp/explore-review/r"
CASES = [
    (T, ["hits", "String"]),
    (T, ["hits", "CToInt"]),
    (T, ["hits", "CToInt", "src"]),
    (T, ["hits", "-c", "CToInt"]),
    (T, ["outline", "src/it/studiors/tauself/interfaces/UtilityPacchettoZip.java"]),
    (H, ["hits", "RetrainActor"]),
    (H, ["hits", "ZZZ_NO_MATCH_XYZ"]),
    (R, ["hits", "NEEDLE_"]),
    (T, ["hits", "--json", "-w", "CToInt"]),
    (T, ["hits", "--json", "CToInt", "src"]),
    (H, ["hits", "--json", "RetrainActor"]),
]
MEMORY_CASES = [(H, ["hits", "-l", "-uu", ""])]


def once(cmd, cwd):
    """Seconds of one run."""
    start = time.perf_counter()
    subprocess.run(cmd, cwd=cwd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return time.perf_counter() - start


def tree(pid):
    """PID and its descendants."""
    out, todo = [], [pid]
    while todo:
        p = todo.pop()
        out.append(p)
        try:
            for task in os.listdir(f"/proc/{p}/task"):
                with open(f"/proc/{p}/task/{task}/children") as f:
                    todo += [int(c) for c in f.read().split()]
        except OSError:
            pass
    return out


def peak_mb(cmd, cwd):
    """The largest resident high-water mark (VmHWM) of any process in one run's tree, in MB, and its seconds; read
    every 10 ms until the run ends, so a peak in a process's last 10 ms can be missed."""
    start = time.perf_counter()
    p = subprocess.Popen(cmd, cwd=cwd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    peak = 0
    while p.poll() is None:
        for pid in tree(p.pid):
            try:
                with open(f"/proc/{pid}/status") as f:
                    m = re.search(r"VmHWM:\s+(\d+) kB", f.read())
                peak = max(peak, int(m.group(1)) if m else 0)
            except OSError:
                pass
        time.sleep(0.01)
    return peak / 1024, time.perf_counter() - start


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=os.path.join(HERE, "../target/release/explore-rust"))
    ap.add_argument("--py", default=os.path.normpath(os.path.join(HERE, "../../scripts/explore")))
    ap.add_argument("--runs", type=int, default=9)
    a = ap.parse_args()
    tools = {"python": ["python3", "-I", a.py], "rust": [os.path.abspath(a.bin)]}
    print(f"{'case':44} {'python ms':>9} {'rust ms':>8} {'speedup':>7} {'python MB':>9} {'rust MB':>8}")
    where = {T: "taupos", H: "helix", R: "fixture"}
    for cwd, args in CASES:
        times = {k: [] for k in tools}
        for _ in range(a.runs):
            for k, cmd in tools.items():
                times[k].append(once(cmd + args, cwd))
        py, rs = (statistics.median(times[k]) * 1000 for k in tools)
        mem = {k: peak_mb(cmd + args, cwd)[0] for k, cmd in tools.items()}
        print(f"{where[cwd] + ' ' + ' '.join(args):44.44} {py:9.0f} {rs:8.0f} {py / rs:6.1f}x "
              f"{mem['python']:9.1f} {mem['rust']:8.1f}")
    for cwd, args in MEMORY_CASES:
        (py_mb, py_s), (rs_mb, rs_s) = (peak_mb(tools[k] + args, cwd) for k in ("python", "rust"))
        print(f"{where[cwd] + ' ' + ' '.join(args):44.44} {py_s * 1000:9.0f} {rs_s * 1000:8.0f} {py_s / rs_s:6.1f}x "
              f"{py_mb:9.1f} {rs_mb:8.1f}")


main()
