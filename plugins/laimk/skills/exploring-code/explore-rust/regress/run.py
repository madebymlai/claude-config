"""Compare explore-rust with the Python explore it rewrites, case by case: stdout and exit code.

  python3 -I run.py [--bin BIN] [--out DIR] [--only SUBSTRING]

Each case runs `python3 -I scripts/explore ARGS` and `BIN ARGS` from the same directory, with stdin from /dev/null.
The Python script is scripts/explore as it is in the working tree, copied when the run starts; --py names another.
A case whose directory is missing is reported as skipped. The fixture repository comes from make_fixture.py, which
this script runs first; the other directories are existing repositories and review fixtures on this machine.
A --json case compares bytes too; when they differ, it also reports whether the documents are equal once decoded
with msgspec. Differing outputs are written to DIR (default /tmp/explore-rust-regress) as CASE.py and CASE.rs.
"""
import argparse
import json
import glob
import os
import re
import shlex
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PY = os.path.normpath(os.path.join(HERE, "../../scripts/explore"))
FX = "/tmp/explore-rust-fx"
DIRS = {
    "fx": FX,
    "fxsub": FX + "/target",
    "fxlat": FX + "/lat",
    "r": "/tmp/explore-review/r",
    "m": "/tmp/explore-rev2/m",
    "msm": "/tmp/explore-rev2/m/vendor/sm",
    "skip": "/tmp/explore-work3/skip",
    "skip2": "/tmp/explore-work3/skip2",
    "top": "/tmp/explore-work3/fx/top",
    "rev7": "/tmp/explore-rev7",
    "helix": "/home/laimk/git/helix-trader",
    "taupos": "/home/laimk/git/dsrs/taupos-self-interface",
    "tmp": "/tmp",
    "corpus": "/tmp/explore-rust-corpus",
    "places": "/tmp/explore-json/subs/places",
    "beads": "/home/laimk/git/beads",
    "archi": "/home/laimk/git/archi-LAI",
    "jobjoy": "/home/laimk/git/jobjoy",
    "nautilus": "/home/laimk/git/nautilus_trader",
    "coop": "/home/laimk/git/dsrs/coop-24-7",
    "tuaself2": "/home/laimk/git/dsrs/tuaself-2-layer",
    "vision": "/home/laimk/git/dsrs/vision-self-checkout",
}
LATIN1_JAVA = "src/it/studiors/tauself/interfaces/UtilityPacchettoZip.java"

