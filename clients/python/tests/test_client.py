"""The Python client against a headless FreeKerf (``freekerf serve --headless``)."""

from __future__ import annotations

import os
import subprocess
import threading
import time
from pathlib import Path

import pytest

import freekerf

ROOT = Path(__file__).resolve().parents[3]
SVG = ('<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm">'
       '<rect x="1" y="1" width="5" height="5" stroke="red" fill="none"/></svg>')


def binary() -> str:
    path = os.environ.get("FREEKERF_BIN") or str(ROOT / "target" / "debug" / "freekerf")
    if not Path(path).exists():
        pytest.skip(f"no freekerf binary at {path} (cargo build -p freekerf-cli, or set FREEKERF_BIN)")
    return path


def configure(home: Path, *pairs: str) -> None:
    for key, value in zip(pairs[::2], pairs[1::2]):
        subprocess.run([binary(), "--home", str(home), "config", "set", key, value], check=True)


@pytest.fixture
def home(tmp_path: Path):
    proc = subprocess.Popen([binary(), "--home", str(tmp_path), "serve", "--headless"])
    yield tmp_path
    proc.terminate()
    proc.wait(timeout=10)


def test_the_api_comes_from_the_registry(home: Path):
    with freekerf.connect(name="pytest", home=home, timeout=10) as app:
        assert app.tier == "edit"
        assert app.headless
        assert "document" in dir(app)
        assert "select_all" in dir(app.document)
        assert "import_file" in dir(app.document)  # document.import
        assert "Selects every object" in app.document.select_all.__doc__
        with pytest.raises(AttributeError):
            app.nothing
        with pytest.raises(AttributeError):
            app.document.nothing


def test_document_actions_state_and_scripts(home: Path):
    with freekerf.connect(name="pytest", home=home, timeout=10) as app:
        ids = app.document.import_data(format="svg", text=SVG)["objects"]
        assert len(ids) == 1
        app.document.select_all()
        assert app.document.translate({"dx": 5}, dy=0) == {"changed": True}
        assert app.state("session.selection") == ids
        assert app.state()["headless"] is True
        report = app.run_script("app.view.setMode({mode: 'produce'}); app.session.snapshot().document.objects.length")
        assert report["value"] == 1 and report["error"] is None
        assert app.state("session.view.mode") == "produce"
        dry = app.run_script("app.machine.home(); 2", dry_run=True)
        assert dry["error"] is not None  # machine is above the Python client's default tier
        assert app.search_api("select all")[0]["action"] == "document.select_all"
        assert "interface AppDocument" in app.api_dts()
        app.subscribe()
        app.view.set_mode(mode="design")
        events = list(app.events(timeout=5))
        assert any(e.get("change") == "view" for e in events)


def test_tiers_and_laser(home: Path):
    with freekerf.connect(name="pytest", home=home, timeout=10) as app:
        with pytest.raises(freekerf.PermissionDenied) as e:
            app.machine.jog(direction="N")
        assert "needs tier `machine`" in str(e.value)
        with pytest.raises(freekerf.PermissionDenied):
            app.job.run()
        with pytest.raises(freekerf.FreeKerfError) as e:
            app.dispatch("nope.nope")
        assert e.value.kind == "unknown"


def test_machine_tier_from_the_settings(tmp_path: Path):
    configure(tmp_path, "automation.python.allow_machine", "true")
    proc = subprocess.Popen([binary(), "--home", str(tmp_path), "serve", "--headless"])
    try:
        with freekerf.connect(home=tmp_path, timeout=10) as app:
            assert app.tier == "machine"
            # allowed now; nothing is connected, so the action itself fails
            with pytest.raises(freekerf.FreeKerfError) as e:
                app.machine.home()
            assert e.value.kind != "permission_denied"
            # laser stays above the tier, and no one is present to confirm it
            with pytest.raises(freekerf.PermissionDenied):
                app.machine.send(line="M3 S1000")
    finally:
        proc.terminate()
        proc.wait(timeout=10)


def test_stop_while_a_script_runs(home: Path):
    with freekerf.connect(name="pytest", home=home, timeout=10) as app:
        result = {}

        def run():
            result["report"] = app.run_script("while (true) {}", timeout_s=20)

        t = threading.Thread(target=run)
        t.start()
        time.sleep(0.5)
        app.stop()
        t.join(timeout=10)
        assert result["report"]["error"] == "script stopped: cancelled"


def test_not_running(tmp_path: Path):
    with pytest.raises(freekerf.FreeKerfError) as e:
        freekerf.connect(home=tmp_path)
    assert e.value.kind == "not_running"


def test_endpoint_paths(tmp_path: Path, monkeypatch):
    assert freekerf.runtime_dir(tmp_path) == tmp_path
    monkeypatch.setenv("FREEKERF_HOME", str(tmp_path))
    assert freekerf.runtime_dir() == tmp_path
