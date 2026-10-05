#!/usr/bin/env python3
# The packages' changelog, from git history: every commit that changed a package (its
# packaging under pmaports/, or the sources pmaports/sync.sh packs into it), bundled by the
# package version it went into (pkgver-rpkgrel at that commit: several commits can make up one
# version, and a commit can be in several packages). Newest first, by the day of each
# version's latest change. Published next to the apk repository as changes.html by
# publish-packages.sh. Needs the full history (fetch-depth: 0).
# usage: changelog.py OUT.html
import html
import os
import re
import subprocess
import sys

REPO = os.environ.get("GITHUB_REPOSITORY", "artillect/l16-linux")
# what each of our programs is built from, beside its pmaports folder (as pmaports/sync.sh
# packs them)
SOURCES = {
    "l16-camera": ["l16-camera/", "tools/l16-shoot", "tools/l16-lri-assemble"],
    "glycin-lri": ["glycin-lri/"],
    "l16-gallery": ["l16-gallery/", "l16-camera/src/icons.rs", "glycin-lri/src/lri.rs"],
    "l16-phosh-plugins": ["l16-phosh/"],
    "l16-gnss": ["l16-gnss/"],
    "l16-render": ["l16-render/"],
    "linux-light-lfc": ["kernel/"],
}


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


# each package's APKBUILD (as named in the tree today) and the paths that make it
packages = []
for apkbuild in git("ls-files", "pmaports").split():
    if apkbuild.endswith("/APKBUILD"):
        folder = apkbuild[: -len("APKBUILD")]
        name = folder.rstrip("/").rsplit("/", 1)[-1]
        packages.append((apkbuild, [folder] + SOURCES.get(name, [])))
paths = sorted({p for _, ps in packages for p in ps})

# (package, version): its day (its latest change's) and its changes, newest first
bundles = {}
log = git("log", "--format=%x00%H%x09%ad%x09%s", "--date=short", "--name-only", "--", *paths)
for entry in log.split("\0")[1:]:
    head, *files = entry.strip().split("\n")
    commit, day, subject = head.split("\t", 2)
    files = [f for f in files if f]
    for apkbuild, ps in packages:
        if any(f.startswith(p) if p.endswith("/") else f == p for f in files for p in ps):
            v = version(commit, apkbuild)
            if v:
                bundles.setdefault(v, (day, []))[1].append((commit, subject))

# by day, newest first (the log is newest first, so the bundles are in that order already)
days = {}
for (name, ver), (day, changes) in bundles.items():
    days.setdefault(day, []).append((name, ver, changes))

out = ["""<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>l16-linux changes</title>
<style>
h2 { border-top: 2px solid #ccc; padding-top: 0.8em; margin-top: 2em }
h3 { font-size: 1em; margin-bottom: 0.3em }
ul { margin-top: 0 }
</style>
<body style="font-family: sans-serif; max-width: 40em; margin: 2em auto; padding: 0 16px">
<h1>l16-linux changes</h1>
<p>What changed in each package update, newest first.
<code>sudo apk upgrade</code> on the camera installs them. <a href="./">The package repository</a></p>
"""]
for day in sorted(days, reverse=True):
    out.append(f"<h2>{day}</h2>\n")
    for name, ver, changes in days[day]:
        out.append(f"<h3>{html.escape(name)} {html.escape(ver)}</h3>\n<ul>\n")
        for commit, subject in changes:
            out.append(f'<li><a href="https://github.com/{REPO}/commit/{commit}">'
                       f"{html.escape(subject)}</a></li>\n")
        out.append("</ul>\n")
out.append("</body>\n</html>\n")
with open(sys.argv[1], "w") as f:
    f.write("".join(out))
print(f"changelog: {len(bundles)} package versions over {len(days)} days")