# (directory, arguments as a shell line); the argument line is split with shlex, so quoting works as in a shell
CASES = """
fx      --help
fx      -h
fx
fx      bogus
fx      hits --help
fx      outline -h
fx      hits -ih NEEDLE_P1 flags
fx      hits -- --help flags
fx      show langs/a.rs:3 --help
fx      hits NEEDLE_P1 flags -- -h
fx      hits
fx      hits NEEDLE_
fx      hits ZZZ_NO_MATCH
fx      hits ZZZ_NO_MATCH no/such/path
fx      hits ZZZ_NO_MATCH langs ctx
fx      hits MANY_ --max 20
fx      hits MANY_ --max=5
fx      hits MANY_ --max 0
fx      hits MANY_ --max x
fx      hits MANY_ --max
fx      hits --max 1000 NEEDLE_
fx      hits -c NEEDLE_
fx      hits -l NEEDLE_
fx      hits --count NEEDLE_P
fx      hits --files-with-matches NEEDLE_P
fx      hits --count-matches 'x=1' flags
fx      hits --count-matches NEEDLE_P1 flags
fx      hits -c MANY_ --max 3
fx      hits -C 1 NEEDLE_CTX
fx      hits -C 2 NEEDLE_CTX ctx
fx      hits -C2 NEEDLE_CTX ctx/n.py
fx      hits -A 1 NEEDLE_CTX
fx      hits -B 2 NEEDLE_CTX
fx      hits -A1 -B1 NEEDLE_CTX3
fx      hits --context=3 NEEDLE_CTX6
fx      hits --after-context 1 NEEDLE_CTX5
fx      hits -C 1 NEEDLE_LONG
fx      hits -i needle_case
fx      hits -S needle_case
fx      hits -S Needle_Case
fx      hits -s needle_case
fx      hits -w NEEDLE_WORD
fx      hits --word-regexp NEEDLE_WORD
fx      hits -F a.b flags
fx      hits a.b flags
fx      hits -F 'foo(bar)' flags
fx      hits -x NEEDLE_LINE
fx      hits --line-regexp NEEDLE_LINE
fx      hits -U 'MLSTART\\n\\s*MLEND'
fx      hits -U -c 'MLSTART\\n\\s*MLEND'
fx      hits --multiline 'MLSTART.*' ml
fx      hits -Uc 'MLSTART\\n'
fx      hits -inw needle_word
fx      hits -t rust NEEDLE_
fx      hits -tpy NEEDLE_
fx      hits --type=java NEEDLE_
fx      hits -T rust NEEDLE_
fx      hits --type-not rust NEEDLE_
fx      hits -t nosuchtype NEEDLE_
fx      hits -g '*.py' NEEDLE_
fx      hits -g '!*.rs' NEEDLE_
fx      hits --glob='langs/*.t*' NEEDLE_
fx      hits --iglob '*.java' NEEDLE_
fx      hits -g '*.log' NEEDLE_TRACKED
fx      hits -g '*.rs' NEEDLE_TRACKED
fx      hits -g '[' NEEDLE_
fx      hits -L NEEDLE_RS
fx      hits --follow NEEDLE_RS1
fx      hits NEEDLE_RS link_dir
fx      hits NEEDLE_RS link_file.rs
fx      hits --hidden NEEDLE_HIDDEN
fx      hits -m 1 NEEDLE_P1
fx      hits -m1 -C1 NEEDLE_MAXC
fx      hits --max-count=2 NEEDLE_MAXC
fx      hits -m 0 NEEDLE_P1
fx      hits -e NEEDLE_P1 -e NEEDLE_P2 flags
fx      hits -eNEEDLE_P1 flags
fx      hits -ie needle_p1 flags
fx      hits --regexp=NEEDLE_P2 flags
fx      hits --regexp NEEDLE_P2
fx      hits -f flags/pats flags
fx      hits --file=flags/pats
fx      hits -f flags/pats -e NEEDLE_CASE
fx      hits -f /dev/null
fx      hits -f - flags
fx      hits -v -f /dev/null flags/line.txt
fx      hits -f no/such/pats
fx      hits -e
fx      hits -e -dash odd
fx      hits -- -dash odd
fx      hits -F -- --max flags
fx      hits NEEDLE_DASH odd/-dash.rs
fx      hits -E latin1 Käufer lat
fx      hits Käufer lat
fx      hits -E latin1 NEEDLE_LAT
fx      hits --encoding=latin1 größe lat
fx      hits -E utf-8 NEEDLE_LAT
fx      hits -E none NEEDLE_UTF16
fx      hits NEEDLE_UTF16
fx      hits -E bogus NEEDLE_LAT
fx      hits -P 'foo(?=bar)' flags
fx      hits --pcre2 'NEEDLE_P\\d(?!.*P2)' flags
fx      hits --engine=pcre2 NEEDLE_P1 flags
fx      hits NEEDLE_LAT
fx      hits NEEDLE_LATNAME
fx      hits -c NEEDLE_LATNAME
fx      hits NEEDLE_ skip
fx      hits NEEDLE_NODEF
fx      hits NEEDLE_UPPER
fx      hits NEEDLE_MIXED
fx      hits NEEDLE_DOT skip
fx      hits SUPPRESSED_NEEDLE
fx      hits NEEDLE_BOM
fx      hits NEEDLE_CRLF
fx      hits NEEDLE_HUGE
fx      hits NEEDLE_WIDE
fx      hits NEEDLE_BIN
fx      hits NEEDLE_BIN bin/late.rs
fx      hits NEEDLE_BIN bin/early.dat
fx      hits NEEDLE_BIN bin/early.dat bin/late.rs
fx      hits -a NEEDLE_BIN
fx      hits --text NEEDLE_BIN1
fx      hits NEEDLE_LONG
fx      hits NEEDLE_RS
fx      hits NEEDLE_JAVA
fx      hits NEEDLE_KT
fx      hits NEEDLE_PY
fx      hits NEEDLE_JS
fx      hits NEEDLE_MJS
fx      hits NEEDLE_TS
fx      hits NEEDLE_GO
fx      hits NEEDLE_CS
fx      hits NEEDLE_C
fx      hits NEEDLE_H
fx      hits NEEDLE_RB
fx      hits NEEDLE_PHP
fx      hits NEEDLE_SW
fx      hits NEEDLE_SC
fx      hits NEEDLE_LUA
fx      hits NEEDLE_SH
fx      hits NEEDLE_TRACKED
fx      hits NEEDLE_TRACKED target
fx      hits NEEDLE_TRACKED target/out.rs
fx      hits NEEDLE_TRACKED odd
fx      hits NEEDLE_TRACKED 'odd/hid[1].log'
fx      hits -c NEEDLE_TRACKED
fx      hits NEEDLE_TRACKED_ENV .
fx      hits NEEDLE_TRACKED /tmp/explore-rust-fx/target
fx      hits NEEDLE_UNTRACKED_HIDDEN
fx      hits --no-ignore NEEDLE_UNTRACKED_HIDDEN
fx      hits --no-ignore-vcs NEEDLE_UNTRACKED
fx      hits -u NEEDLE_UNTRACKED
fx      hits NEEDLE_DOTIGNORE
fx      hits NEEDLE_DOTFILE
fx      hits NEEDLE_SPACE 'dir with space'
fx      hits NEEDLE_WEIRD 'odd/weird[1].txt'
fx      hits NEEDLE_STAR 'odd/star*.txt'
fx      hits NEEDLE_LOCKEDDIR ../explore-rust-fx-locked
fx      hits ZZZ_NOPE ../explore-rust-fx-locked
fx      hits --max-depth 1 NEEDLE_
fx      hits -d 2 NEEDLE_
fx      hits 'unclosed('
fx      hits -io NEEDLE_P1 flags
fx      hits -iq NEEDLE_P1 flags
fx      hits -M10 NEEDLE_P1 flags
fx      hits -rX NEEDLE_P1 flags
fx      hits -r X NEEDLE_P1 flags
fx      hits -R NEEDLE_P1 flags
fx      hits --with-filename --sort-files NEEDLE_P1 flags
fx      hits --no-sort-files NEEDLE_P1 flags
fx      hits -b NEEDLE_P1 flags
fx      hits --column NEEDLE_P1 flags
fx      hits --trim NEEDLE_P1 flags
fx      hits -V
fx      hits --type-list
fx      hits --passthru NEEDLE_P1 flags
fx      hits --passthrough NEEDLE_P2 flags/p.rs flags/max.txt
fx      hits --passthrough -A 1 NEEDLE_MAXC flags/max.txt
fx      hits --pre pre/upper.sh NEEDLE_PRE pre
fx      hits --pre pre/upper.sh --pre-glob '*.txt' -i needle_pre pre
fx      hits --pre pre/upper.sh --no-pre needle_pre pre
fx      hits --pre no/such/tool needle_pre pre
fx      hits -z NEEDLE_GZ zip
fx      hits --search-zip -c NEEDLE_GZ
fx      hits NEEDLE_GZ zip
fx      hits --null-data NEEDLE_NUL nul
fx      hits NEEDLE_NUL nul
fx      hits --stop-on-nonmatch NEEDLE_MAXC flags
fx      hits --ignore-file flags/ign NEEDLE_P flags
fx      hits --ignore-file flags/ign --no-ignore-files NEEDLE_P flags
fx      hits --ignore-file no/such/ign NEEDLE_P1 flags
fx      hits --ignore-file flags/ign --ignore-file-case-insensitive -c NEEDLE_
fx      hits --no-messages NEEDLE_RS
fx      hits --no-messages ZZZ_NOPE
fx      hits --no-ignore-messages NEEDLE_RS
fx      hits --no-mmap NEEDLE_BIN bin/late.rs
fx      hits --mmap NEEDLE_BIN bin
fx      hits --regex-size-limit 10 'a{100}' flags
fx      hits --dfa-size-limit 1K NEEDLE_P1 flags
fx      hits --auto-hybrid-regex 'foo(?=bar)' flags
fx      hits --no-pcre2-unicode -P 'NEEDLE_P\\w' flags
fx      hits --no-config NEEDLE_P1 flags
fx      hits -n -N NEEDLE_P1 flags
fx      hits --no-line-number -r -H NEEDLE_P1 flags
fx      hits --sort path --color never NEEDLE_P1 flags
fx      hits --line-number --colors 'match:fg:red' NEEDLE_P1 flags
fx      hits -q NEEDLE_
fx      hits -p NEEDLE_
fx      hits --files NEEDLE_
fx      hits -v -c x flags/count.txt
fx      hits -o NEEDLE_P1 flags
fx      hits --replace X NEEDLE_P1 flags
fx      hits -M 10 NEEDLE_P1 flags
fx      hits --no-such-flag NEEDLE_P1
fx      hits --crlf 'NEEDLE_CRLF$'
fx      hits 'NEEDLE_CRLF$'
fx      hits -j 2 NEEDLE_P1 flags
fx      hits --threads=1 NEEDLE_P1 flags
fx      hits '' flags/line.txt
fx      hits -w '' flags/line.txt
fx      hits '^' flags/line.txt
fx      hits '$' flags/line.txt
fx      hits -c '' flags
fx      hits --count-matches '' flags/count.txt
fx      hits NEEDLE_ link_file.rs langs/a.rs
fx      hits NEEDLE_RS langs langs/a.rs
fx      hits NEEDLE_RS ./langs/ /tmp/explore-rust-fx/langs/a.rs
fx      hits -C 2 NEEDLE_TIE
fx      hits -B 3 NEEDLE_TIE2
fx      hits NEEDLE_LONG5
fx      hits -C 1 NEEDLE_LONG5
fx      hits --max 1 NEEDLE_TRACKED
fx      hits -c --max 1 NEEDLE_TRACKED
fx      hits NEEDLE_LATNAME1 lat/n%E4me.rs
fx      hits -C 1 NEEDLE_LATNAME
fx      hits %E4 lat
fx      hits -e NEEDLE_LAT lat/n%E4me.rs lat/d%EFr
fx      outline lat/n%E4me.rs lat/UP%E4.JAVA lat/both%E4.rs lat/d%EFr/k.py lat/d%EFr/x.txt
fx      show lat/n%E4me.rs:1 lat/d%EFr/k.py 2
fx      show lat/n%E4me.rs:9
fx      outline lat/n%E4me.r%E4
fx      hits -%E4 NEEDLE_LAT
fx      hits -g '*.zzz' NEEDLE_
fx      hits -g '*.zzz' NEEDLE_ langs
fxsub   hits NEEDLE_
fxsub   hits NEEDLE_ ..
fxsub   hits NEEDLE_TRACKED ../odd
fxsub   hits -c NEEDLE_TRACKED
fxlat   hits NEEDLE_LAT
fxlat   outline Lat.java c1.rs
fx      outline langs/a.rs
fx      outline langs/A.java
fx      outline langs/a.kt
fx      outline langs/a.py
fx      outline langs/a.pyi
fx      outline langs/a.js
fx      outline langs/a.mjs
fx      outline langs/a.ts
fx      outline langs/a.tsx
fx      outline langs/a.go
fx      outline langs/a.cs
fx      outline langs/a.c
fx      outline langs/a.h
fx      outline langs/a.cpp
fx      outline langs/a.hxx
fx      outline langs/a.rb
fx      outline langs/a.php
fx      outline langs/a.swift
fx      outline langs/a.scala
fx      outline langs/a.lua
fx      outline langs/a.sh
fx      outline skip/nodefs.rs skip/empty.rs skip/Upper.JAVA skip/Mixed.Rs skip/notes.txt
fx      outline skip/..rs skip/.rs skip/locked.rs
fx      outline skip/suppressed.rs
fx      outline skip/bom.rs skip/utf16.rs skip/crlf.rs
fx      outline big/huge.rs big/wide.rs
fx      outline lat/Lat.java lat/c1.rs lat/plain.txt
fx      outline long/sig.rs long/end.rs
fx      outline ctx/n.py ctx/m.rs ml/m.rs
fx      outline no/such.rs langs/a.rs
fx      outline langs
fx      outline langs/a.rs ./langs/a.rs
fx      outline link_file.rs
fx      outline bin/late.rs
fx      outline
fx      show langs/a.rs:10
fx      show langs/a.rs:3-8
fx      show langs/a.rs:3,5
fx      show langs/a.rs:12:9
fx      show langs/a.rs:3:9
fx      show langs/a.rs 5 9
fx      show langs/a.rs 20
fx      show langs/a.rs:2 langs/A.java:4-6
fx      show langs/a.rs 1 3 langs/a.py 1 3
fx      show langs/a.rs:500
fx      show langs/a.rs:10-5
fx      show langs/a.rs:0
fx      show langs/a.rs:0-0
fx      show no/such.rs:3
fx      show langs:3
fx      show skip/locked.rs:1
fx      show skip/empty.rs:1
fx      show skip/crlf.rs:1-3
fx      show lat/Lat.java:1-4
fx      show lat/c1.rs:1
fx      show langs/a.rs
fx      show langs/a.rs:x
fx      show
fx      show 'dir with space/s p.rs:1'
fx      show langs/a.rs:2 no/such.rs:1
fx      show langs/a.rs 3 x
r       hits NEEDLE_
r       hits -C 1 NEEDLE_NEW
r       hits -C 1 NEEDLE_CTOR
r       hits -C 1 NEEDLE_ARROW
r       hits -C 1 NEEDLE_DOCSTR
r       hits -C 1 NEEDLE_ML
r       outline src/C.java
r       outline src/a.rs
r       outline src/b.ts
r       outline src/k.kt
r       outline src/p.py
r       outline vendor/sm/s.rs
r       show src/a.rs:10
r       show src/a.rs:3-8
r       show src/a.rs 5 9
r       show src/a.rs 20
r       show src/a.rs:2 src/C.java:4-6
r       show src/a.rs:3,5
r       hits -c NEEDLE_
r       hits NEEDLE_SUBMOD
r       hits NEEDLE_ 'dir with space'
m       hits NEEDLE_
m       hits -c NEEDLE_
m       hits NEEDLE_J java
m       outline java/Ann.java
m       outline cs/a.cs php/a.php swift/a.swift c/a.c
m       outline java/Upper.JAVA rs/utf16.rs ./-dash.rs
m       outline c/a.cpp c/a.h go/a.go js/a.jsx kt/A.kt lua/a.lua py/d.py rb/a.rb rs/a.rs scala/a.scala sh/a.sh ts/c.ts tsx/a.tsx
m       hits -E latin1 Käufer java
m       hits -L NEEDLE_DASH
m       hits -eZZZNOPE sub
m       hits NEEDLE_SMGEN
msm     hits NEEDLE_SMGEN
m       show java/Lat.java:1-7
m       hits NEEDLE_LAT java
m       outline java/Lat.java
m       hits NEEDLE_LONG
m       hits -C 2 NEEDLE_P py
m       outline ts/c.ts
m       hits NEEDLE_NM
m       hits --count-matches x=1 long
skip    outline U.JAVA empty.rs P.java L.java ok.rs
skip    hits NEEDLE_
skip2   hits NEEDLE_
skip2   hits -c NEEDLE_
top     hits -c NEEDLE
top     hits NEEDLE src
rev7    outline s5000.rs fn80k.rs l190k.rs
rev7    outline tenfn_190k.rs mb3_2.rs
rev7    hits -c 'fn ' s5000.rs few_lines_4mb.rs
helix   hits RetrainActor
helix   hits ZZZ_NO_MATCH_XYZ
helix   hits -c 'fn ' src
helix   hits -C 2 -i retrain src/model
helix   outline src/model/launcher.rs
helix   hits -t rust -w 'impl' src/model
taupos  hits CToInt src
taupos  hits -c CToInt
taupos  hits String
taupos  outline {LATIN1_JAVA}
taupos  hits -E latin1 -c 'à' src
taupos  hits -l deprecated JavaDoc_Interface
taupos  hits -w -C 1 CToInt src/it/studiors/tauself/interfaces/pos
taupos  outline @ls:*.java
helix   outline @ls:*.rs
helix   outline @ls:*.py
m       outline @ls:*
r       outline @ls:*
taupos  outline @ls:*.js
corpus  outline @glob:**/*
corpus  hits -w -C 1 'def|function|fun|func|class'
beads   outline @ls:*.go
beads   outline @ls:*.sh
archi   outline @ls:*.ts @ls:*.tsx @ls:*.swift @ls:*.kt
archi   outline @ls:*.py
jobjoy  outline @ls:*.java @ls:*.h @ls:*.cpp @ls:*.swift @ls:*.tsx
nautilus outline @ls:*.rs
nautilus outline @ls:*.py @ls:*.sh
nautilus hits -c 'fn '
nautilus hits OrderBook crates/model/src/orderbook
coop    outline @ls:*.kt @ls:*.ts
tuaself2 outline @ls:*.cc @ls:*.h @ls:*.cpp
vision  outline @ls:*.hpp @ls:*.cpp @ls:*.ts
""".replace("{LATIN1_JAVA}", LATIN1_JAVA)


