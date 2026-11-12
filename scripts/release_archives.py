"""Create a Howff installer archive from a checked executable."""

import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile


def write(output, release, binary, *, version, target, profile, kernel_sha256):
    output, release = Path(output), Path(release)
    output.mkdir(parents=True, exist_ok=True)
    digest = hashlib.sha256(binary).hexdigest()
    manifest = json.dumps(
        {
            "product": "howff",
            "schema": 1,
            "update_protocol": 1,
            "version": version,
            "target": target,
            "profile": profile,
            "kernel_sha256": kernel_sha256,
            "executable_sha256": digest,
        },
        separators=(",", ":"),
    ).encode()
    entries = {
        "howff": (binary, 0o755),
        "Install Howff.sh": ((release / "Install Howff.sh").read_bytes(), 0o755),
        "README.md": ((release / "Howff README.md").read_bytes(), 0o644),
        "SHA256SUMS": ((digest + "  howff\n").encode(), 0o644),
        "manifest.json": (manifest, 0o644),
    }
    path = output / f"howff-{version}-arm64.tar.gz"
    with path.open("wb") as stream:
        with gzip.GzipFile(fileobj=stream, mode="wb", mtime=0, filename="") as packed:
            with tarfile.open(fileobj=packed, mode="w") as package:
                for name, (data, mode) in entries.items():
                    entry = tarfile.TarInfo(f"howff/{name}")
                    entry.size = len(data)
                    entry.mode = mode
                    entry.mtime = 0
                    package.addfile(entry, io.BytesIO(data))
    return {
        "archive": str(path),
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "bytes": path.stat().st_size,
        "executable_sha256": digest,
    }
