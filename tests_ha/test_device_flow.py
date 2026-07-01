from homeassistant.config_entries import SOURCE_INTEGRATION_DISCOVERY

from custom_components.byonk.const import CONF_DEVICE_KEY, DOMAIN
from tests_ha.conftest import make_hub_entry

SCREENS_NO_PARAMS = {
    "screens": [{"name": "transit", "params": [], "schema_error": None}],
    "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson"],
}
SCREENS_PARAMS = {
    "screens": [{"name": "transit",
                 "params": [{"name": "limit", "type": "int", "default": 8}],
                 "schema_error": None}],
    "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson"],
}


async def _setup_hub(hass):
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_discovery_creates_device_entry_and_posts(hass, byonk):
    byonk.screens = SCREENS_NO_PARAMS
    await _setup_hub(hass)

    result = await hass.config_entries.flow.async_init(
        DOMAIN,
        context={"source": SOURCE_INTEGRATION_DISCOVERY},
        data={"key": "CC:DD", "code": "ABCD-1234", "model": "og"},
    )
    assert result["type"] == "form"
    assert result["step_id"] == "configure"

    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {"screen": "transit"}
    )
    await hass.async_block_till_done()

    assert result["type"] == "create_entry"
    assert byonk.add_device.await_args.args[0]["key"] == "CC:DD"
    assert byonk.add_device.await_args.args[0]["screen"] == "transit"
    entries = [e for e in hass.config_entries.async_entries(DOMAIN)
               if e.data.get(CONF_DEVICE_KEY) == "CC:DD"]
    assert len(entries) == 1


async def test_discovery_with_params_shows_second_form(hass, byonk):
    byonk.screens = SCREENS_PARAMS
    await _setup_hub(hass)
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": SOURCE_INTEGRATION_DISCOVERY},
        data={"key": "CC:DD", "code": "ABCD-1234", "model": "og"},
    )
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {"screen": "transit"}
    )
    assert result["type"] == "form"
    assert result["step_id"] == "dev_params"
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {"limit": 5}
    )
    await hass.async_block_till_done()
    assert result["type"] == "create_entry"
    assert byonk.add_device.await_args.args[0]["params"] == {"limit": 5}


async def test_discovery_aborts_if_already_configured(hass, byonk):
    byonk.screens = SCREENS_NO_PARAMS
    hub = await _setup_hub(hass)
    from tests_ha.conftest import make_device_entry
    make_device_entry(hass, hub, "CC:DD")
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": SOURCE_INTEGRATION_DISCOVERY},
        data={"key": "CC:DD", "code": "ABCD-1234", "model": "og"},
    )
    assert result["type"] == "abort"
    assert result["reason"] == "already_configured"


async def test_hub_single_instance(hass, byonk):
    await _setup_hub(hass)
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": "user"}
    )
    assert result["type"] == "abort"
    assert result["reason"] == "single_instance_allowed"


