#!/usr/bin/env python3
"""Build Kestrel's mod registry `index.json` from the Kestrel-mods releases.

The launcher reads this file to browse and install mods (schema 1, see
`docs/api/mod-index.md` in the Horizon tree: `horizon_mods::index`). Each
entry lists one version — the mod's latest GitHub release — with the `.kmod`
asset's size and SHA-256, so the launcher can check the download before it
installs it.

What is listed: the 10 user-facing ported mods. The three probes
(`world_probe`, `inventory_probe`, `entity_probe`) are diagnostics, not user
mods, and are deliberately excluded.

Signatures: none. The index leaves `key` unset, so entries are unsigned and
the launcher checks them by SHA-256 alone (`Trust::Unsigned` in
`horizon_mods::index::check_file`). That is what the schema allows for a
project with no key; a signing key can be added later without a schema change.

Page fields (name, summary, description, authors, license) come from the
mod's current source manifest in `mods/<dir>/horizon-mod.toml` — the page
may be newer than the file it pins. The file-matching fields (id, version,
api, side, capabilities) come from the release asset's own manifest, and the
script refuses to run if the two disagree on any of them.

Usage (from the repo root):
    python3 tools/build_index.py [--out index.json]

Requires the `gh` CLI (authenticated; in CI the default `GITHUB_TOKEN`).
"""

import hashlib
import json
import re
import subprocess
import sys
import tempfile
import tomllib
import zipfile
from datetime import datetime, timezone
from pathlib import Path

REPO = "NepThunder/Kestrel-mods"
GAME = "26.3"
FILE_HOST = "https://github.com/NepThunder/Kestrel-mods/releases/download/"
MAX_CHANGELOG = 6000
MAX_SUMMARY = 160

# The user-facing ported mods, by manifest id. Probes are excluded on purpose.
ALLOW_IDS = {
    "coords_hud",
    "classic_zoom",
    "customhud",
    "jei",
    "armor_status",
    "armorhud",
    "held_item_info",
    "appleskin",
    "blockscope",
    "jade",
}


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, text=True, capture_output=True, **kw)


def gh(*args):
    return run(["gh", *args]).stdout


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def sanitize_mentions(text):
    """A bare @word in rendered markdown makes GitHub list that name as a
    contributor. Wrap mentions in backticks (skipping text already in code
    spans) so they stay literal."""
    out = []
    in_code = False
    for part in re.split(r"(`+)", text):
        if part.startswith("`"):
            in_code = not in_code
            out.append(part)
        elif in_code:
            out.append(part)
        else:
            out.append(re.sub(r"@([A-Za-z0-9_][A-Za-z0-9_.-]*)", r"`@\1`", part))
    return "".join(out)


def summary_of(description):
    """First sentence of the description, at most 160 characters. A first
    sentence that runs long is cut at its first ':' or ';' when that leaves
    a usable line, else at the last space inside the limit."""
    first = re.split(r"\.\s", description.strip(), maxsplit=1)[0]
    if not first.endswith("."):
        first += "."
    if len(first) > MAX_SUMMARY:
        for sep in (":", ";"):
            head = first.split(sep, 1)[0]
            if len(head) >= 40:
                return head.strip() + "."
        cut = first[: MAX_SUMMARY - 1].rsplit(" ", 1)[0]
        return cut.rstrip(",;") + "…"
    return first


def repo_root():
    return Path(run(["git", "rev-parse", "--show-toplevel"]).stdout.strip())


def mod_dirs(root):
    """manifest id -> (mod dir, license). License comes from the mod's
    Cargo.toml when it names one, else the workspace default (MIT)."""
    found = {}
    for manifest in sorted((root / "mods").glob("*/horizon-mod.toml")):
        data = tomllib.loads(manifest.read_text())
        cargo = manifest.parent / "Cargo.toml"
        text = cargo.read_text() if cargo.exists() else ""
        m = re.search(r'^license\s*=\s*"([^"]+)"', text, re.M)
        found[data["id"]] = (manifest.parent, m.group(1) if m else "MIT")
    return found


def main():
    root = repo_root()
    out = Path(sys.argv[sys.argv.index("--out") + 1]) if "--out" in sys.argv else root / "index.json"
    dirs = mod_dirs(root)

    releases = json.loads(
        gh(
            "release", "list", "--repo", REPO, "--limit", "100",
            "--json", "tagName,publishedAt,isDraft,isPrerelease",
        )
    )
    projects = {}
    with tempfile.TemporaryDirectory() as tmp:
        for rel in releases:
            if rel["isDraft"] or rel["isPrerelease"]:
                continue
            tag = rel["tagName"]
            dest = Path(tmp) / tag
            dest.mkdir()
            dl = subprocess.run(
                ["gh", "release", "download", tag, "--repo", REPO,
                 "--pattern", "*.kmod", "--dir", str(dest)],
                capture_output=True, text=True,
            )
            if dl.returncode != 0:
                continue
            assets = sorted(dest.glob("*.kmod"))
            if len(assets) != 1:
                print(f"warning: {tag}: {len(assets)} .kmod assets, skipped", file=sys.stderr)
                continue
            asset = assets[0]
            with zipfile.ZipFile(asset) as z:
                file_manifest = tomllib.loads(z.read("horizon-mod.toml").decode())
            mod_id = file_manifest["id"]
            if mod_id not in ALLOW_IDS or mod_id in projects:
                continue
            if mod_id not in dirs:
                print(f"error: {mod_id}: no mods/<dir> for it", file=sys.stderr)
                return 1
            mod_dir, license = dirs[mod_id]
            src_manifest = tomllib.loads((mod_dir / "horizon-mod.toml").read_text())
            for field in ("id", "version", "api", "side", "capabilities"):
                if file_manifest.get(field) != src_manifest.get(field):
                    print(
                        f"error: {mod_id}: asset and source disagree on {field}",
                        file=sys.stderr,
                    )
                    return 1
            changelog = sanitize_mentions(
                json.loads(
                    gh("release", "view", tag, "--repo", REPO, "--json", "body")
                )["body"]
                or ""
            ).strip()[:MAX_CHANGELOG]
            description = src_manifest["description"]
            projects[mod_id] = {
                "id": mod_id,
                "kind": "mod",
                "name": src_manifest["name"],
                "summary": summary_of(description),
                "description": description,
                "authors": src_manifest["authors"],
                "owners": ["NepThunder"],
                "license": license,
                "source": f"https://github.com/NepThunder/Kestrel-mods/tree/main/mods/{mod_dir.name}",
                "versions": [
                    {
                        "version": file_manifest["version"],
                        "published": rel["publishedAt"][:10],
                        "changelog": changelog,
                        "api": file_manifest["api"],
                        "side": file_manifest["side"],
                        "capabilities": file_manifest["capabilities"],
                        "game": GAME,
                        "file": {
                            "name": asset.name,
                            "url": f"{FILE_HOST}{tag}/{asset.name}",
                            "size": asset.stat().st_size,
                            "sha256": sha256_file(asset),
                        },
                    }
                ],
            }

    missing = sorted(ALLOW_IDS - set(projects))
    if missing:
        print(f"error: no release found for: {', '.join(missing)}", file=sys.stderr)
        return 1

    index = {
        "schema": 1,
        "generated": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "projects": [projects[mid] for mid in sorted(projects)],
        "revoked": [],
    }
    out.write_text(json.dumps(index, indent=2, ensure_ascii=False) + "\n")
    print(f"Wrote {out}: {len(index['projects'])} project(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
