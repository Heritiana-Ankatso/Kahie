"""Make the Debian 'dfsg' Giac source tree buildable with current autotools, library only.

We only build src/libgiac (the CAS core), so:
- configure.ac outputs just Makefile and src/Makefile (dfsg removed docs that it still
  lists; micropython, docs, examples and po are not needed);
- the top Makefile.am only descends into src;
- gettext >= 0.20 removed the intl/ macros: use AM_GNU_GETTEXT([external]);
- config/config.rpath, expected by gettext's macros, is created empty.
Usage: python3 prepare-giac.py <giac-source-dir>
"""
import pathlib
import re
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
print("prepared", root)
