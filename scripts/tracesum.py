#!/usr/bin/env python3
"""tracesum.py - rank a Bevy `trace_chrome` capture by where the frame went.

Reads the `trace-<ts>.json` that a `--features trace_chrome` build writes next to the
working directory. The file is a Chrome-trace array of B/E (begin/end) events, one event per
line, and it is often TRUNCATED (no closing `]`) if the game was killed instead of closed
cleanly - so this parses line by line and never needs the whole file in memory.

    python scripts/tracesum.py trace-1234.json
    python scripts/tracesum.py trace-1234.json --top 40 --frames 1800
    python scripts/tracesum.py trace-1234.json --skip-seconds 3   # drop warm-up

What it prints:
  1. Per-THREAD busy time per frame  -> the answer to "how many cores are we really using".
  2. Top spans by SELF time per frame (self = span time minus its child spans), which is the
     number that adds up to a thread's busy time and points at the actual hot code.
  3. Top `system:` spans by INCLUSIVE time per frame.

All per-frame numbers are divided by the frame count. Frames are counted from the `frame`
span if the trace has one, else from the `schedule: name=First` span; override with
`--frames N`.

Caveat, and it matters: tracing itself costs time per span, so many tiny spans are inflated.
Trust the RANKING and big gaps, not absolute milliseconds. Compare against the journal's
`main_ms` (perf build, no tracing) for the real scale.
"""
import argparse
import json
import sys
from collections import defaultdict


def parse_events(path):
    """Yield event dicts from a possibly-truncated chrome trace, tolerant of junk lines."""
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            s = line.strip().strip(",")
            if not s or s in ("[", "]"):
                continue
            if s.startswith("["):
                s = s[1:].strip().strip(",")
            if s.endswith("]"):
                s = s[:-1].strip().strip(",")
            if not s.startswith("{"):
                continue
            try:
                yield json.loads(s)
            except json.JSONDecodeError:
                continue  # truncated final line


def short(name, limit=110):
    name = name.replace("\\\"", "\"")
    return name if len(name) <= limit else name[: limit - 1] + "…"


def thread_group(tname):
    t = (tname or "").lower()
    if "main" in t or t == "":
        return "main"
    if "render" in t:
        return "render"
    if "compute" in t or "task" in t or "worker" in t:
        return "compute-pool"
    if "io" in t:
        return "io-pool"
    return tname


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("trace")
    ap.add_argument("--top", type=int, default=30)
    ap.add_argument("--frames", type=int, default=0, help="override the frame count")
    ap.add_argument("--skip-seconds", type=float, default=0.0, help="ignore the first N seconds (warm-up)")
    args = ap.parse_args()

    thread_names = {}
    stacks = defaultdict(list)          # tid -> [(name, ts, child_time)]
    incl = defaultdict(float)           # name -> inclusive us
    selft = defaultdict(float)          # name -> self us
    count = defaultdict(int)
    worst = defaultdict(float)
    per_thread_self = defaultdict(float)    # tid -> total self us (== busy time)
    frame_spans = 0
    first_schedule = 0
    t0 = None
    ev_count = 0

    for ev in parse_events(args.trace):
        ph = ev.get("ph")
        if ph == "M":
            if ev.get("name") == "thread_name":
                thread_names[ev.get("tid")] = (ev.get("args") or {}).get("name", "")
            continue
        if ph not in ("B", "E", "X"):
            continue
        ts = ev.get("ts")
        if ts is None:
            continue
        if t0 is None:
            t0 = ts
        if args.skip_seconds and (ts - t0) < args.skip_seconds * 1e6:
            # still maintain stacks so E events pair up, but do not account
            if ph == "B":
                stacks[ev["tid"]].append((ev.get("name", "?"), ts, 0.0, False))
            elif ph == "E" and stacks[ev["tid"]]:
                stacks[ev["tid"]].pop()
            continue
        ev_count += 1
        tid = ev.get("tid")
        if ph == "B":
            stacks[tid].append((ev.get("name", "?"), ts, 0.0, True))
        elif ph == "E":
            st = stacks[tid]
            if not st:
                continue
            name, start, child, counted = st.pop()
            dur = ts - start
            if st:
                pn, ps, pc, pcounted = st[-1]
                st[-1] = (pn, ps, pc + dur, pcounted)
            if not counted:
                continue
            incl[name] += dur
            selft[name] += max(dur - child, 0.0)
            per_thread_self[tid] += max(dur - child, 0.0)
            count[name] += 1
            worst[name] = max(worst[name], dur)
            if name == "frame":
                frame_spans += 1
            if name.startswith("schedule") and name.rstrip().endswith("name=First"):
                first_schedule += 1
        else:  # complete event
            dur = ev.get("dur", 0.0)
            name = ev.get("name", "?")
            incl[name] += dur
            selft[name] += dur
            per_thread_self[tid] += dur
            count[name] += 1
            worst[name] = max(worst[name], dur)

    if not incl:
        sys.exit("no B/E events found - is this a trace_chrome capture?")

    frames = args.frames or frame_spans or first_schedule
    if not frames:
        frames = 1
        print("warning: could not count frames; per-frame columns are TOTALS. Pass --frames N.\n")
    else:
        src = "--frames" if args.frames else ("`frame` spans" if frame_spans else "`schedule: name=First` spans")
        print(f"frames counted: {frames}  (from {src}); events read: {ev_count}\n")

    def per_frame(us):
        return us / frames / 1000.0

    # 1. per-thread busy
    groups = defaultdict(lambda: [0, 0.0])
    print("== THREAD BUSY TIME per frame (sum of self time; ~ cores worth of work)")
    print(f"{'thread':<34}{'ms/frame':>10}")
    rows = []
    for tid, us in per_thread_self.items():
        tn = thread_names.get(tid, f"tid {tid}")
        g = thread_group(thread_names.get(tid, ""))
        groups[g][0] += 1
        groups[g][1] += us
        rows.append((us, tn))
    for us, tn in sorted(rows, reverse=True)[:12]:
        print(f"{short(tn, 33):<34}{per_frame(us):>10.2f}")
    print("\n  by group:")
    for g, (n, us) in sorted(groups.items(), key=lambda kv: -kv[1][1]):
        print(f"  {g:<20} threads={n:<3} {per_frame(us):>8.2f} ms/frame")
    total = sum(per_thread_self.values())
    print(f"  {'ALL':<20} {'':<11}{per_frame(total):>8.2f} ms/frame (traced; inflated by tracing)\n")

    # 2. self time
    print(f"== TOP {args.top} BY SELF TIME per frame")
    print(f"{'self ms/f':>10}{'incl ms/f':>11}{'calls/f':>9}{'worst ms':>10}  span")
    for name, us in sorted(selft.items(), key=lambda kv: -kv[1])[: args.top]:
        print(f"{per_frame(us):>10.3f}{per_frame(incl[name]):>11.3f}{count[name] / frames:>9.1f}{worst[name] / 1000.0:>10.2f}  {short(name)}")

    # 3. systems, inclusive
    print(f"\n== TOP {args.top} `system:` SPANS BY INCLUSIVE TIME per frame")
    print(f"{'incl ms/f':>10}{'self ms/f':>11}{'calls/f':>9}  system")
    systems = [(n, u) for n, u in incl.items() if n.startswith("system")]
    for name, us in sorted(systems, key=lambda kv: -kv[1])[: args.top]:
        print(f"{per_frame(us):>10.3f}{per_frame(selft[name]):>11.3f}{count[name] / frames:>9.1f}  {short(name)}")


if __name__ == "__main__":
    main()
