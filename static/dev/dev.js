// Byonk Dev Mode JavaScript

// Per-algorithm default values for noise_scale and error_clamp.
// When the user switches dither algorithm, these defaults are applied unless
// the user has saved per-algorithm overrides in localStorage.
const DITHER_DEFAULTS = {
    'atkinson':            { noiseScale: '0',   errorClamp: '0.08' },
    'floyd-steinberg':     { noiseScale: '4.0', errorClamp: '0.12' },
    'jarvis-judice-ninke': { noiseScale: '6.0', errorClamp: '0.03' },
    'sierra':              { noiseScale: '5.5', errorClamp: '0.10' },
    'sierra-two-row':      { noiseScale: '7.0', errorClamp: '0.10' },
    'sierra-lite':         { noiseScale: '2.5', errorClamp: '0.11' },
    'stucki':              { noiseScale: '6.0', errorClamp: '0.03' },
    'burkes':              { noiseScale: '7.0', errorClamp: '0.10' },
};

const state = {
    screens: [],
    devices: [],
    panels: [],
    defaultScreen: null,
    eventSource: null,
    isRendering: false,
    timeLocked: false,
    colorPopup: {
        visible: false,
        colorIndex: -1,
        originalHex: '',
    },
    // Per-device color overrides from color tuning popup.
    // Map of deviceKey → colors_actual string.  Persisted in localStorage.
    colorOverrides: {},
    // Per-algorithm dither tuning overrides.
    // Map of algorithm name → { noiseScale, errorClamp }.
    ditherTuningOverrides: {},
};

// DOM elements
const elements = {
    screenSelect: document.getElementById('screen-select'),
    panelSelect: document.getElementById('panel-select'),
    batteryInput: document.getElementById('battery-input'),
    rssiInput: document.getElementById('rssi-input'),
    timeInput: document.getElementById('time-input'),
    timeMode: document.getElementById('time-mode'),
    timeReset: document.getElementById('time-reset'),
    paramsInput: document.getElementById('params-input'),
    ditherSelect: document.getElementById('dither-select'),
    displayImage: document.getElementById('display-image'),
    loadingOverlay: document.getElementById('loading-overlay'),
    deviceFrame: document.getElementById('device-frame'),
    deviceBezel: document.querySelector('.device-bezel'),
    displaySize: document.getElementById('display-size'),
    renderTime: document.getElementById('render-time'),
    watchStatus: document.getElementById('watch-status'),
    deviceInfo: document.getElementById('device-info'),
    useActual: document.getElementById('use-actual'),
    useActualLabel: document.getElementById('use-actual-label'),
    preserveExact: document.getElementById('preserve-exact'),
    errorClamp: document.getElementById('error-clamp'),
    chromaClamp: document.getElementById('chroma-clamp'),
    noiseScale: document.getElementById('noise-scale'),
    consoleOutput: document.getElementById('console-output'),
    colorPopup: document.getElementById('color-popup'),
    popupClose: document.getElementById('color-popup-close'),
    popupOriginalColor: document.getElementById('popup-original-color'),
    popupLiveColor: document.getElementById('popup-live-color'),
    popupHue: document.getElementById('popup-hue'),
    popupSat: document.getElementById('popup-sat'),
    popupLit: document.getElementById('popup-lit'),
    popupHueVal: document.getElementById('popup-hue-val'),
    popupSatVal: document.getElementById('popup-sat-val'),
    popupLitVal: document.getElementById('popup-lit-val'),
    popupHexInput: document.getElementById('popup-hex-input'),
    popupResetBtn: document.getElementById('popup-reset-btn'),
};

