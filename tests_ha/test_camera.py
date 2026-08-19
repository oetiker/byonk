"""The screen preview camera on a device page."""
import pytest
from homeassistant.components.camera import MIN_STREAM_INTERVAL, async_get_image
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers.entity_component import DATA_INSTANCES

from custom_components.byonk.api import ByonkConnectionError
from custom_components.byonk.camera import FRAME_INTERVAL
from tests_ha.conftest import PNG_1PX, make_device_entry, make_hub_entry

TRANSIT_REF = "byonk-builtin/useful/swiss-departure-board"
DEV = {"key": "AA:BB", "registered": True, "screen": TRANSIT_REF}


async def setup_device(hass, byonk, key="AA:BB"):
    byonk.devices = [{**DEV, "key": key}]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    entry = make_device_entry(hass, hub, key)
    await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    return entry


def preview_entity(hass):
    cameras = hass.states.async_all("camera")
    assert cameras, "no camera entity was created"
    return cameras[0].entity_id


async def test_device_gets_a_screen_preview_camera(hass, byonk):
    await setup_device(hass, byonk)
    assert preview_entity(hass)


async def test_hub_has_no_camera(hass, byonk):
    """The hub is a server, not a screen — it has nothing to preview."""
    byonk.devices = []
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    assert hass.states.async_all("camera") == []


async def test_the_default_device_gets_a_camera(hass, byonk):
    """DEFAULT shows a real screen (every unassigned device displays it), so
    it needs a preview as much as a physical device does."""
    await setup_device(hass, byonk, key="DEFAULT")
    assert preview_entity(hass)


async def test_image_comes_from_byonk(hass, byonk):
    await setup_device(hass, byonk)
    image = await async_get_image(hass, preview_entity(hass))
    assert image.content == PNG_1PX
    assert byonk.get_device_preview.await_args.args == ("AA:BB",)


async def test_frame_interval_is_not_the_video_default(hass, byonk):
    """Home Assistant's default is `MIN_STREAM_INTERVAL` (0.5 s), meant for
    video. Left alone it would ask byonk for a frame twice a second for as
    long as the device page is open.

    `frame_interval` is not a state attribute — `handle_async_still_stream`
    reads it off the entity — so this asserts against the live entity."""
    await setup_device(hass, byonk)
    entity = hass.data[DATA_INSTANCES]["camera"].get_entity(preview_entity(hass))
    assert entity.frame_interval == FRAME_INTERVAL
    assert FRAME_INTERVAL > MIN_STREAM_INTERVAL * 10


async def test_a_failed_fetch_leaves_the_camera_alive(hass, byonk):
    """A byonk that is down must cost you the picture, not the device page.

    An exception escaping `async_camera_image` marks the entity unavailable
    and hides the screen/dither/panel controls alongside it, so the fetch
    swallows API errors and returns no frame instead."""
    byonk.get_device_preview.side_effect = ByonkConnectionError("boom")
    await setup_device(hass, byonk)
    entity_id = preview_entity(hass)

    # "No frame" reaches this caller as an error — that part is HA's doing.
    with pytest.raises(HomeAssistantError):
        await async_get_image(hass, entity_id)

    # ...but the entity itself is still there and still usable.
    state = hass.states.get(entity_id)
    assert state is not None
    assert state.state != "unavailable"


async def test_refresh_preview_button_forces_a_render(hass, byonk):
    await setup_device(hass, byonk)
    button = next(
        s.entity_id for s in hass.states.async_all("button") if "refresh_preview" in s.entity_id
    )
    await hass.services.async_call("button", "press", {"entity_id": button}, blocking=True)
    byonk.get_device_preview.assert_awaited_with(
        "AA:BB", force=True, dither=True, measured=True
    )


async def test_hub_keeps_its_own_button(hass, byonk):
    """Adding a device button must not displace the hub's."""
    byonk.screen_repos = [{"handle": "weather", "builtin": False, "status": "ready"}]
    byonk.devices = []
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    assert hass.states.get("button.byonk_update_screen_repos") is not None


async def test_the_camera_reports_png(hass, byonk):
    """`Camera.__init__` defaults `content_type` to `image/jpeg`. Left at that,
    the preview is served mislabelled and Home Assistant routes it through
    `scale_jpeg_camera_image`, which cannot read a PNG."""
    await setup_device(hass, byonk)
    entity = hass.data[DATA_INSTANCES]["camera"].get_entity(preview_entity(hass))
    assert entity.content_type == "image/png"

    # ...and the scaling path stays out of the way even when a caller asks for
    # a specific size.
    image = await async_get_image(hass, preview_entity(hass), width=100, height=100)
    assert image.content == PNG_1PX
    assert image.content_type == "image/png"


async def test_preview_options_default_to_byonks_normal_render(hass, byonk):
    await setup_device(hass, byonk)
    await async_get_image(hass, preview_entity(hass))
    byonk.get_device_preview.assert_awaited_with("AA:BB", dither=True, measured=True)


async def test_turning_off_dithering_asks_for_the_undithered_render(hass, byonk):
    entry = await setup_device(hass, byonk)
    await hass.services.async_call(
        "switch",
        "turn_off",
        {"entity_id": "switch.trmnl_aa_bb_preview_dithering"},
        blocking=True,
    )
    await async_get_image(hass, preview_entity(hass))
    byonk.get_device_preview.assert_awaited_with("AA:BB", dither=False, measured=True)
    # It is a Home Assistant view preference, so it persists on the entry...
    assert entry.options["preview_dither"] is False
    # ...and byonk's device configuration is untouched: changing how the
    # preview is drawn must never change what the panel shows.
    assert byonk.update_device.await_count == 0


async def test_turning_off_measured_colors_asks_for_spec_colors(hass, byonk):
    await setup_device(hass, byonk)
    await hass.services.async_call(
        "switch",
        "turn_off",
        {"entity_id": "switch.trmnl_aa_bb_preview_measured_colors"},
        blocking=True,
    )
    await async_get_image(hass, preview_entity(hass))
    byonk.get_device_preview.assert_awaited_with("AA:BB", dither=True, measured=False)


async def test_refresh_button_forces_the_variant_on_display(hass, byonk):
    """Refreshing byonk's default render while looking at the undithered one
    would refresh an image nobody can see."""
    await setup_device(hass, byonk)
    await hass.services.async_call(
        "switch",
        "turn_off",
        {"entity_id": "switch.trmnl_aa_bb_preview_dithering"},
        blocking=True,
    )
    await hass.services.async_call(
        "button",
        "press",
        {"entity_id": "button.trmnl_aa_bb_refresh_preview"},
        blocking=True,
    )
    byonk.get_device_preview.assert_awaited_with(
        "AA:BB", force=True, dither=False, measured=True
    )
