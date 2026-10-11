"""Drive a running FreeKerf from Python.

FreeKerf (``freekerf serve --headless``, later the desktop app) listens on a local
endpoint; this client attaches to it and calls the same actions as the UI, scripts
and agents. The API is built from the app's action registry when you connect::

    import freekerf

    with freekerf.connect(name="notebook") as app:
        app.document.import_file(path="logo.svg")      # action document.import
        app.document.select_all()
        app.document.translate(dx=10, dy=0)
        print(app.state("session.document"))
        report = app.run_script("app.document.selectAll()", dry_run=True)

Every call is checked by the app against your tier (``edit`` by default, ``machine``
with ``automation.python.allow_machine``); actions that can turn the laser on need a
person to confirm them in the app and are refused by a headless instance.
"""

from __future__ import annotations

import keyword
import os
import threading
import time
from typing import Any, Dict, Iterator, List, Optional, Union

from ._endpoint import Endpoint, runtime_dir
from ._transport import Transport

__all__ = ["App", "FreeKerfError", "PermissionDenied", "Refused", "connect", "runtime_dir"]
__version__ = "0.1.0"

PROTOCOL_VERSION = 1


class FreeKerfError(Exception):
    """An error answered by the app (``kind`` is machine-readable)."""

    def __init__(self, kind: str, message: str):
        super().__init__(message)
        self.kind = kind
        self.message = message


class PermissionDenied(FreeKerfError):
    """The action needs a higher tier than this client has."""


class Refused(FreeKerfError):
    """The app refused the action (for example a laser action without a person)."""


def _error(e: Dict[str, Any]) -> FreeKerfError:
    kind = str(e.get("kind", "error"))
    message = str(e.get("message", "unknown error"))
    cls = {"permission_denied": PermissionDenied, "refused": Refused}.get(kind, FreeKerfError)
    return cls(kind, message)


def _method_name(action: str) -> str:
    """The Python method of an action: ``document.import`` is ``import_file`` (a
    keyword); other keywords get a trailing underscore."""
    if action == "import":
        return "import_file"
    if keyword.iskeyword(action):
        return action + "_"
    return action


class _Namespace:
    """``app.document``: one method per action of the namespace."""

    def __init__(self, app: "App", name: str, actions: Dict[str, dict]):
        self._app = app
        self._name = name
        self._actions = actions  # method name → spec

    def __getattr__(self, method: str):
        spec = self._actions.get(method)
        if spec is None:
            raise AttributeError(f"no action {self._name}.{method}")
        action = spec["name"]

        def call(params: Optional[dict] = None, **kwargs: Any) -> Any:
            merged = dict(params or {})
            merged.update(kwargs)
            return self._app.dispatch(action, merged or None)

        call.__name__ = method
        call.__doc__ = f"{spec.get('description', '')}\n\nAction `{action}`, tier `{spec.get('tier')}`."
        return call

    def __dir__(self) -> List[str]:
        return sorted(self._actions)

    def __repr__(self) -> str:
        return f"<freekerf namespace {self._name}: {', '.join(sorted(self._actions))}>"