# --json cases, compared byte for byte, and once decoded when the bytes differ
JSON_CASES = """
fx      hits --json NEEDLE_
fx      hits --json ZZZ_NO_MATCH langs ctx
fx      hits --json MANY_ --max 20
fx      hits --json -c NEEDLE_
fx      hits --json -l NEEDLE_TRACKED
fx      hits --json --count-matches 'x=1' flags
fx      hits --json -C 1 NEEDLE_CTX
fx      hits --json -C 2 NEEDLE_TIE
fx      hits --json NEEDLE_LAT
fx      hits --json NEEDLE_LATNAME
fx      hits --json NEEDLE_ skip
fx      hits --json NEEDLE_CRLF
fx      hits --json NEEDLE_LONG
fx      hits --json NEEDLE_TRACKED
fx      hits --json -U 'MLSTART\\n\\s*MLEND'
fx      hits --json NEEDLE_RS
fx      hits --json NEEDLE_C
fx      hits -- --json langs
fx      outline --json langs/a.rs langs/A.java langs/a.ts langs/a.cpp langs/a.c langs/a.kt langs/a.go
fx      outline --json @glob:langs/*
fx      outline --json skip/nodefs.rs skip/empty.rs skip/Upper.JAVA skip/notes.txt skip/locked.rs no/such.rs
fx      outline --json lat/n%E4me.rs lat/d%EFr/k.py
fx      outline --json big/huge.rs
fx      outline --json no/such.rs
fx      outline --json langs/a.rs no/such.rs langs ./no/such.rs
fx      outline --json lat/n%E4me.r%E4 lat/n%E4me.rs
fx      show --json langs/a.rs:3
m       hits --json NEEDLE_
m       outline --json @ls:*
r       hits --json -C 1 NEEDLE_
helix   hits --json RetrainActor
helix   hits --json -c 'fn ' src
helix   outline --json @ls:*.rs
taupos  hits --json CToInt src
taupos  hits --json String
taupos  outline --json {LATIN1_JAVA}
taupos  hits --json -w CToInt
taupos  hits --json -C 1 -w CToInt src
places  hits --json 'NEEDLE_(code|comment|string)'
places  hits --json -U 'NEEDLE_code,\\n\\s*x\\)|NEEDLE_comment'
places  hits --json -i needle_
places  hits --json -v -e NEEDLE notes.txt a.rs
places  hits --json -w NEEDLE_code -C 2
fx      hits --json 'NEEDLE_[A-Z]+[0-9]'
fx      hits --json NEEDLE_LONG5
fx      hits --json -E latin1 NEEDLE_LAT
fx      hits --json 'x=1' flags/count.txt
fx      hits --json '' flags/line.txt
fx      hits --json -v -c x flags/count.txt
fx      hits --json -v NEEDLE flags/line.txt
fx      hits --json -U 'MLSTART\\n'
fx      hits --json --null-data NEEDLE_NUL nul
corpus  outline --json @glob:**/*
""".replace("{LATIN1_JAVA}", LATIN1_JAVA)


