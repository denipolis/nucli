#!/usr/bin/env python3
import json
import pathlib
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8")
args = ["cargo", "metadata", "--format-version", "1", "--locked"]
if len(sys.argv) > 1:
    args += ["--filter-platform", sys.argv[1]]
metadata = json.loads(subprocess.check_output(args))
linked = {node["id"] for node in metadata["resolve"]["nodes"]}
for package in sorted(metadata["packages"], key=lambda item: item["name"]):
    if package["id"] not in linked or package["name"] == "nucli":
        continue
    print("=" * 72)
    print(f'{package["name"]} {package["version"]} ({package.get("license") or "see files"})')
    folder = pathlib.Path(package["manifest_path"]).parent
    for path in sorted(folder.iterdir()):
        upper = path.name.upper()
        if path.is_file() and (upper.startswith("LICEN") or upper.startswith("COPYING") or upper.startswith("NOTICE")):
            print(f"\n--- {path.name}\n")
            print(path.read_text(encoding="utf-8", errors="replace"))
