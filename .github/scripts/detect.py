#!/usr/bin/env python3
"""Compute which service images must be rebuilt for a release.

Usage:
  detect.py <cargo-metadata.json> <changed-files.txt> [<old Cargo.lock> <new Cargo.lock>]
  detect.py <cargo-metadata.json> --all

cargo-metadata.json comes from `cargo metadata --format-version 1 --locked`
run in mazbri-bin at HEAD. The dependency closure is computed from that graph
(normal + build deps, dev-deps ignored), so the rules follow Cargo.toml rather
than a hardcoded table. Errs on the side of rebuilding more, never fewer.

Prints {"build": [...], "reuse": [...]} where each entry is
{"package", "image", "bin", "bootstrap"}; reasons go to stderr.
"""
import json
import os
import re
import sys

# cargo package -> (image name, bootstrap prefix). Binary name == package name.
SERVICES = {
    "mazbri-analytics": ("mazbri-analytics", "analytics"),
    "mazbri-auth-service": ("mazbri-auth", "auth"),
    "mazbri-billing-service": ("mazbri-billing", "billing"),
    "mazbri-bypass": ("mazbri-bypass", "bypass"),
    "mazbri-cache-service": ("mazbri-cache", "cache"),
    "mazbri-cbpro-ext-service": ("mazbri-cbpro-ext", "cbpro-ext"),
    "mazbri-cbpro-service": ("mazbri-cbpro", "cbpro"),
    "mazbri-cert-service": ("mazbri-cert", "cert"),
    "mazbri-gateway": ("mazbri-gateway", "gateway"),
    "mazbri-mgmt-api-v2": ("mazbri-mgmt", "mgmt"),
    "mazbri-ratelimit-service": ("mazbri-ratelimit", "ratelimit"),
}
# Any change here rebuilds every image.
GLOBAL = [
    r"^mazbri-bin/Cargo\.toml$",
    r"^mazbri-libs/Cargo\.toml$",
    r"rust-toolchain",
    r"(^|/)\.cargo/",
    r"^docker/",
    r"^\.github/workflows/release",
    r"^\.github/scripts/",
]
# Never affects a release binary (Cargo.lock is handled by the lock diff).
IGNORE = [r"\.md$", r"/tests?/", r"/benches/", r"/examples/", r"(^|/)Cargo\.lock$"]


def entry(pkg):
    image, boot = SERVICES[pkg]
    return {"package": pkg, "image": image, "bin": pkg, "bootstrap": f"{boot}-bootstrap.toml"}


def main():
    meta = json.load(open(sys.argv[1]))
    pk = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    repo = os.path.dirname(meta["workspace_root"])  # .../<repo>/mazbri-bin -> <repo>
    members = {pk[i]["name"]: i for i in meta["workspace_members"]}
    missing = set(SERVICES) - set(members)
    extra = set(members) - set(SERVICES)
    if missing or extra:
        sys.exit(f"SERVICES table out of sync with workspace: missing={missing} extra={extra}")

    def closure(pid):
        seen, stack = {pid}, [pid]
        while stack:
            for d in nodes[stack.pop()]["deps"]:
                if any(k["kind"] in (None, "build") for k in d["dep_kinds"]) and d["pkg"] not in seen:
                    seen.add(d["pkg"])
                    stack.append(d["pkg"])
        return seen

    deps = {s: closure(i) for s, i in members.items()}
    local_dir = {i: os.path.relpath(os.path.dirname(p["manifest_path"]), repo)
                 for i, p in pk.items() if p["source"] is None}

    hit, why = set(), {}

    def add(s, reason):
        hit.add(s)
        why.setdefault(s, []).append(reason)

    if sys.argv[2] == "--all":
        for s in SERVICES:
            add(s, "forced")
    else:
        files = [l.strip() for l in open(sys.argv[2]) if l.strip()]
        for f in files:
            if any(re.search(g, f) for g in GLOBAL):
                for s in SERVICES:
                    add(s, f"global:{f}")
                continue
            if f == "mazbri-license.pub":  # read by mazbri-cert/build.rs
                for s, cl in deps.items():
                    if any(pk[x]["name"] == "mazbri-cert" for x in cl):
                        add(s, f)
                continue
            if f.startswith("mazbri-web/"):  # embedded by mgmt build.rs
                add("mazbri-mgmt-api-v2", f)
                continue
            if any(re.search(g, f) for g in IGNORE):
                continue
            for i, d in local_dir.items():
                if f.startswith(d + "/"):
                    for s, cl in deps.items():
                        if i in cl:
                            add(s, f"{pk[i]['name']}:{f}")

        if len(sys.argv) >= 5:  # third-party crate changes via Cargo.lock
            def parse(p):
                txt = open(p).read() if os.path.exists(p) else ""
                out = {}
                for blk in txt.split("[[package]]")[1:]:
                    n = re.search(r'name = "([^"]+)"', blk).group(1)
                    out.setdefault(n, set()).add(blk.strip())
                return out
            a, b = parse(sys.argv[3]), parse(sys.argv[4])
            changed = {n for n in set(a) | set(b) if a.get(n) != b.get(n)}
            for s, cl in deps.items():
                c = {pk[x]["name"] for x in cl} & changed
                if c:
                    add(s, "Cargo.lock:" + ",".join(sorted(c))[:120])

    for s in sorted(hit):
        print(f"build {s}: {why[s][:3]}", file=sys.stderr)
    print(json.dumps({
        "build": [entry(s) for s in sorted(hit)],
        "reuse": [entry(s) for s in sorted(set(SERVICES) - hit)],
    }))


if __name__ == "__main__":
    main()
