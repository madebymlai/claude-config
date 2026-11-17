"""Compare the files explore-rust and the Python explore tag [tracked, hidden by .gitignore], case by case.

  python3 -I tags.py [--bin BIN] [--py PY]

Each case is a hits run; the tagged files are read from its output, whether grouped, a -l digest or the by-file
counts. A case prints the number tagged by each and the files only one of them tags. PY defaults to scripts/explore.
"""
import argparse
import os
import re
import shlex
import subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
TAG = "  [tracked, hidden by .gitignore]"
T = "/home/laimk/git/dsrs/taupos-self-interface"
CASES = f"""
{T}                              hits -l ''
{T}                              hits -c deprecated
{T}                              hits -l -g '*.html' ''
{T}                              hits -l '' src JavaDoc_Interface/index-files
{T}/src                          hits -l ''
/home/laimk/git/helix-trader     hits -l ''
/tmp/explore-rust-fx             hits -l ''
/tmp/explore-rust-fx             hits -l '' langs odd
/tmp/explore-rust-fx             hits -l -t rust ''
/tmp/explore-rust-fx             hits -l -T rust ''
/tmp/explore-rust-fx             hits -l -g '!*.log' ''
/tmp/explore-rust-fx             hits -l -L ''
/tmp/explore-rust-fx             hits -l --max-depth 1 ''
/tmp/explore-rust-fx             hits -l --no-hidden ''
/tmp/explore-rust-fx             hits -l --max-filesize 10 ''
/tmp/explore-rust-fx             hits -l '' link_dir link_file.rs
/tmp/explore-rust-fx/odd         hits -l ''
/tmp/explore-rust-fx/odd         hits -l '' ..
/tmp/explore-review/r            hits -l ''
/tmp/explore-review/r            hits -l '' src nested vendor
/tmp/explore-review/r/vendor/sm  hits -l ''
/tmp/explore-rev2/m              hits -l ''
/tmp/explore-rev2/m              hits -l '' gen sub vendor
/tmp/explore-rev2/m/vendor/sm    hits -l ''
/tmp/explore-rev3/o              hits -l ''
/tmp/explore-rev3/o              hits -l '' wt src
/tmp/explore-rev3/o/wt           hits -l ''
/tmp/explore-rev4/s              hits -l ''
/tmp/explore-rev5/p              hits -l ''
/tmp/explore-rev5/p              hits -l -g '*.rs' ''
/tmp/explore-rev5/p              hits -l '' src
/tmp/explore-work3/fx/top        hits -l ''
/tmp/explore-work3/fx/top        hits -l '' src
/tmp/explore-work3/fx/top        hits -l '' ./src/ out/sub
/tmp/explore-work3/fx/top        hits -l '' 'lit/weird[1].txt'
/tmp/explore-work3/fx/top        hits -l '' 'lit/weird*.txt'
/tmp/explore-work3/fx/top        hits -l '' 'sp ace'
/tmp/explore-work3/fx/top        hits -l '' .
/tmp/explore-work3/fx/top        hits -l '' /tmp/explore-work3/fx/top/src
/tmp/explore-work3/fx/top        hits -l '' /tmp/explore-work3/fx/srclink
/tmp/explore-work3/fx/top        hits -l '' ../srclink/deep
/tmp/explore-work3/fx/top        hits -l '' /tmp/explore-work3/fx/sm
/tmp/explore-work3/fx/top        hits -l '' vendor/sm
/tmp/explore-work3/fx/top/src    hits -l ''
/tmp/explore-work3/fx/top/src    hits -l '' ../out deep
/tmp/explore-work3/fx/top/src    hits -l '' ..
/tmp/explore-work3/fx/top/vendor/sm  hits -l ''
/tmp/explore-work3/fx/top/vendor/sm  hits -l '' gen
/tmp/explore-work3/fx/top/vendor/sm  hits -l '' gen/s.txt ../../src
"""


def tagged(out):
    files = set()
    for line in os.fsdecode(out).splitlines():
        if line.endswith(TAG):
            files.add(re.sub(r"^\s*\d+  ", "", line[: -len(TAG)]))
    return files


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=os.path.join(HERE, "../target/release/explore-rust"))
    ap.add_argument("--py", default=os.path.normpath(os.path.join(HERE, "../../scripts/explore")))
    a = ap.parse_args()
    differ = 0
    for line in CASES.strip().splitlines():
        cwd, rest = line.split(None, 1)
        args = shlex.split(rest)
        if not os.path.isdir(cwd):
            print(f"skipped {cwd}")
            continue
        run = lambda cmd: subprocess.run(cmd, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True).stdout
        want, got = tagged(run(["python3", "-I", a.py, *args])), tagged(run([os.path.abspath(a.bin), *args]))
        same = want == got
        differ += not same
        print(f"{'same  ' if same else 'DIFFER'} py {len(want):4} rs {len(got):4}  {cwd} {shlex.join(args)}")
        for f in sorted(want - got):
            print(f"         only py: {f}")
        for f in sorted(got - want):
            print(f"         only rs: {f}")
    print(f"{differ} cases differ")


main()