def raw(arg):
    """ARG with each %XX written as the byte XX, for names and patterns that are not UTF-8."""
    return re.sub(rb"%([0-9A-F]{2})", lambda m: bytes([int(m.group(1), 16)]), arg.encode())


def cases(text=None):
    for line in (CASES if text is None else text).strip().splitlines():
        key, _, rest = line.strip().partition(" ")
        yield key, [raw(a) for a in shlex.split(rest)]


def expand(args, cwd):
    """ARGS with each @ls:GLOB replaced by the files git tracks under CWD that match GLOB, and each @glob:GLOB by
    the files under CWD that match it."""
    out = []
    for a in args:
        if a.startswith(b"@ls:"):
            ls = subprocess.run([b"git", b"ls-files", b"-z", b"--", a[4:]], cwd=cwd, capture_output=True, check=True)
            out += [f for f in ls.stdout.split(b"\0") if f]
        elif a.startswith(b"@glob:"):
            found = glob.glob(os.fsdecode(a[6:]), root_dir=cwd, recursive=True)
            out += sorted(os.fsencode(f) for f in found if os.path.isfile(os.path.join(cwd, f)))
        else:
            out.append(a)
    return out


def json_diagnosis(args, want, got):
    """For a --json case whose bytes differ, whether the two documents still decode to equal values."""
    if b"--json" not in args:
        return ""
    import msgspec

    try:
        same = msgspec.json.decode(want[0]) == msgspec.json.decode(got[0])
    except msgspec.DecodeError as e:
        return f"  [--json: not both JSON: {e}]"
    return "  [--json: equal once parsed]" if same and want[1] == got[1] else "  [--json: differs once parsed]"