// Format a Date as YYYY-MM-DDTHH:MM for datetime-local input
function formatDatetimeLocal(d) {
    const pad = (n) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

// Update the time input with current time (when not locked)
function tickTime() {
    if (!state.timeLocked) {
        elements.timeInput.value = formatDatetimeLocal(new Date());
    }
}

// Set time mode (live vs frozen)
function setTimeLocked(locked) {
    state.timeLocked = locked;
    elements.timeMode.textContent = locked ? 'frozen' : 'live';
    elements.timeMode.className = 'time-mode ' + (locked ? 'frozen' : 'live');
    elements.timeReset.classList.toggle('hidden', !locked);
    if (!locked) {
        tickTime();
    }
}

// Initialize
async function init() {
    await loadScreens();
    setupEventListeners();
    setupColorPopup();
    connectSSE();
    loadSavedState();
    updateUseActualVisibility();
    updateColorSwatches();
    setupLens();
    tickTime();
    setInterval(tickTime, 1000);
    render();
}

// Default dimensions and colors when no panel profile is selected
const DEFAULTS = {
    width: '800',
    height: '480',
    colors: '#000000,#555555,#AAAAAA,#FFFFFF',
};

// Get effective dimensions from panel profile or defaults
function getEffectiveDimensions() {
    const panelOpt = elements.panelSelect.selectedOptions[0];
    if (panelOpt?.value && panelOpt.dataset.width) {
        return { width: panelOpt.dataset.width, height: panelOpt.dataset.height };
    }
    return { width: DEFAULTS.width, height: DEFAULTS.height };
}

// Get effective colors from panel profile or defaults
function getEffectiveColors() {
    const panelOpt = elements.panelSelect.selectedOptions[0];
    if (panelOpt?.value && panelOpt.dataset.colors) return panelOpt.dataset.colors;
    return DEFAULTS.colors;
}

// Load available screens from server
async function loadScreens() {
    try {
        const response = await fetch('/dev/screens');
        const data = await response.json();
        state.screens = data.screens;
        state.devices = data.devices || [];
        state.panels = data.panels || [];
        state.defaultScreen = data.default_screen;

        // Populate panel select
        elements.panelSelect.innerHTML = '<option value="">None (use default colors)</option>';
        state.panels.forEach(panel => {
            const option = document.createElement('option');
            option.value = panel.id;
            option.textContent = panel.name;
            option.dataset.colors = panel.colors;
            if (panel.colors_actual) option.dataset.colorsActual = panel.colors_actual;
            if (panel.width) option.dataset.width = panel.width;
            if (panel.height) option.dataset.height = panel.height;
            elements.panelSelect.appendChild(option);
        });

        // Group devices by screen name
        const devicesByScreen = {};
        state.devices.forEach(dev => {
            if (!devicesByScreen[dev.screen]) {
                devicesByScreen[dev.screen] = [];
            }
            devicesByScreen[dev.screen].push(dev);
        });

        // Populate screen select with screens and their devices
        elements.screenSelect.innerHTML = '';
        state.screens.forEach(screen => {
            const option = document.createElement('option');
            option.value = `screen:${screen.name}`;
            option.textContent = screen.name;
            if (screen.name === state.defaultScreen) {
                option.textContent += ' (default)';
            }
            elements.screenSelect.appendChild(option);

            // Add device entries under this screen
            const devices = devicesByScreen[screen.name] || [];
            devices.forEach(dev => {
                const devOption = document.createElement('option');
                devOption.value = `device:${dev.id}`;
                devOption.textContent = `  \u21b3 ${dev.id}`;
                if (dev.panel) devOption.dataset.panel = dev.panel;
                if (dev.dither) devOption.dataset.dither = dev.dither;
                elements.screenSelect.appendChild(devOption);
            });
        });

        // Select default screen
        if (state.defaultScreen) {
            elements.screenSelect.value = `screen:${state.defaultScreen}`;
        }
    } catch (error) {
        console.error('Failed to load screens:', error);
        consoleError('Failed to load screens: ' + error.message);
    }
}

// Setup event listeners
function setupEventListeners() {
    // Panel select updates swatches and dimensions
    elements.panelSelect.addEventListener('change', () => {
        updateDeviceFrame();
        updateColorSwatches();
        updateUseActualVisibility();
        saveState();
        render();
    });

    // "Show actual panel colors" toggle
    elements.useActual.addEventListener('change', () => {
        saveState();
        render();
    });

    // Screen select — device-aware panel selection
    elements.screenSelect.addEventListener('change', () => {
        const selected = elements.screenSelect.value;
        const isDevice = selected.startsWith('device:');

        // Toggle device info banner
        elements.deviceInfo.classList.toggle('hidden', !isDevice);

        // Auto-select panel and dither from device config
        if (isDevice) {
            const option = elements.screenSelect.selectedOptions[0];
            if (option.dataset.panel) {
                elements.panelSelect.value = option.dataset.panel;
            }
            if (option.dataset.dither) {
                elements.ditherSelect.value = option.dataset.dither;
            }
            // Apply saved color override for this device (if any)
            const deviceKey = selected.slice('device:'.length);
            const override = state.colorOverrides[deviceKey];
            if (override) {
                const panelOpt = elements.panelSelect.selectedOptions[0];
                if (panelOpt?.value) {
                    panelOpt.dataset.colorsActual = override;
                }
            }
            updateColorSwatches();
            updateUseActualVisibility();
        }

        updateDeviceFrame();
        saveState();
        render();
    });

    // Battery, RSSI, Time changes
    elements.batteryInput.addEventListener('change', () => {
        saveState();
        render();
    });
    elements.rssiInput.addEventListener('change', () => {
        saveState();
        render();
    });
    elements.timeInput.addEventListener('change', () => {
        setTimeLocked(true);
        saveState();
        render();
    });

    // Reset time to live
    elements.timeReset.addEventListener('click', () => {
        setTimeLocked(false);
        saveState();
        render();
    });

    // Preserve exact matches toggle
    elements.preserveExact.addEventListener('change', () => {
        saveState();
        render();
    });

    // Dither dropdown — apply per-algorithm defaults or saved overrides
    elements.ditherSelect.addEventListener('change', () => {
        applyDitherDefaults();
        saveState();
        render();
    });

    // Dither tunables
    elements.errorClamp.addEventListener('change', () => {
        saveDitherTuningOverride();
        saveState();
        render();
    });
    elements.chromaClamp.addEventListener('change', () => {
        saveState();
        render();
    });
    elements.noiseScale.addEventListener('change', () => {
        saveDitherTuningOverride();
        saveState();
        render();
    });

    // Params input (debounced)
    let paramsTimeout;
    elements.paramsInput.addEventListener('input', () => {
        clearTimeout(paramsTimeout);
        paramsTimeout = setTimeout(() => {
            saveState();
            render();
        }, 500);
    });
}

// Apply per-algorithm defaults (or saved overrides) when dither algorithm changes
function applyDitherDefaults() {
    const algo = elements.ditherSelect.value;
    const override = state.ditherTuningOverrides[algo];
    const defaults = DITHER_DEFAULTS[algo] || DITHER_DEFAULTS['atkinson'];
    elements.noiseScale.value = override?.noiseScale ?? defaults.noiseScale;
    elements.errorClamp.value = override?.errorClamp ?? defaults.errorClamp;
}

// Save current noise_scale and error_clamp as per-algorithm override
function saveDitherTuningOverride() {
    const algo = elements.ditherSelect.value;
    state.ditherTuningOverrides[algo] = {
        noiseScale: elements.noiseScale.value,
        errorClamp: elements.errorClamp.value,
    };
}

// Connect to Server-Sent Events for file changes
function connectSSE() {
    if (state.eventSource) {
        state.eventSource.close();
    }

    state.eventSource = new EventSource('/dev/events');

    state.eventSource.onopen = () => {
        elements.watchStatus.textContent = 'File Watch: Connected';
        elements.watchStatus.classList.remove('disconnected');
        elements.watchStatus.classList.add('connected');
    };

    state.eventSource.onerror = () => {
        elements.watchStatus.textContent = 'File Watch: Disconnected';
        elements.watchStatus.classList.remove('connected');
        elements.watchStatus.classList.add('disconnected');

        // Reconnect after a delay
        setTimeout(connectSSE, 3000);
    };

    state.eventSource.addEventListener('file-change', (event) => {
        const files = JSON.parse(event.data);
        consoleLog('Files changed: ' + files.join(', '));
        render();
    });

    state.eventSource.addEventListener('refresh', () => {
        render();
    });
}

// Update device frame styling based on effective dimensions
function updateDeviceFrame() {
    const dims = getEffectiveDimensions();
    // High-res displays (either dimension > 1000) get smooth scaling; others get pixelated
    const frameClass = (parseInt(dims.width) > 1000 || parseInt(dims.height) > 1000) ? 'x' : 'og';
    elements.deviceFrame.classList.remove('og', 'x');
    elements.deviceFrame.classList.add(frameClass);

    elements.displaySize.textContent = `${dims.width} x ${dims.height}`;
}

// Update color swatches display
function updateColorSwatches() {
    const container = document.getElementById('color-swatches');
    if (!container) return;

    const colorsStr = getEffectiveColors();
    const colors = colorsStr ? colorsStr.split(',').map(c => c.trim()).filter(c => c) : [];

    // Get measured colors from selected panel
    const panelOption = elements.panelSelect.selectedOptions[0];
    const actualStr = panelOption && panelOption.dataset.colorsActual;
    const actualColors = actualStr ? actualStr.split(',').map(c => c.trim()).filter(c => c) : [];

    if (colors.length === 0) {
        container.innerHTML = '';
        return;
    }

    let html = '<div class="swatch-label-row"><span class="swatch-label">Official</span></div>';
    html += '<div class="swatch-row">';
    colors.forEach((color, i) => {
        const actual = actualColors[i] || '';
        html += `<span class="swatch" style="background:${color}" title="Official: ${color}${actual ? '\nActual: ' + actual : ''}"></span>`;
    });
    html += '</div>';

    if (actualColors.length > 0) {
        html += '<div class="swatch-label-row"><span class="swatch-label">Actual</span>';
        html += '<button class="copy-colors-btn" onclick="copyActualColors()">Copy color config</button>';
        html += '</div>';
        html += '<div class="swatch-row actual">';
        actualColors.forEach((color, i) => {
            html += `<span class="swatch" style="background:${color}" data-index="${i}" title="Actual: ${color} (click to adjust)" onclick="openColorPopup(this, ${i})"></span>`;
        });
        html += '</div>';
    }

    container.innerHTML = html;
}

// Show/hide the "Show actual panel colors" checkbox based on panel selection
function updateUseActualVisibility() {
    const option = elements.panelSelect.selectedOptions[0];
    const hasActual = option && option.dataset.colorsActual;
    elements.useActualLabel.style.display = hasActual ? '' : 'none';
}

// Console functions
function consoleClear() {
    elements.consoleOutput.textContent = '';
}

function consoleLog(msg) {
    const time = new Date().toLocaleTimeString();
    const line = document.createElement('span');
    line.className = 'console-info';
    line.textContent = `[${time}] ${msg}\n`;
    elements.consoleOutput.appendChild(line);
    elements.consoleOutput.parentElement.scrollTop = elements.consoleOutput.parentElement.scrollHeight;
}

function consoleError(msg) {
    const time = new Date().toLocaleTimeString();
    const line = document.createElement('span');
    line.className = 'console-error';
    line.textContent = `[${time}] ERROR: ${msg}\n`;
    elements.consoleOutput.appendChild(line);
    elements.consoleOutput.parentElement.scrollTop = elements.consoleOutput.parentElement.scrollHeight;
}

// Render the current screen
async function render() {
    if (state.isRendering) return;

    const selected = elements.screenSelect.value;
    if (!selected) return;

    const isDevice = selected.startsWith('device:');
    const mac = isDevice ? selected.slice('device:'.length) : '';
    const screen = isDevice ? '' : selected.slice('screen:'.length);

    state.isRendering = true;
    elements.loadingOverlay.classList.remove('hidden');
    consoleClear();

    const startTime = performance.now();

    try {
        // Validate params JSON
        let params = '{}';
        try {
            const parsed = JSON.parse(elements.paramsInput.value || '{}');
            params = JSON.stringify(parsed);
        } catch (e) {
            consoleError('Invalid JSON in parameters: ' + e.message);
            return;
        }

        // Convert local datetime to UTC timestamp (only when time is frozen)
        let timestamp = '';
        if (state.timeLocked && elements.timeInput.value) {
            const localDate = new Date(elements.timeInput.value);
            timestamp = Math.floor(localDate.getTime() / 1000).toString();
        }

        const dims = getEffectiveDimensions();
        const colors = getEffectiveColors();

        const queryParams = new URLSearchParams({
            width: dims.width,
            height: dims.height,
            battery_voltage: elements.batteryInput.value,
            rssi: elements.rssiInput.value,
            colors: colors,
            params,
        });

        // Add preserve_exact param
        if (!elements.preserveExact.checked) {
            queryParams.set('preserve_exact', 'false');
        }

        // Add dither if not auto
        if (elements.ditherSelect.value) {
            queryParams.set('dither', elements.ditherSelect.value);
        }

        // Dither tunables
        const errorClamp = elements.errorClamp.value;
        if (errorClamp !== '' && errorClamp !== '0.08') {
            queryParams.set('error_clamp', errorClamp);
        }
        const chromaClamp = elements.chromaClamp.value;
        if (chromaClamp !== '') {
            queryParams.set('chroma_clamp', chromaClamp);
        }
        const noiseScale = elements.noiseScale.value;
        if (noiseScale !== '' && noiseScale !== '5') {
            queryParams.set('noise_scale', noiseScale);
        }

        // Add panel if selected (for measured color preview)
        if (elements.panelSelect.value) {
            queryParams.set('panel', elements.panelSelect.value);
            queryParams.set('use_actual', elements.useActual.checked ? 'true' : 'false');
            // Send tuned actual colors directly so the preview doesn't need the override map
            const panelOpt = elements.panelSelect.selectedOptions[0];
            if (panelOpt?.dataset.colorsActual) {
                queryParams.set('colors_actual', panelOpt.dataset.colorsActual);
            }
        }

        // Add screen or mac (mac takes precedence)
        if (mac) {
            queryParams.set('mac', mac);
        } else {
            queryParams.set('screen', screen);
        }

        // Add timestamp if set
        if (timestamp) {
            queryParams.set('timestamp', timestamp);
        }

        const response = await fetch(`/dev/render?${queryParams}`);

        if (!response.ok) {
            const error = await response.json();
            throw new Error(error.details || error.error);
        }

        const blob = await response.blob();
        const url = URL.createObjectURL(blob);

        // Revoke old URL to prevent memory leak
        if (elements.displayImage.src.startsWith('blob:')) {
            URL.revokeObjectURL(elements.displayImage.src);
        }

        elements.displayImage.src = url;

        // Update lens with new image
        updateLensImage(url);

        const elapsed = (performance.now() - startTime).toFixed(0);
        elements.renderTime.textContent = `Rendered in ${elapsed}ms`;
        consoleLog(`Rendered in ${elapsed}ms`);
    } catch (error) {
        console.error('Render error:', error);
        consoleError(error.message);
    } finally {
        state.isRendering = false;
        elements.loadingOverlay.classList.add('hidden');
    }
}

// Apply restored color overrides to panel option DOM and sync to server.
// Called after loadSavedState restores state.colorOverrides from localStorage.
function applyRestoredColorOverrides() {
    const selected = elements.screenSelect.value;
    if (!selected.startsWith('device:')) return;
    const deviceKey = selected.slice('device:'.length);
    const override = state.colorOverrides[deviceKey];
    if (!override) return;

    // Apply to the current panel option's dataset
    const panelOpt = elements.panelSelect.selectedOptions[0];
    if (panelOpt?.value) {
        panelOpt.dataset.colorsActual = override;
    }

    // POST to server so the production handler picks it up
    fetch('/dev/panel-colors', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ device: deviceKey, colors_actual: override }),
    }).catch(e => console.error('Failed to restore panel colors:', e));
}

