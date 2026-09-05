#!/usr/bin/env python3
"""Assert distroless facilitator image filesystem mode from `docker export` stdin.

Export member names have no leading slash. TarInfo.mode includes type bits;
compare with stat.S_IMODE.
"""

from __future__ import annotations

import stat
import sys
import tarfile

UID = 65532
DIR_MODE = 0o755


def fail(msg: str) -> None:
    print(f"image-fs-assert: {msg}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    wanted = {
        "etc/facilitator": "dir",
        "etc/facilitator/config.toml": "file",
        "usr/bin/facilitator": "bin",
    }
    found: dict[str, tarfile.TarInfo] = {}
    with tarfile.open(fileobj=sys.stdin.buffer, mode="r|") as tar:
        for ti in tar:
            name = ti.name.lstrip("./")
            if name.endswith("/") and name[:-1] in wanted:
                name = name[:-1]
            if name in wanted:
                found[name] = ti

    for path, kind in wanted.items():
        ti = found.get(path)
        if ti is None:
            fail(f"missing {path}")
        mode = stat.S_IMODE(ti.mode)
        if ti.uid != UID or ti.gid != UID:
            fail(f"{path} uid:gid {ti.uid}:{ti.gid}, want {UID}:{UID}")
        if kind == "dir":
            if not ti.isdir():
                fail(f"{path} is not a directory")
            if mode != DIR_MODE:
                fail(f"{path} mode {oct(mode)}, want {oct(DIR_MODE)}")
        elif kind == "file":
            if not ti.isreg():
                fail(f"{path} is not a regular file")
            if mode & 0o400 == 0:
                fail(f"{path} not owner-readable ({oct(mode)})")
        else:
            if not ti.isreg():
                fail(f"{path} is not a regular file")
            if mode & 0o100 == 0:
                fail(f"{path} not owner-executable ({oct(mode)})")

    print("image-fs-assert: ok")


if __name__ == "__main__":
    main()
