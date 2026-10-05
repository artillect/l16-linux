#!/usr/bin/env python3
# The packages' changelog, from git history: every commit that changed a package's
# version (pkgver or pkgrel in an APKBUILD under pmaports/), newest first, by day, with
# its subject line and the new versions. Published next to the apk repository as
# changes.html by publish-packages.sh. Needs the full history (fetch-depth: 0).
# usage: changelog.py OUT.html
import html
import os
import re
import subprocess
import sys

REPO = os.environ.get("GITHUB_REPOSITORY", "artillect/l16-linux")


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True).stdout


def version(commit, path):
    """pkgname and pkgver-rpkgrel of an APKBUILD at a commit (None if it isn't there)"""
    text = git("show", f"{commit}:{path}")
    vars = dict(re.findall(r"^(\w+)=\"?([^\"\s]*)\"?", text, re.M))
    if "pkgname" not in vars or "pkgver" not in vars:
        return None
    expand = lambda v: re.sub(r"\$\{?(\w+)\}?", lambda m: vars.get(m.group(1), ""), v)
    return expand(vars["pkgname"]), f"{expand(vars['pkgver'])}-r{vars.get('pkgrel', '0')}"


days = {}
log = git("log", "--format=%H%x09%ad%x09%s", "--date=short", "--", "pmaports")
for line in log.splitlines():
    commit, day, subject = line.split("\t", 2)
    paths = git("diff-tree", "--no-commit-id", "--name-only", "-r", "--root", commit,
                "--", "pmaports").split()
    bumped = []
    for path in paths:
        if not path.endswith("/APKBUILD"):
            continue
        now = version(commit, path)
        if now and now != version(commit + "^", path):
            bumped.append(now)
    if bumped:
        days.setdefault(day, []).append((commit, subject, sorted(bumped)))

out = ["""<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>l16-linux changes</title>
<body style="font-family: sans-serif; max-width: 40em; margin: 2em auto; padding: 0 16px">
<h1>l16-linux changes</h1>
<p>What each package update changed, newest first. <code>sudo apk upgrade</code> on the
camera installs them. <a href="./">The package repository</a></p>
"""]
for day, entries in days.items():
    out.append(f"<h2>{day}</h2>\n<ul>\n")
    for commit, subject, bumped in entries:
        versions = ", ".join(f"{html.escape(n)} {html.escape(v)}" for n, v in bumped)
        out.append(f'<li><a href="https://github.com/{REPO}/commit/{commit}">'
                   f"{html.escape(subject)}</a><br><small>{versions}</small></li>\n")
    out.append("</ul>\n")
out.append("</body>\n</html>\n")
with open(sys.argv[1], "w") as f:
    f.write("".join(out))
print(f"changelog: {sum(len(e) for e in days.values())} updates over {len(days)} days")