// Save state to localStorage
function saveState() {
    const savedState = {
        screen: elements.screenSelect.value,
        panel: elements.panelSelect.value,
        battery: elements.batteryInput.value,
        rssi: elements.rssiInput.value,
        timeLocked: state.timeLocked,
        time: state.timeLocked ? elements.timeInput.value : '',
        params: elements.paramsInput.value,
        useActual: elements.useActual.checked,
        preserveExact: elements.preserveExact.checked,
        dither: elements.ditherSelect.value,
        errorClamp: elements.errorClamp.value,
        chromaClamp: elements.chromaClamp.value,
        noiseScale: elements.noiseScale.value,
        colorOverrides: state.colorOverrides,
        ditherTuningOverrides: state.ditherTuningOverrides,
    };
    localStorage.setItem('byonk-dev-state', JSON.stringify(savedState));
}

// Load state from localStorage
function loadSavedState() {
    try {
        const saved = localStorage.getItem('byonk-dev-state');
        if (saved) {
            const data = JSON.parse(saved);
            if (data.screen) {
                // Try exact match first (new format with prefix)
                const options = Array.from(elements.screenSelect.options);
                if (options.find(o => o.value === data.screen)) {
                    elements.screenSelect.value = data.screen;
                } else if (state.screens.find(s => s.name === data.screen)) {
                    // Migrate old format without prefix
                    elements.screenSelect.value = `screen:${data.screen}`;
                }
            }
            if (data.panel) {
                elements.panelSelect.value = data.panel;
            }
            if (data.battery) {
                elements.batteryInput.value = data.battery;
            }
            if (data.rssi) {
                elements.rssiInput.value = data.rssi;
            }
            if (data.timeLocked && data.time) {
                elements.timeInput.value = data.time;
                setTimeLocked(true);
            }
            if (data.params) {
                elements.paramsInput.value = data.params;
            }
            if (typeof data.useActual === 'boolean') {
                elements.useActual.checked = data.useActual;
            }
            if (typeof data.preserveExact === 'boolean') {
                elements.preserveExact.checked = data.preserveExact;
            }
            if (data.dither) {
                elements.ditherSelect.value = data.dither;
            }
            if (data.errorClamp) {
                elements.errorClamp.value = data.errorClamp;
            }
            if (typeof data.chromaClamp === 'string') {
                elements.chromaClamp.value = data.chromaClamp;
            }
            if (data.noiseScale) {
                elements.noiseScale.value = data.noiseScale;
            }
            if (data.colorOverrides && typeof data.colorOverrides === 'object') {
                state.colorOverrides = data.colorOverrides;
            }
            if (data.ditherTuningOverrides && typeof data.ditherTuningOverrides === 'object') {
                state.ditherTuningOverrides = data.ditherTuningOverrides;
            }

            // Show device info banner if a device is selected
            const isDevice = elements.screenSelect.value.startsWith('device:');
            elements.deviceInfo.classList.toggle('hidden', !isDevice);

            // Re-apply color overrides to DOM and sync to server
            applyRestoredColorOverrides();

            updateDeviceFrame();
            updateColorSwatches();
            updateUseActualVisibility();
        }
    } catch (e) {
        console.warn('Failed to load saved state:', e);
    }
}