def run(cmd, cwd, env=None):
    """Stdout and exit code of CMD; its stderr goes to STDERRS when the run is a --json one that succeeded."""
    r = subprocess.run(cmd, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True, env=env)
    if b"--json" in cmd and r.returncode == 0 and r.stderr:
        STDERRS.append((cmd, r.stderr))
    return r.stdout, r.returncode


# successful --json runs that wrote to stderr, which a reader merging stderr into stdout cannot parse
STDERRS = []
STUBS = "/tmp/explore-rust-stubs"


def stub_paths():
    """PATH values for the warning checks: one whose ast-grep fails, and one with no ast-grep."""
    fail, none = os.path.join(STUBS, "fail"), os.path.join(STUBS, "none")
    for d in (fail, none):
        os.makedirs(d, exist_ok=True)
        for tool in ("rg", "git"):
            link = os.path.join(d, tool)
            if not os.path.lexists(link):
                os.symlink(subprocess.run(["which", tool], capture_output=True, text=True).stdout.strip(), link)
    with open(os.path.join(fail, "ast-grep"), "w") as f:
        f.write("#!/bin/sh\necho 'ast-grep: simulated failure' >&2\nexit 3\n")
    os.chmod(os.path.join(fail, "ast-grep"), 0o755)
    return {"ast-grep fails": fail, "no ast-grep": none}


