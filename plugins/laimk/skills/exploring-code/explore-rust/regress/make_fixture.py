"""Build the regression fixture repository at DIR (default /tmp/explore-rust-fx), replacing any earlier one.

  python3 -I make_fixture.py [DIR]

The fixture is a git repository holding what the regression cases search: a definition-bearing file in each of the
17 languages, Latin-1 contents and names, files with no definitions and files the parser skips, binary files,
tracked files .gitignore hides, long lines, nested blocks for -C, and enough matches to pass a cap.
"""
import os
import shutil
import subprocess
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/explore-rust-fx"

LANGS = {
    "langs/a.rs": """\
use std::fmt;

/// A point.
#[derive(Debug)]
pub struct Point { x: i32 }

enum Shape { Dot, Line }

union Bits { a: u32, b: f32 }

macro_rules! twice { ($e:expr) => { $e * 2 } }

mod inner {
    pub fn helper() -> i32 { NEEDLE_RS1 }
}

trait Draw {
    fn draw(&self);
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let c = |v: i32| v + NEEDLE_RS2;
        write!(f, "{}", c(self.x))
    }
}

impl Point {
    #[inline]
    pub const fn new(
        x: i32,
    ) -> Self {
        Point { x: NEEDLE_RS3 }
    }
}
""",
    "langs/A.java": """\
package demo;

@interface Marker { String value(); }

public class A {
    enum Kind { ONE, TWO }
    record Pair(int a, int b) { }
    interface Shape { void area(); }

    @Override
    public String toString() {
        return "NEEDLE_JAVA1";
    }

    @Deprecated
    @SuppressWarnings("x")
    A(int x) {
        int y = NEEDLE_JAVA2;
    }
}
""",
    "langs/a.kt": """\
package demo

class Box(val v: Int) {
    fun get(): Int = NEEDLE_KT1
    companion object {
        fun make() = Box(NEEDLE_KT2)
    }
}

object Single {
    fun hello() { println("NEEDLE_KT3") }
}

fun top(x: Int) = x + NEEDLE_KT4
""",
    "langs/a.py": '''\
import os


class Thing:
    """NEEDLE_PY1 in a docstring"""

    @property
    def size(self):
        return NEEDLE_PY2

    async def run(self):
        def inner():
            return NEEDLE_PY3
        return inner


def top(
    a,
):
    return NEEDLE_PY4
''',
    "langs/a.pyi": "def stub(x: int) -> int: ...  # NEEDLE_PYI\n",
    "langs/a.js": """\
function plain() { return NEEDLE_JS1; }
function* gen() { yield NEEDLE_JS2; }
class K {
  method() { return NEEDLE_JS3; }
}
const arrow = (x) => x + NEEDLE_JS4;
let notArrow = NEEDLE_JS5;
""",
    "langs/a.mjs": "export const f = () => NEEDLE_MJS;\n",
    "langs/a.ts": """\
namespace NS {
  export function inNs() { return NEEDLE_TS1; }
}
declare module "m" {
  function decl(): void;
}
interface I { a: number }
enum E { A, B }
abstract class Base {
  abstract area(): number;
  describe() { return NEEDLE_TS2; }
}
function* g() { yield NEEDLE_TS3; }
const af = async (x: number) => x + NEEDLE_TS4;
""",
    "langs/a.tsx": """\
namespace UI {
  export const Button = () => <b>NEEDLE_TSX1</b>;
}
interface Props { a: string }
enum Mode { A }
abstract class View {
  render() { return <i>NEEDLE_TSX2</i>; }
}
function App() { return <div>NEEDLE_TSX3</div>; }
""",
    "langs/a.go": """\
package main

type T struct{ a int }

type (
	U int
	V string
)

func (t *T) Method() int { return NEEDLE_GO1 }

func main() {
	f := func() int { return NEEDLE_GO2 }
	_ = f
}
""",
    "langs/a.cs": """\
namespace Demo {
    public class C {
        public C() { var x = NEEDLE_CS1; }
        [Obsolete]
        public int M() { return NEEDLE_CS2; }
    }
    struct S { }
    interface IS { }
    enum En { A }
    record R(int A);
}
""",
    "langs/a.c": """\
#include <stdio.h>

struct pt { int x; };

static int
add(int a, int b)
{
    return a + b + NEEDLE_C1;
}

int *ptr_fn(void) { return NEEDLE_C2; }
""",
    "langs/a.h": "struct hdr { int NEEDLE_H; };\nint decl(void);\n",
    "langs/a.cpp": """\
namespace ns {
class Widget {
public:
    void draw() { int n = NEEDLE_CPP1; }
};
struct Plain { int a; };
}

template <typename T>
T twice(T v) { return v * NEEDLE_CPP2; }

void ns::Widget::other() { NEEDLE_CPP3; }
""",
    "langs/a.hxx": "class Hidden { int NEEDLE_HXX; };\n",
    "langs/a.rb": """\
module Mod
  class Klass
    def inst
      NEEDLE_RB1
    end

    def self.cls
      NEEDLE_RB2
    end
  end
end
""",
    "langs/a.php": """\
<?php
interface Shape { public function area(); }
trait T { function tm() { return NEEDLE_PHP1; } }
class P implements Shape {
    public function area() { return NEEDLE_PHP2; }
}
function free_fn() { return NEEDLE_PHP3; }
""",
    "langs/a.swift": """\
protocol Drawable { func draw() }
class View {
    func render() -> Int { return NEEDLE_SW1 }
}
struct Pt {
    func norm() -> Int { NEEDLE_SW2 }
}
func top() { print(NEEDLE_SW3) }
""",
    "langs/a.scala": """\
trait Greeter { def greet(): String }
object Main {
  def main(args: Array[String]): Unit = println(NEEDLE_SC1)
}
class Impl extends Greeter {
  def greet(): String = NEEDLE_SC2
}
""",
    "langs/a.lua": """\
local function loc() return NEEDLE_LUA1 end
function M.dotted() return NEEDLE_LUA2 end
function obj:method() return NEEDLE_LUA3 end
""",
    "langs/a.sh": """\
#!/bin/sh
greet() {
  echo NEEDLE_SH1
}
function other {
  echo NEEDLE_SH2
}
echo NEEDLE_SH3
""",
}

