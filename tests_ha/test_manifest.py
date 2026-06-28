import json
from pathlib import Path

MANIFEST = Path("custom_components/byonk/manifest.json")


def test_manifest_has_required_keys():
    data = json.loads(MANIFEST.read_text())
    assert data["domain"] == "byonk"
    assert data["integration_type"] == "hub"
    assert data["iot_class"] == "local_polling"
    assert data["config_flow"] is True
    assert data["after_dependencies"] == ["hassio"]
    assert "hassio" not in data.get("dependencies", [])
    assert data["codeowners"] == ["@oetiker"]
    assert "version" in data and data["version"]
    for key in ("documentation", "issue_tracker"):
        assert data[key].startswith("https://github.com/oetiker/byonk")


def test_hacs_json_parses():
    data = json.loads(Path("hacs.json").read_text())
    assert data["name"]