// Lens functionality for 1:1 pixel viewing
let lensCanvas = null;
let lensCtx = null;
let fullResImage = null;
let lensElement = null;

function setupLens() {
    // Create lens element
    lensElement = document.createElement('div');
    lensElement.id = 'lens';
    lensElement.className = 'lens hidden';
    document.body.appendChild(lensElement);

    // Create canvas inside lens for drawing
    lensCanvas = document.createElement('canvas');
    lensCanvas.width = 200;
    lensCanvas.height = 200;
    lensElement.appendChild(lensCanvas);
    lensCtx = lensCanvas.getContext('2d');

    // Create hidden image for full resolution
    fullResImage = new Image();

    // Add mouse event listeners to display image
    elements.displayImage.addEventListener('mouseenter', showLens);
    elements.displayImage.addEventListener('mouseleave', hideLens);
    elements.displayImage.addEventListener('mousemove', moveLens);
}

function updateLensImage(url) {
    if (fullResImage) {
        fullResImage.src = url;
    }
}

function showLens(e) {
    if (!fullResImage.complete || !fullResImage.naturalWidth) return;

    lensElement.classList.remove('hidden');
    moveLens(e);
}

function hideLens() {
    lensElement.classList.add('hidden');
}

function moveLens(e) {
    if (!fullResImage.complete || !fullResImage.naturalWidth) return;

    const rect = elements.displayImage.getBoundingClientRect();
    const lensSize = 200;
    const lensRadius = lensSize / 2;
    const magnification = 2;
    const sourceSize = lensSize / magnification;  // 100px source shown at 200px

    // Position lens near cursor (offset to avoid covering what you're looking at)
    const lensX = e.clientX + 20;
    const lensY = e.clientY - lensSize - 10;

    lensElement.style.left = `${lensX}px`;
    lensElement.style.top = `${lensY}px`;

    // Calculate the position in the original image
    const imgX = e.clientX - rect.left;
    const imgY = e.clientY - rect.top;

    // Calculate scale between displayed size and actual image size
    const scaleX = fullResImage.naturalWidth / rect.width;
    const scaleY = fullResImage.naturalHeight / rect.height;

    // Source region size in full-res image pixels
    const srcW = sourceSize * scaleX;
    const srcH = sourceSize * scaleY;

    // Source coordinates in the full-res image (centered on cursor position)
    const srcX = (imgX * scaleX) - (srcW / 2);
    const srcY = (imgY * scaleY) - (srcH / 2);

    // Clear and draw the magnified region
    lensCtx.clearRect(0, 0, lensSize, lensSize);

    // Draw circular clip path
    lensCtx.save();
    lensCtx.beginPath();
    lensCtx.arc(lensRadius, lensRadius, lensRadius - 2, 0, Math.PI * 2);
    lensCtx.clip();

    // Fill with white background (for areas outside image)
    lensCtx.fillStyle = '#fff';
    lensCtx.fillRect(0, 0, lensSize, lensSize);

    // Disable image smoothing for crisp pixels
    lensCtx.imageSmoothingEnabled = false;

    // Draw the source region scaled up by magnification factor
    lensCtx.drawImage(
        fullResImage,
        srcX, srcY, srcW, srcH,              // Source rectangle in image pixels
        0, 0, lensSize, lensSize              // Destination rectangle (200x200)
    );

    lensCtx.restore();
}

