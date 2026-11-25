"""Create the 0.0.1-alpha GitHub release and attach the exe.

Needs a token with `repo`: it is read from git's own credential store for github.com, so nothing is
typed or printed here.

    python tools/publish_release.py            create the release for the current tag, upload the exe
    python tools/publish_release.py --dry-run  say what it would do
"""

import json
import os
import subprocess
import sys
import urllib.request

OWNER = "kuribox"
REPO = "OOT-ModMerger"
TAG = "v0.0.1-alpha"

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = os.path.join(ROOT, "releases", "oot-mod-merge-0.0.1-alpha.exe")
NOTES = os.path.join(ROOT, "docs", "RELEASE-0.0.1-alpha.md")


def token() -> str:
    out = subprocess.run(
        ["git", "credential", "fill"],
        input="protocol=https\nhost=github.com\n\n",
        capture_output=True,
        text=True,
    )
    for line in out.stdout.splitlines():
        if line.startswith("password="):
            return line.split("=", 1)[1]
    raise SystemExit("no github credential in git's store")


def api(url: str, tok: str, data: bytes | None = None, method: str = "GET", ctype="application/json"):
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"Bearer {tok}")
    req.add_header("Accept", "application/vnd.github+json")
    req.add_header("X-GitHub-Api-Version", "2022-11-28")
    if data is not None:
        req.add_header("Content-Type", ctype)
    with urllib.request.urlopen(req) as r:
        return json.loads(r.read().decode() or "{}")


def main() -> int:
    tok = token()
    body = open(NOTES, encoding="utf-8").read()
    if "--dry-run" in sys.argv:
        print(f"would release {TAG} on {OWNER}/{REPO} with {os.path.basename(EXE)} ({os.path.getsize(EXE):,} bytes)")
        return 0

    try:
        rel = api(
            f"https://api.github.com/repos/{OWNER}/{REPO}/releases",
            tok,
            json.dumps(
                {
                    "tag_name": TAG,
                    "name": "0.0.1-alpha",
                    "body": body,
                    "draft": False,
                    "prerelease": True,
                    "target_commitish": "main",
                }
            ).encode(),
            "POST",
        )
    except urllib.error.HTTPError as e:
        detail = e.read().decode()
        if e.code == 422 and "already_exists" in detail:
            rel = api(f"https://api.github.com/repos/{OWNER}/{REPO}/releases/tags/{TAG}", tok)
        else:
            raise SystemExit(f"creating the release failed: {e.code} {detail}")

    print(f"release: {rel['html_url']}")
    name = os.path.basename(EXE)
    # An asset with the same name cannot be uploaded twice: drop the old one first, so re-running
    # this after a fix refreshes the download instead of failing.
    for a in rel.get("assets", []):
        if a["name"] == name:
            api(f"https://api.github.com/repos/{OWNER}/{REPO}/releases/assets/{a['id']}", tok, method="DELETE")
            print(f"removed the old {name}")
    payload = open(EXE, "rb").read()
    asset = api(
        f"https://uploads.github.com/repos/{OWNER}/{REPO}/releases/{rel['id']}/assets?name={name}",
        tok,
        payload,
        "POST",
        "application/octet-stream",
    )
    print(f"asset: {asset['name']} {asset['size']:,} bytes ({asset['browser_download_url']})")
    print(f"digest: {asset.get('digest')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