LATIN1 = {
    # Latin-1 contents in a parsed language
    "lat/Lat.java": "// größe\npublic class Lat {\n    String käufer() { return \"NEEDLE_LAT1 Käufer\"; }\n}\n",
    # Latin-1 contents with bytes 0x80-0x9f, which windows-1252 would read as other characters
    "lat/c1.rs": "fn c1() { let s = \"\x85\x96\xa0 NEEDLE_LAT2\"; }\n",
    "lat/plain.txt": "caf\xe9 NEEDLE_LAT3\n",
}


def write(rel, data, mode="w"):
    path = os.path.join(ROOT, rel) if isinstance(rel, str) else os.path.join(os.fsencode(ROOT), rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, mode) as f:
        f.write(data)


def git(*args):
    subprocess.run(["git", *args], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)


def remove(path):
    if os.path.lexists(path):
        for dirpath, dirnames, _ in os.walk(path):
            for d in dirnames:
                os.chmod(os.path.join(dirpath, d), 0o755)
        shutil.rmtree(path)


def main():
    remove(ROOT)
    remove(ROOT + "-locked")
    os.makedirs(ROOT)
    git("init", "-q", "-b", "main")
    git("config", "user.email", "fixture@example.invalid")
    git("config", "user.name", "fixture")
    for rel, text in LANGS.items():
        write(rel, text)
    for rel, text in LATIN1.items():
        write(rel, text.encode("latin-1"), "wb")
    # Latin-1 names: a parsed file, one with no language, a plain text file and a directory
    write(b"lat/n\xe4me.rs", b"fn named() {\n    NEEDLE_LATNAME1\n}\n", "wb")
    write(b"lat/UP\xe4.JAVA", b"class Up {\n    int NEEDLE_LATNAME2;\n}\n", "wb")
    write(b"lat/d\xefr/x.txt", b"NEEDLE_LATNAME3\n", "wb")
    write(b"lat/both\xe4.rs", "fn both() {\n    let s = \"\xe4\";\n    NEEDLE_LATNAME4\n}\n".encode("latin-1"), "wb")
    write(b"lat/d\xefr/k.py", "class K\xe4:\n    def m(self):\n        return 'NEEDLE_LATNAME5 \xe9'\n".encode("latin-1"), "wb")

    # files with no definitions, and files the parser skips or cannot parse
    write("skip/nodefs.rs", "use std::io;\nconst X: i32 = 1; // NEEDLE_NODEF\n")
    write("skip/empty.rs", "")
    write("skip/Upper.JAVA", "class Upper { int NEEDLE_UPPER; }\n")
    write("skip/Mixed.Rs", "fn mixed() { NEEDLE_MIXED }\n")
    write("skip/notes.txt", "NEEDLE_TXT plain text\n")
    write("skip/..rs", "fn dots() { NEEDLE_DOTS }\n")
    write("skip/.rs", "fn dotonly() { NEEDLE_DOTONLY }\n")
    write("skip/locked.rs", "fn locked() { NEEDLE_LOCKED }\n")
    write("skip/suppressed.rs", "// ast-grep-ignore\nfn hidden_by_comment() { SUPPRESSED_NEEDLE }\n")
    write("skip/bom.rs", "﻿fn bom() { NEEDLE_BOM }\n")
    write("skip/utf16.rs", "fn wide() { NEEDLE_UTF16 }\n".encode("utf-16"), "wb")
    write("skip/crlf.rs", "fn crlf() {\r\n    NEEDLE_CRLF\r\n}\r\n", "w")
    # a file over ast-grep's size limit (3,000,000 bytes and 200,000 lines) and one just under its line limit
    write("big/huge.rs", "fn big() {\n" + "    let a = 1; // filler line\n" * 210_000 + "    NEEDLE_HUGE\n}\n")
    write("big/wide.rs", "fn wide() {\n" + ("    let a = 1; // " + "x" * 40 + "\n") * 150_000 + "    NEEDLE_WIDE\n}\n")

    # binary files: NUL early, NUL past the first 64 KiB with matches on both sides
    write("bin/early.dat", b"NEEDLE_BIN1\n\x00rest\n", "wb")
    write("bin/late.rs", b"fn late() { NEEDLE_BIN2 }\n" + b"// pad\n" * 12000 + b"\x00\nNEEDLE_BIN3\n", "wb")

    # long lines, cut around the match
    write("long/min.js", "var a=" + "1+" * 200 + "NEEDLE_LONG1" + "+2" * 200 + ";\n")
    write("long/end.rs", "fn e() { let s = \"" + "y" * 300 + " NEEDLE_LONG2\"; }\n")
    write("long/start.rs", "fn s() { let t = \"NEEDLE_LONG3" + "z" * 300 + "\"; }\n")
    write("long/uni.rs", "fn u() { let s = \"" + "\u00e9\u4e00" * 150 + " NEEDLE_LONG5 " + "\u00fc" * 100 + "\"; }\n")
    write("long/sig.rs", "pub fn " + "very_long_name_" * 8 + "(a: i32, b: i32, c: i32) -> i32 { NEEDLE_LONG4 }\n")

    # nested blocks for -C, with tabs, blank lines and a context line equidistant from two matches
    write("ctx/n.py", """\
class Outer:
    def method(self):
        if True:
            x = 1
            y = NEEDLE_CTX1

            z = 3
        return x

\tdef tabbed(self):
\t\treturn NEEDLE_CTX2
def two():
    a = NEEDLE_CTX3
    b = 2
    c = NEEDLE_CTX4
""")
    write("ctx/tie.rs", "fn a() {\n    NEEDLE_TIE1\n}\n\nfn b() {\n    NEEDLE_TIE2\n}\n")
    write("ctx/m.rs", "fn m() {\n    let a = NEEDLE_CTX5;\n    let b = 2;\n}\n\nfn n() {\n    NEEDLE_CTX6\n}\n")
    # multi-line matches, for -U
    write("ml/m.rs", "fn ml() {\n    let a = \"MLSTART\n    MLEND\";\n}\n")

    # many matches across directories, for the cap
    for d in range(4):
        for f in range(6):
            write(f"many/d{d}/f{f}.rs", "".join(f"fn f{i}() {{ MANY_{d}_{f} }}\n" for i in range(3 + d)))

    # case and word forms
    write("flags/case.rs", "fn c() {\n    let a = Needle_Case;\n    let b = needle_case;\n    let c = NEEDLE_CASE;\n"
                           "    let d = xNEEDLE_WORDx;\n    let e = NEEDLE_WORD;\n}\n")
    write("flags/line.txt", "NEEDLE_LINE\n NEEDLE_LINE\nNEEDLE_LINE tail\n")
    write("flags/regex.txt", "a.b\naxb\nfoo(bar)\n")
    write("flags/pats", "NEEDLE_P1\nNEEDLE_P2\n")
    write("flags/p.rs", "fn p() { NEEDLE_P1; NEEDLE_P2; NEEDLE_P1 }\n")
    write("flags/count.txt", "x=1 x=1 x=1\nx=1\n")
    write("flags/max.txt", "".join(f"NEEDLE_MAXC {i}\n" for i in range(6)))
    write("flags/look.txt", "foobar\nfoobaz\n")

    # paths: spaces, glob characters, a leading dash, hidden files and directories, symlinks
    write("dir with space/s p.rs", "fn spaced() { NEEDLE_SPACE }\n")
    write("odd/weird[1].txt", "NEEDLE_WEIRD\n")
    write("odd/star*.txt", "NEEDLE_STAR\n")
    write("odd/-dash.rs", "fn dash() { NEEDLE_DASH }\n")
    write(".hidden/h.rs", "fn hid() { NEEDLE_HIDDEN }\n")
    write(".env.rs", "fn env() { NEEDLE_DOTFILE }\n")
    write(".ignore", "ignored_by_dot_ignore/\n")
    write("ignored_by_dot_ignore/x.rs", "fn dot_ignore() { NEEDLE_DOTIGNORE }\n")
    write("target/out.rs", "fn tracked_target() { NEEDLE_TRACKED_HIDDEN }\n")
    write("target/untracked.rs", "fn untracked_target() { NEEDLE_UNTRACKED_HIDDEN }\n")
    write("target/sub/deep.rs", "fn deep() { NEEDLE_TRACKED_DEEP }\n")
    write("logs/app.log", "NEEDLE_TRACKED_LOG\n")
    write(".env.example", "NEEDLE_TRACKED_ENV=1\n")
    write("odd/hid[1].log", "NEEDLE_TRACKED_GLOBNAME\n")
    write(".gitignore", "target/\n*.log\n.env.example\n")
    os.symlink("langs/a.rs", os.path.join(ROOT, "link_file.rs"))
    os.symlink("langs", os.path.join(ROOT, "link_dir"))

    # inputs for --pre, -z, --null-data and --ignore-file
    write("pre/upper.sh", "#!/bin/sh\ntr a-z A-Z\n")
    os.chmod(os.path.join(ROOT, "pre/upper.sh"), 0o755)
    write("pre/p.txt", "needle_pre one\n")
    write("pre/p.rs", "fn pre() { needle_pre }\n")
    import gzip
    write("zip/z.txt.gz", gzip.compress(b"NEEDLE_GZ inside\n", mtime=0), "wb")
    write("nul/n.dat", b"NEEDLE_NUL one\x00two NEEDLE_NUL\x00three\x00", "wb")
    write("flags/ign", "*.txt\n")

    git("add", "-A")
    git("add", "-f", "target/out.rs", "target/sub/deep.rs", "logs/app.log", ".env.example", "odd/hid[1].log")
    git("commit", "-q", "-m", "fixture")
    os.chmod(os.path.join(ROOT, "skip/locked.rs"), 0)
    # a directory rg cannot enter, kept out of the repo so it touches only the cases that search it
    os.makedirs(ROOT + "-locked/inner")
    with open(ROOT + "-locked/inner/x.rs", "w") as f:
        f.write("fn x() { NEEDLE_LOCKEDDIR }\n")
    with open(ROOT + "-locked/open.rs", "w") as f:
        f.write("fn open() { NEEDLE_LOCKEDDIR }\n")
    os.chmod(ROOT + "-locked/inner", 0)


main()