# --json runs while ast-grep fails or is missing: the outputs differ by design, since explore-rust parses without
# ast-grep, so these check what both must do: succeed with nothing on stderr and the document ending in warnings, or
# fail with a message on stderr and no document
WARNING_CASES = """
fx      hits --json NEEDLE_RS
fx      outline --json langs/a.rs no/such.rs
"""


def check_warnings(py, rust):
    failures = []
    for why, path in stub_paths().items():
        env = dict(os.environ, PATH=path)
        for key, args in cases(WARNING_CASES):
            for name, cmd in (("python", [os.fsencode(sys.executable), b"-I", os.fsencode(py)]), ("rust", [os.fsencode(rust)])):
                r = subprocess.run([*cmd, *args], cwd=DIRS[key], stdin=subprocess.DEVNULL, capture_output=True,
                                   env=env)
                label = f"{why}: {name} {shlex.join(os.fsdecode(a) for a in args)}"
                try:
                    doc = json.loads(r.stdout)
                    last = list(doc)[-1]
                except (ValueError, IndexError):
                    doc, last = None, None
                if r.returncode != 0 and not r.stdout:
                    # an error: a message on stderr and no document, as the --help says
                    print(f"  error, {label}: exit {r.returncode}: {os.fsdecode(r.stderr).strip()}")
                elif r.returncode != 0 or r.stderr or last != "warnings":
                    failures.append(f"{label}: exit {r.returncode}, stderr {r.stderr[:80]!r}, last key {last}")
                else:
                    print(f"  warnings, {label}: {doc['warnings']}")
    return failures


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=os.path.join(HERE, "../target/release/explore-rust"))
    ap.add_argument("--out", default="/tmp/explore-rust-regress")
    ap.add_argument("--only", default="")
    ap.add_argument("--no-fixture", action="store_true")
    ap.add_argument("--py", default=PY, help="the Python explore to compare with (default: scripts/explore)")
    ap.add_argument("--keep", action="store_true", help="write every case's Python output to DIR, as CASE.out")
    a = ap.parse_args()
    # a label can hold a name that is not UTF-8, as its surrogate escapes
    sys.stdout.reconfigure(errors="backslashreplace")
    # a copy, so that edits to the script while the run goes on do not move the oracle
    py = os.path.join(a.out + "-oracle", "explore")
    os.makedirs(os.path.dirname(py), exist_ok=True)
    with open(a.py, "rb") as src, open(py, "wb") as dst:
        dst.write(src.read())
    if not a.no_fixture:
        subprocess.run([sys.executable, "-I", os.path.join(HERE, "make_fixture.py"), FX], check=True)
    os.makedirs(a.out, exist_ok=True)
    for f in os.listdir(a.out):
        os.remove(os.path.join(a.out, f))
    plan = [(key, args) for key, args in cases()] + [(key, args) for key, args in cases(JSON_CASES)]
    same, differ, skipped = 0, [], []
    for n, (key, args) in enumerate(plan):
        label = f"{n:03d} {key} {shlex.join(os.fsdecode(a) for a in args)}"[:200]
        if a.only not in label:
            continue
        cwd = DIRS[key]
        if not os.path.isdir(cwd):
            skipped.append(label)
            continue
        args = expand(args, cwd)
        want = run([b"python3", b"-I", os.fsencode(py), *args], cwd)
        got = run([os.fsencode(os.path.abspath(a.bin)), *args], cwd)
        if a.keep:
            with open(os.path.join(a.out, f"{n:03d}.out"), "wb") as f:
                f.write(os.fsencode(f"$ {label}\n") + want[0] + f"[exit {want[1]}]\n".encode())
        if want == got:
            same += 1
            continue
        differ.append(label + json_diagnosis(args, want, got))
        for ext, (out, code) in (("py", want), ("rs", got)):
            with open(os.path.join(a.out, f"{n:03d}.{ext}"), "wb") as f:
                f.write(out + f"[exit {code}]\n".encode())
    total = same + len(differ)
    print(f"{same} of {total} cases identical" + (f", {len(skipped)} skipped" if skipped else ""))
    for label in differ:
        print("  differs: " + label)
    for label in skipped:
        print("  skipped: " + label)
    for cmd, err in STDERRS:
        print(f"  stderr on a successful --json run: {shlex.join(os.fsdecode(c) for c in cmd)}: {err[:200]!r}")
    failures = [] if a.only else check_warnings(py, os.path.abspath(a.bin))
    for f in failures:
        print("  warning check failed: " + f)
    sys.exit(1 if differ or STDERRS or failures else 0)


main()
