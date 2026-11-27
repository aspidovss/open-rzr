#!/usr/bin/env python3
"""Release page text for a tag, made from RELEASE_NOTES.en.md and RELEASE_NOTES.md.

    python3 packaging/ci/release-notes.py v0.8.1.5 > body.md
    python3 packaging/ci/release-notes.py --check v0.8.1.5     # tag = version in Cargo.toml?

Takes every section newer than the previous release: the highest `v*` tag below this one
(`git tag`, or `--tags v0.6.10,v0.8.1.1` for a test). English first, Russian folded below.
Exits with an error when the tag does not match the version in Cargo.toml, so that a mistyped
tag (v1.8.1.1 for 0.8.1.1) never becomes a release that every copy of open-rzr would offer as
an update.
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REPO = "https://github.com/aspidovss/open-rzr"


def parts(v):
    """'v0.8.1.5' / '0.8.1+5' / '0.8.1' -> (0, 8, 1, 5)"""
    p = [int(x) for x in re.split(r"[.+]", v.strip().lstrip("vV")) if x.isdigit()]
    return tuple((p + [0, 0, 0, 0])[:4])


def text(v):
    """(0, 8, 1, 0) -> '0.8.1', (0, 8, 1, 5) -> '0.8.1.5'"""
    return ".".join(str(x) for x in (v if v[3] else v[:3]))


def cargo_version():
    m = re.search(r'^version\s*=\s*"([^"]+)"', (ROOT / "Cargo.toml").read_text(encoding="utf-8"), re.M)
    if not m:
        sys.exit("release-notes: no version in Cargo.toml")
    return parts(m.group(1))


def sections(path):
    """[(version, title line, body)] in file order"""
    if not path.exists():
        return []
    s = path.read_text(encoding="utf-8-sig")
    out = []
    for m in re.finditer(r"^## open-rzr (\S+)[^\n]*\n(.*?)(?=^## |\Z)", s, re.M | re.S):
        out.append((parts(m.group(1)), m.group(1), m.group(2).strip()))
    return out


def demote(body):
    """### Heading -> #### Heading (the release page has its own ## and ###)"""
    return re.sub(r"^(#+) ", lambda m: "#" + m.group(1) + " ", body, flags=re.M)


def git_tags():
    try:
        r = subprocess.run(["git", "tag", "-l", "v*"], cwd=ROOT, capture_output=True, text=True, check=True)
        return r.stdout.split()
    except (OSError, subprocess.CalledProcessError):
        return []


def main(argv):
    check = "--check" in argv
    tags = None
    args = []
    i = 0
    while i < len(argv):
        a = argv[i]
        if a == "--tags":
            tags = [t for t in argv[i + 1].split(",") if t]
            i += 2
            continue
        if a != "--check":
            args.append(a)
        i += 1
    if len(args) != 1:
        sys.exit(__doc__)
    ver = parts(args[0])
    cargo = cargo_version()
    if ver != cargo:
        sys.exit(f"release-notes: tag {args[0]} does not match Cargo.toml version {text(cargo)} — tag v{text(cargo)} instead")
    if check:
        print(f"tag {args[0]} = Cargo.toml {text(cargo)}")
        return
    older = [parts(t) for t in (tags if tags is not None else git_tags())]
    older = [t for t in older if t < ver]
    prev = max(older) if older else (0, 0, 0, 0)
    take = lambda secs: [s for s in secs if prev < s[0] <= ver]
    en, ru = take(sections(ROOT / "RELEASE_NOTES.en.md")), take(sections(ROOT / "RELEASE_NOTES.md"))
    v = args[0].lstrip("vV")
    out = [
        f"**open-rzr {v}** — Razer mouse configurator for Windows and Linux, no telemetry, no accounts.",
        "",
        "| | |",
        "|---|---|",
        f"| Windows | `open-rzr-{v}-windows-x64-setup.exe` (installer) or `open-rzr-{v}-windows-x64-portable.zip` |",
        f"| Linux | `.deb`, `.AppImage` or `.tar.gz` (install the udev rule from the program or with `open-rzr-cli install-udev`) |",
        "| Checksums | `SHA256SUMS.txt` |",
        "",
        f"Guide: [English]({REPO}/blob/v{v}/ui/docs/en/getting-started.md) · [Русский]({REPO}/blob/v{v}/ui/docs/ru/getting-started.md)",
        "",
    ]
    if prev != (0, 0, 0, 0):
        out += [f"Changes since {text(prev)}:", ""]
    for _, title, body in en:
        out += [f"## {title}", "", demote(body), ""]
    if ru:
        out += ["<details>", "<summary>Русский: что нового</summary>", ""]
        for _, title, body in ru:
            out += [f"## {title}", "", demote(body), ""]
        out += ["</details>", ""]
    if not en and not ru:
        out += [f"See [RELEASE_NOTES.md]({REPO}/blob/v{v}/RELEASE_NOTES.md).", ""]
    sys.stdout.write("\n".join(out))


if __name__ == "__main__":
    main(sys.argv[1:])
