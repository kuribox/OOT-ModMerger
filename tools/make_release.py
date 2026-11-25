"""Build the release exe and copy it into releases/ with its SHA-256.

    python tools/make_release.py           build (cargo build --release) and copy
    python tools/make_release.py --no-build   copy what target/release already holds
"""

import hashlib
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = os.path.join(ROOT, "target", "release", "oot-mod-merge.exe")
OUT = os.path.join(ROOT, "releases")


def version() -> str:
    with open(os.path.join(ROOT, "crates", "app", "Cargo.toml"), encoding="utf-8") as f:
        for line in f:
            if line.startswith("version"):
                return line.split("=", 1)[1].strip().strip('"')
    return "0.0.0"


def main() -> int:
    if "--no-build" not in sys.argv:
        cargo = shutil.which("cargo")
        if not cargo:
            cargo = os.path.join(os.path.expanduser("~"), ".cargo", "bin", "cargo.exe")
        build = subprocess.run([cargo, "build", "--release"], cwd=ROOT)
        if build.returncode != 0:
            print("the build failed; nothing was copied")
            return build.returncode
    if not os.path.isfile(EXE):
        print(f"{EXE} is missing")
        return 1
    os.makedirs(OUT, exist_ok=True)
    name = f"oot-mod-merge-{version()}.exe"
    target = os.path.join(OUT, name)
    shutil.copy2(EXE, target)
    digest = hashlib.sha256(open(target, "rb").read()).hexdigest()
    with open(os.path.join(OUT, "SHA256SUMS.txt"), "w", encoding="utf-8") as f:
        f.write(f"{digest}  {name}\n")
    size = os.path.getsize(target)
    print(f"releases/{name}  {size:,} bytes  sha256 {digest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