// --- Color conversion utilities ---

function hexToRgb(hex) {
    hex = hex.replace('#', '');
    return {
        r: parseInt(hex.substring(0, 2), 16),
        g: parseInt(hex.substring(2, 4), 16),
        b: parseInt(hex.substring(4, 6), 16),
    };
}

function rgbToHex(r, g, b) {
    return '#' + [r, g, b].map(v => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, '0')).join('').toUpperCase();
}

function rgbToHsl(r, g, b) {
    r /= 255; g /= 255; b /= 255;
    const max = Math.max(r, g, b), min = Math.min(r, g, b);
    let h, s, l = (max + min) / 2;
    if (max === min) {
        h = s = 0;
    } else {
        const d = max - min;
        s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
        switch (max) {
            case r: h = ((g - b) / d + (g < b ? 6 : 0)) / 6; break;
            case g: h = ((b - r) / d + 2) / 6; break;
            case b: h = ((r - g) / d + 4) / 6; break;
        }
    }
    return { h: Math.round(h * 360), s: Math.round(s * 100), l: Math.round(l * 100) };
}

function hslToRgb(h, s, l) {
    h /= 360; s /= 100; l /= 100;
    let r, g, b;
    if (s === 0) {
        r = g = b = l;
    } else {
        const hue2rgb = (p, q, t) => {
            if (t < 0) t += 1;
            if (t > 1) t -= 1;
            if (t < 1/6) return p + (q - p) * 6 * t;
            if (t < 1/2) return q;
            if (t < 2/3) return p + (q - p) * (2/3 - t) * 6;
            return p;
        };
        const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
        const p = 2 * l - q;
        r = hue2rgb(p, q, h + 1/3);
        g = hue2rgb(p, q, h);
        b = hue2rgb(p, q, h - 1/3);
    }
    return { r: Math.round(r * 255), g: Math.round(g * 255), b: Math.round(b * 255) };
}

