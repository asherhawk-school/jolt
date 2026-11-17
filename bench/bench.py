#!/usr/bin/env python3
"""Time jolt against javac on the test programs and on a large generated project.

usage: bench/bench.py [--quick]
"""
import glob
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
JOLT = os.path.join(ROOT, "target", "release", "jolt")
QUICK = "--quick" in sys.argv


def timeit(cmd, runs):
    times = []
    for _ in range(runs):
        t = time.perf_counter()
        subprocess.run(cmd, check=True, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        times.append(time.perf_counter() - t)
    return statistics.median(times)


def run_java(cp, main):
    stdin_file = os.path.join(ROOT, "tests", "programs", main + ".in")
    with open(stdin_file if os.path.exists(stdin_file) else os.devnull) as stdin:
        return subprocess.run(["java", "-cp", cp, main], stdin=stdin, capture_output=True, text=True).stdout


def fmt(s):
    return f"{s * 1000:9.1f} ms" if s >= 0.001 else f"{s * 1e6:7.0f} µs"


def compare(label, files, runs_jolt, runs_javac, main=None):
    tmp = tempfile.mkdtemp()
    try:
        dj, dc = os.path.join(tmp, "jolt"), os.path.join(tmp, "javac")
        os.makedirs(dj)
        os.makedirs(dc)
        tj = timeit([JOLT, "-d", dj] + files, runs_jolt)
        tc = timeit(["javac", "-nowarn", "-d", dc] + files, runs_javac)
        same = ""
        if main:
            same = "  output identical" if run_java(dj, main) == run_java(dc, main) else "  OUTPUT DIFFERS!"
        print(f"{label:<28} jolt {fmt(tj)}   javac {fmt(tc)}   {tc / tj:7.0f}x faster{same}")
        return tj, tc
    finally:
        shutil.rmtree(tmp)


def main():
    subprocess.run(["cargo", "build", "--release", "--quiet"], cwd=ROOT, check=True)
    print(subprocess.run(["java", "-version"], capture_output=True, text=True).stderr.splitlines()[0])
    print()
    progs = sorted(glob.glob(os.path.join(ROOT, "tests", "programs", "*.java")))
    rj, rc = (5, 2) if QUICK else (21, 5)
    for p in progs:
        name = os.path.basename(p)[:-5]
        compare(name, [p], rj, rc, main=name)
    compare("all test programs together", progs, rj, rc)
    print()
    sizes = [(10, 20), (100, 40)] if QUICK else [(10, 20), (100, 40), (400, 50)]
    for nfiles, nmethods in sizes:
        gen = tempfile.mkdtemp()
        try:
            subprocess.run([sys.executable, os.path.join(ROOT, "bench", "gen.py"), gen, str(nfiles), str(nmethods)], check=True)
            files = sorted(glob.glob(os.path.join(gen, "*.java")))
            lines = sum(sum(1 for _ in open(f)) for f in files)
            tj, tc = compare(f"generated {lines // 1000}k lines", files, 5 if QUICK else 9, 1 if QUICK else 3, main="Main")
            print(f"{'':<28} jolt {lines / tj / 1e6:6.2f}M lines/s   javac {lines / tc / 1e3:7.0f}k lines/s")
        finally:
            shutil.rmtree(gen)


if __name__ == "__main__":
    main()
