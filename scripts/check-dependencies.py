"""Check public crates.io dependency versions against OSV without uploading sources."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys
import tomllib
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    lock_bytes = (root / "Cargo.lock").read_bytes()
    packages = []
    for package in tomllib.loads(lock_bytes.decode())["package"]:
        source = package.get("source")
        if source is None:
            if package["name"] != "mio-note":
                raise ValueError("Manual review required for local dependency: " + package["name"])
            continue
        if source != "registry+https://github.com/rust-lang/crates.io-index":
            raise ValueError("Manual review required for dependency source: " + source)
        packages.append(package)
    if not packages:
        raise ValueError("No public registry dependencies found")
    request = urllib.request.Request(
        "https://api.osv.dev/v1/querybatch",
        data=json.dumps({"queries": [
            {"package": {"name": p["name"], "ecosystem": "crates.io"},
             "version": p["version"]} for p in packages
        ]}).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=45) as response:
        payload = response.read(4 * 1024 * 1024 + 1)
        if len(payload) > 4 * 1024 * 1024:
            raise ValueError("Advisory response exceeded the limit")
    results = json.loads(payload)["results"]
    if len(results) != len(packages):
        raise ValueError("Incomplete advisory response")
    findings = [
        {"name": p["name"], "version": p["version"], "vulns": r["vulns"]}
        for p, r in zip(packages, results) if r.get("vulns")
    ]
    report = {
        "checked_at_utc": datetime.now(timezone.utc).isoformat(),
        "source": "https://api.osv.dev/v1/querybatch",
        "cargo_lock_sha256": hashlib.sha256(lock_bytes).hexdigest(),
        "packages_checked": len(packages),
        "findings": findings,
    }
    text = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    print(text, end="")
    return 1 if findings else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, urllib.error.URLError) as error:
        print("Dependency check unavailable: " + str(error), file=sys.stderr)
        sys.exit(2)