class App:
    """A connection to a running FreeKerf. Use :func:`connect`."""

    def __init__(self, endpoint: Endpoint, name: str):
        self._endpoint = endpoint
        self._name = name
        self._lock = threading.Lock()
        self._next = 1
        self._events: List[dict] = []
        self._transport = self._open()
        self._namespaces: Dict[str, _Namespace] = {}
        grouped: Dict[str, Dict[str, dict]] = {}
        for spec in self._call("specs"):
            ns, _, action = spec["name"].partition(".")
            grouped.setdefault(ns, {})[_method_name(action)] = spec
        for ns, actions in grouped.items():
            self._namespaces[ns] = _Namespace(self, ns, actions)

    def _open(self) -> Transport:
        t = Transport(self._endpoint.address)
        t.send({"hello": {
            "token": self._endpoint.read_token(),
            "client": "python_client",
            "name": self._name,
            "version": PROTOCOL_VERSION,
        }})
        answer = t.recv()
        if answer is None:
            raise FreeKerfError("closed", "the connection to FreeKerf closed")
        if "error" in answer:
            t.close()
            raise _error(answer["error"])
        self.welcome: Dict[str, Any] = answer["welcome"]
        return t

    def _call(self, method: str, params: Any = None) -> Any:
        with self._lock:
            rid = self._next
            self._next += 1
            self._transport.send({"id": rid, "method": method, "params": params})
            while True:
                msg = self._transport.recv()
                if msg is None:
                    raise FreeKerfError("closed", "the connection to FreeKerf closed")
                if msg.get("id") == rid:
                    if "error" in msg:
                        raise _error(msg["error"])
                    return msg.get("result")
                if "event" in msg:
                    self._events.append(msg["event"])

    # ---- the API ---------------------------------------------------------------

    def __getattr__(self, name: str) -> _Namespace:
        ns = self.__dict__.get("_namespaces", {}).get(name)
        if ns is None:
            raise AttributeError(f"no namespace {name!r}")
        return ns

    def __dir__(self) -> List[str]:
        return sorted(set(super().__dir__()) | set(self._namespaces))

    @property
    def tier(self) -> str:
        """The highest tier this client may use without asking."""
        return str(self.welcome.get("tier"))

    @property
    def headless(self) -> bool:
        """Whether the app has no person in front of it (laser actions refused)."""
        return bool(self.welcome.get("headless"))

    def dispatch(self, action: str, params: Optional[dict] = None) -> Any:
        """Runs one action by name (``"document.select_all"``)."""
        return self._call("dispatch", {"action": action, "params": params})

    def state(self, path: Optional[str] = None) -> Any:
        """The app state (session, machine, console), or the part at ``path``
        (``"session.view.mode"``)."""
        return self._call("get_state", {"path": path} if path else None)

    def run_script(self, code: str, dry_run: bool = False, timeout_s: Optional[int] = None) -> Dict[str, Any]:
        """Runs JavaScript in the app (one undo step, rolled back on error) and
        returns its report: ``value``, ``log``, ``actions``, ``error``..."""
        params: Dict[str, Any] = {"code": code, "dry_run": dry_run, "name": self._name}
        if timeout_s is not None:
            params["timeout_s"] = timeout_s
        return self._call("run_script", params)

    def search_api(self, query: str, limit: Optional[int] = None) -> List[Dict[str, Any]]:
        """Actions matching every word of ``query``."""
        return self._call("search_api", {"query": query, "limit": limit})

    def api_dts(self) -> str:
        """The TypeScript declarations of the script API."""
        return self._call("api_dts")

    def stop(self) -> None:
        """STOP: cancels running scripts and soft-resets the machine. Uses its own
        connection, so it works while another call of this client is running."""
        t = Transport(self._endpoint.address)
        try:
            t.send({"hello": {"token": self._endpoint.read_token(), "client": "python_client",
                              "name": self._name, "version": PROTOCOL_VERSION}})
            answer = t.recv()
            if answer is None or "error" in answer:
                raise _error((answer or {}).get("error", {"kind": "closed", "message": "closed"}))
            t.send({"id": 1, "method": "stop", "params": None})
            while True:
                msg = t.recv()
                if msg is None or msg.get("id") == 1:
                    break
        finally:
            t.close()

    def subscribe(self) -> None:
        """Starts receiving events (read them with :meth:`events`)."""
        self._call("subscribe")

    def events(self, timeout: float = 0.0) -> Iterator[dict]:
        """Events received so far; waits up to ``timeout`` seconds for at least one."""
        deadline = time.monotonic() + timeout
        while not self._events and time.monotonic() < deadline:
            # a ping lets the reader pick up events sent since the last call
            self._call("ping")
            if not self._events:
                time.sleep(0.05)
        while self._events:
            yield self._events.pop(0)

    def close(self) -> None:
        self._transport.close()

    def __enter__(self) -> "App":
        return self

    def __exit__(self, *exc: Any) -> None:
        self.close()

    def __repr__(self) -> str:
        return f"<freekerf.App {self._name!r} tier={self.tier} headless={self.headless}>"


def connect(
    name: str = "python",
    home: Optional[Union[str, "os.PathLike[str]"]] = None,
    timeout: float = 0.0,
) -> App:
    """Connects to the running FreeKerf. ``home`` is the instance's ``--home``
    directory (default: ``$FREEKERF_HOME`` or the per-user one); ``timeout`` waits
    for an instance that is starting."""
    endpoint = Endpoint(runtime_dir(home))
    deadline = time.monotonic() + timeout
    while True:
        try:
            return App(endpoint, name)
        except (OSError, FreeKerfError) as e:
            if isinstance(e, FreeKerfError) and e.kind != "closed":
                raise
            if time.monotonic() >= deadline:
                raise FreeKerfError("not_running", f"no running FreeKerf at {endpoint.address}: {e}") from e
            time.sleep(0.05)