function hexToHsl(hex) {
    const { r, g, b } = hexToRgb(hex);
    return rgbToHsl(r, g, b);
}

function hslToHex(h, s, l) {
    const { r, g, b } = hslToRgb(h, s, l);
    return rgbToHex(r, g, b);
}

// --- Color popup ---

function openColorPopup(swatchEl, index) {
    const rect = swatchEl.getBoundingClientRect();
    const popup = elements.colorPopup;

    // Get current actual color
    const panelOption = elements.panelSelect.selectedOptions[0];
    const actualStr = panelOption && panelOption.dataset.colorsActual;
    if (!actualStr) return;
    const actualColors = actualStr.split(',').map(c => c.trim()).filter(c => c);
    const hex = actualColors[index];
    if (!hex) return;

    state.colorPopup.visible = true;
    state.colorPopup.colorIndex = index;
    state.colorPopup.originalHex = hex;

    // Position popup near the swatch
    const popupWidth = 280;
    let left = rect.left;
    let top = rect.bottom + 8;
    // Keep within viewport
    if (left + popupWidth > window.innerWidth - 16) {
        left = window.innerWidth - popupWidth - 16;
    }
    if (top + 340 > window.innerHeight) {
        top = rect.top - 340;
    }
    popup.style.left = `${left}px`;
    popup.style.top = `${top}px`;

    // Set preview boxes
    elements.popupOriginalColor.style.background = hex;
    elements.popupLiveColor.style.background = hex;

    // Init sliders from HSL
    const hsl = hexToHsl(hex);
    elements.popupHue.value = hsl.h;
    elements.popupSat.value = hsl.s;
    elements.popupLit.value = hsl.l;
    elements.popupHueVal.textContent = hsl.h;
    elements.popupSatVal.textContent = hsl.s + '%';
    elements.popupLitVal.textContent = hsl.l + '%';
    elements.popupHexInput.value = hex;

    popup.classList.remove('hidden');
}

