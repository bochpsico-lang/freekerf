"""JSON Lines over a Unix domain socket or a Windows named pipe."""

from __future__ import annotations

import json
import socket
import sys
from typing import Any, Optional


class Transport:
    def __init__(self, address: str):
        if sys.platform == "win32":
            # a synchronous pipe handle: requests and answers alternate
            self._pipe = open(address, "r+b", buffering=0)  # noqa: SIM115
            self._sock: Optional[socket.socket] = None
        else:
            self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self._sock.connect(address)
            self._pipe = None
        self._buf = b""

    def send(self, msg: Any) -> None:
        data = json.dumps(msg, separators=(",", ":")).encode("utf-8") + b"\n"
        if self._sock is not None:
            self._sock.sendall(data)
        else:
            self._pipe.write(data)

    def recv(self) -> Optional[dict]:
        while b"\n" not in self._buf:
            chunk = self._sock.recv(65536) if self._sock is not None else self._pipe.read(65536)
            if not chunk:
                return None
            self._buf += chunk
        line, self._buf = self._buf.split(b"\n", 1)
        if not line.strip():
            return self.recv()
        return json.loads(line)

    def close(self) -> None:
        try:
            self.send({"bye": True})
        except OSError:
            pass
        if self._sock is not None:
            self._sock.close()
        else:
            self._pipe.close()
