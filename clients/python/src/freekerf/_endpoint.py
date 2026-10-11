"""Where the running FreeKerf listens (mirrors `freekerf_platform::Dirs::runtime` and
`freekerf_ipc::Endpoint::in_dir`)."""

from __future__ import annotations

import os
import sys
from pathlib import Path
from typing import Optional, Union


def runtime_dir(home: Optional[Union[str, os.PathLike]] = None) -> Path:
    """The instance's runtime directory: `home` (FreeKerf's `--home`), else
    `$FREEKERF_HOME`, else the per-user directory."""
    if home is not None:
        return Path(os.fspath(home))
    env = os.environ.get("FREEKERF_HOME")
    if env:
        return Path(env)
    if sys.platform == "win32":
        return Path(os.environ["APPDATA"]) / "FreeKerf" / "freekerf" / "data"
    if sys.platform == "darwin":
        return Path.home() / "Library" / "Application Support" / "org.FreeKerf.freekerf"
    xdg_runtime = os.environ.get("XDG_RUNTIME_DIR")
    if xdg_runtime and os.path.isabs(xdg_runtime):
        return Path(xdg_runtime) / "freekerf"
    data = os.environ.get("XDG_DATA_HOME")
    base = Path(data) if data and os.path.isabs(data) else Path.home() / ".local" / "share"
    return base / "freekerf"


def _fnv1a(text: str) -> int:
    h = 0xCBF29CE484222325
    for b in text.encode("utf-8"):
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


class Endpoint:
    """The socket (or named pipe) and the token file of an instance."""

    def __init__(self, runtime: Path):
        self.dir = runtime / "ipc"
        if sys.platform == "win32":
            self.address = r"\\.\pipe\freekerf-%016x" % _fnv1a(str(runtime))
        else:
            self.address = str(self.dir / "ipc.sock")
        self.token_file = self.dir / "ipc.token"

    def read_token(self) -> str:
        token = self.token_file.read_text(encoding="utf-8").strip()
        if not token:
            raise OSError("the instance token is empty")
        return token