function closeColorPopup() {
    state.colorPopup.visible = false;
    elements.colorPopup.classList.add('hidden');
}

function updateColorFromSliders() {
    const h = parseInt(elements.popupHue.value);
    const s = parseInt(elements.popupSat.value);
    const l = parseInt(elements.popupLit.value);
    elements.popupHueVal.textContent = h;
    elements.popupSatVal.textContent = s + '%';
    elements.popupLitVal.textContent = l + '%';

    const hex = hslToHex(h, s, l);
    elements.popupHexInput.value = hex;
    elements.popupLiveColor.style.background = hex;
    applyColorChange(hex);
}

function updateColorFromHex() {
    let hex = elements.popupHexInput.value.trim();
    if (!hex.startsWith('#')) hex = '#' + hex;
    if (!/^#[0-9A-Fa-f]{6}$/.test(hex)) return;

    hex = hex.toUpperCase();
    const hsl = hexToHsl(hex);
    elements.popupHue.value = hsl.h;
    elements.popupSat.value = hsl.s;
    elements.popupLit.value = hsl.l;
    elements.popupHueVal.textContent = hsl.h;
    elements.popupSatVal.textContent = hsl.s + '%';
    elements.popupLitVal.textContent = hsl.l + '%';
    elements.popupLiveColor.style.background = hex;
    applyColorChange(hex);
}

let applyDebounceTimer = null;

function applyColorChange(newHex) {
    const index = state.colorPopup.colorIndex;
    const panelOption = elements.panelSelect.selectedOptions[0];
    if (!panelOption || !panelOption.dataset.colorsActual) return;

    // Update the stored actual colors
    const actualColors = panelOption.dataset.colorsActual.split(',').map(c => c.trim());
    actualColors[index] = newHex;
    panelOption.dataset.colorsActual = actualColors.join(',');

    // Update swatch backgrounds
    updateColorSwatches();

    // Track override per-device for localStorage persistence
    const selected = elements.screenSelect.value;
    const deviceKey = selected.startsWith('device:') ? selected.slice('device:'.length) : '';
    if (deviceKey) {
        state.colorOverrides[deviceKey] = actualColors.join(',');
        saveState();
    }
    clearTimeout(applyDebounceTimer);
    applyDebounceTimer = setTimeout(async () => {
        if (deviceKey) {
            try {
                await fetch('/dev/panel-colors', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        device: deviceKey,
                        colors_actual: actualColors.join(','),
                    }),
                });
            } catch (e) {
                console.error('Failed to update panel colors:', e);
            }
        }
        render();
    }, 300);
}

