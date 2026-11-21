"""Make the Debian 'dfsg' Giac source tree buildable with current autotools, library only.

We only build src/libgiac (the CAS core), so:
- configure.ac outputs just Makefile and src/Makefile (dfsg removed docs that it still
  lists; micropython, docs, examples and po are not needed);
- the top Makefile.am only descends into src;
- gettext >= 0.20 removed the intl/ macros: use AM_GNU_GETTEXT([external]);
- config/config.rpath, expected by gettext's macros, is created empty;
- the parser and lexer (stripped from dfsg) are generated with bison/flex up front: the
  Makefile's ylwrap rule mangles the output names and parallel make races on the header;
- the built-in command help (5 languages, ~1 MB of strings) is replaced by the one empty
  entry Giac itself uses on calculators: Kahie never shows Giac's help.
Usage: python3 prepare-giac.py <giac-source-dir>
"""
import pathlib
import re
import shutil
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
ac = root / "configure.ac"
s = ac.read_text()
s = re.sub(r"^(\s*)AM_GNU_GETTEXT\s*$", r"\1AM_GNU_GETTEXT([external])", s, flags=re.M)
s, n = re.subn(r"AC_OUTPUT\((.*?)\)", "AC_OUTPUT(Makefile src/Makefile)", s, count=1, flags=re.S)
assert n == 1, "AC_OUTPUT not found"
ac.write_text(s)

mam = root / "Makefile.am"
t = mam.read_text()
t = re.sub(r"^SUBDIRS\s*=.*$", "SUBDIRS = src", t, flags=re.M)
t = re.sub(r"^DIST_SUBDIRS\s*=.*$", "DIST_SUBDIRS = src", t, flags=re.M)
mam.write_text(t)

(root / "config").mkdir(exist_ok=True)
(root / "config" / "config.rpath").touch()
src = root / "src"
# Bison 3 dropped YYLEX_PARAM, which the grammar relies on: declare the scanner argument.
yy = src / "input_parser.yy"
g = yy.read_text()
if "%lex-param" not in g:
    g = g.replace("%parse-param {void * scanner}", "%parse-param {void * scanner}\n%lex-param {void * scanner}", 1)
    assert "%lex-param" in g, "parse-param line not found"
    yy.write_text(g)
subprocess.run(["bison", "-p", "giac_yy", "-d", "-o", "input_parser.cc", "input_parser.yy"], cwd=src, check=True,
               stderr=subprocess.DEVNULL)
shutil.copy(src / "input_parser.hh", src / "input_parser.h")
subprocess.run(["flex", "input_lexer.ll"], cwd=src, check=True)
(src / "static_help.h").write_text('{ "", { "", "", "", "",""}, "", "", "" },\n')
for f in ("input_parser.cc", "input_parser.h", "input_lexer.cc", "lexer.h"):
    assert (src / f).exists(), f
print("prepared", root)
