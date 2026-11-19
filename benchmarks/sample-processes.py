#!/usr/bin/env python3
"""Sample an Elaris process and its descendants using macOS/Linux ps."""

import argparse
import csv
import math
import subprocess
import sys
import time


def snapshot():
    output = subprocess.check_output(
        ["ps", "-ax", "-o", "pid=,ppid=,rss=,pcpu=,comm="], text=True
    )
    processes = {}
    children = {}
    for line in output.splitlines():
        pid, parent, rss, cpu, command = line.split(maxsplit=4)
        processes[int(pid)] = (int(rss), cpu, command)
        children.setdefault(int(parent), []).append(int(pid))
    return processes, children


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid", type=int, help="Elaris PID, excluding the launcher")
    parser.add_argument("scenario", help="label for this measurement window")
    parser.add_argument("--samples", type=int, default=10)
    parser.add_argument("--interval", type=float, default=1)
    args = parser.parse_args()
    if args.samples < 1 or not math.isfinite(args.interval) or args.interval <= 0:
        parser.error("samples and interval must be positive")

    writer = csv.writer(sys.stdout)
    writer.writerow(["epoch", "scenario", "role", "pid", "rss_kib", "ps_cpu_percent", "command"])
    for sample in range(args.samples):
        if sample:
            time.sleep(args.interval)
        stamp = f"{time.time():.3f}"
        processes, children = snapshot()
        if args.pid not in processes:
            parser.exit(1, f"Elaris process {args.pid} exited or was not found\n")
        pending = [args.pid]
        while pending:
            pid = pending.pop()
            role = "elaris" if pid == args.pid else "descendant"
            writer.writerow([stamp, args.scenario, role, pid, *processes[pid]])
            pending.extend(children.get(pid, ()))
        sys.stdout.flush()


if __name__ == "__main__":
    main()
