#!/usr/bin/env python3
"""Preloads ONNX Runtime for ort-sys, so builds never reach cdn.pyke.io.

ort-sys (download-binaries) fetches a prebuilt ONNX Runtime from pyke's CDN
while it compiles, and skips the download when
`$ORT_CACHE_DIR/dfbin/<target>/<sha256>` already exists. GitHub's runners are
blocked by that CDN (https://github.com/pykeio/ort/issues/672), so CI takes
the same archives from this repository's `deps-ort-ms-1.28.0` release,
checks them against the SHA-256 that ort-sys pins in its `dist.tsv`, and
extracts them there.

Usage: fetch-ort.py <target> <cache dir>
"""

import hashlib
import lzma
import os
import shutil
import sys
import tarfile
import tempfile
import urllib.request

MIRROR = os.environ.get(
    "ORT_MIRROR", "https://github.com/Sebasalazaro/redactor/releases/download/deps-ort-ms-1.28.0"
)

# From ort-sys 2.0.0-rc.13, build/download/dist.tsv. Update both together.
DISTS = {
    "x86_64-pc-windows-msvc": (
        "x86_64-pc-windows-msvc+directml.tar.lzma2",
        "f7c654b3729cb9e5ad2a36a0c38e5b48e63bf4eed22968931aed33a0ad0b527d",
    ),
    "x86_64-unknown-linux-gnu": (
        "x86_64-unknown-linux-gnu.tar.lzma2",
        "e454f710f8a49f53aa5b4ff51e3454ae1835777e431c6c35c5255ce6f205fd68",
    ),
    "aarch64-apple-darwin": (
        "aarch64-apple-darwin+coreml.tar.lzma2",
        "6934874e2e953576d9c1db47ff1af39c62c4f4220dbe6f988e131f72879674c7",
    ),
}


def main() -> None:
    target, cache_dir = sys.argv[1], sys.argv[2]
    name, expected = DISTS[target]
    dest = os.path.join(cache_dir, "dfbin", target, expected)
    if os.path.isdir(dest):
        print(f"ONNX Runtime for {target} already in {dest}")
        return

    os.makedirs(os.path.dirname(dest), exist_ok=True)
    work = tempfile.mkdtemp(dir=os.path.dirname(dest))
    archive = os.path.join(work, name)
    with urllib.request.urlopen(f"{MIRROR}/{name}", timeout=300) as r, open(archive, "wb") as f:
        shutil.copyfileobj(r, f)

    digest = hashlib.sha256()
    with open(archive, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    if digest.hexdigest() != expected:
        sys.exit(f"{name}: SHA-256 {digest.hexdigest()} does not match the pinned {expected}")

    # A tar stream compressed as raw LZMA2 with a 64 MiB dictionary, as
    # ort-sys reads it (build/download/extract.rs).
    out = os.path.join(work, "out")
    filters = [{"id": lzma.FILTER_LZMA2, "dict_size": 1 << 26}]
    with lzma.LZMAFile(archive, format=lzma.FORMAT_RAW, filters=filters) as raw:
        with tarfile.open(fileobj=raw, mode="r|") as tar:
            tar.extractall(out, filter="data")
    os.replace(out, dest)
    shutil.rmtree(work, ignore_errors=True)
    print(f"ONNX Runtime for {target} ready in {dest}")


if __name__ == "__main__":
    main()