function copyActualColors() {
    const panelOption = elements.panelSelect.selectedOptions[0];
    if (!panelOption || !panelOption.dataset.colorsActual) return;
    const text = panelOption.dataset.colorsActual;
    copyToClipboard(text).then(() => {
        const btn = document.querySelector('.copy-colors-btn');
        if (btn) {
            btn.textContent = 'Copied!';
            btn.classList.add('copied');
            setTimeout(() => {
                btn.textContent = 'Copy color config';
                btn.classList.remove('copied');
            }, 1500);
        }
    });
}

// Clipboard helper — navigator.clipboard requires a secure context (HTTPS or
// localhost).  The dev server runs plain HTTP, so fall back to execCommand.
function copyToClipboard(text) {
    if (navigator.clipboard && window.isSecureContext) {
        return navigator.clipboard.writeText(text);
    }
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.left = '-9999px';
    document.body.appendChild(ta);
    ta.select();
    document.execCommand('copy');
    document.body.removeChild(ta);
    return Promise.resolve();
}

// Setup popup event listeners (called from init)
function setupColorPopup() {
    elements.popupClose.addEventListener('click', closeColorPopup);
    elements.popupHue.addEventListener('input', updateColorFromSliders);
    elements.popupSat.addEventListener('input', updateColorFromSliders);
    elements.popupLit.addEventListener('input', updateColorFromSliders);
    elements.popupHexInput.addEventListener('input', updateColorFromHex);
    elements.popupResetBtn.addEventListener('click', () => {
        const hex = state.colorPopup.originalHex;
        elements.popupHexInput.value = hex;
        updateColorFromHex();
    });

    // Close popup when clicking outside
    document.addEventListener('click', (e) => {
        if (!state.colorPopup.visible) return;
        if (elements.colorPopup.contains(e.target)) return;
        // Don't close when clicking a swatch (will re-open)
        if (e.target.classList.contains('swatch') && e.target.closest('.swatch-row.actual')) return;
        closeColorPopup();
    });
}

// Start the app
init();
