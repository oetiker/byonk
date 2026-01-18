// Byonk Dev Mode JavaScript

const state = {
    screens: [],
    defaultScreen: null,
    eventSource: null,
    isRendering: false,
};

// DOM elements
const elements = {
    screenSelect: document.getElementById('screen-select'),
    macInput: document.getElementById('mac-input'),
    macResolved: document.getElementById('mac-resolved'),
    modelSelect: document.getElementById('model-select'),
    widthInput: document.getElementById('width-input'),
    heightInput: document.getElementById('height-input'),
    batteryInput: document.getElementById('battery-input'),
    rssiInput: document.getElementById('rssi-input'),
    timeInput: document.getElementById('time-input'),
    greyLevelsSelect: document.getElementById('grey-levels-select'),
    paramsInput: document.getElementById('params-input'),
    renderBtn: document.getElementById('render-btn'),
    autoRefresh: document.getElementById('auto-refresh'),
    displayImage: document.getElementById('display-image'),
    loadingOverlay: document.getElementById('loading-overlay'),
    deviceFrame: document.getElementById('device-frame'),
    deviceBezel: document.querySelector('.device-bezel'),
    displaySize: document.getElementById('display-size'),
    renderTime: document.getElementById('render-time'),
    watchStatus: document.getElementById('watch-status'),
    errorPanel: document.getElementById('error-panel'),
    errorContent: document.getElementById('error-content'),
    dismissError: document.getElementById('dismiss-error'),
};

// Initialize
async function init() {
    await loadScreens();
    setupEventListeners();
    connectSSE();
    initializeTimeInput();
    loadSavedState();
    setupLens();
    render();
}

// Initialize time input with current local datetime
function initializeTimeInput() {
    const now = new Date();
    // Format as YYYY-MM-DDTHH:MM for datetime-local input
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const day = String(now.getDate()).padStart(2, '0');
    const hours = String(now.getHours()).padStart(2, '0');
    const minutes = String(now.getMinutes()).padStart(2, '0');
    elements.timeInput.value = `${year}-${month}-${day}T${hours}:${minutes}`;
}

// Load available screens from server
async function loadScreens() {
    try {
        const response = await fetch('/dev/screens');
        const data = await response.json();
        state.screens = data.screens;
        state.defaultScreen = data.default_screen;

        // Populate screen select
        elements.screenSelect.innerHTML = '';
        state.screens.forEach(screen => {
            const option = document.createElement('option');
            option.value = screen.name;
            option.textContent = screen.name;
            if (screen.name === state.defaultScreen) {
                option.textContent += ' (default)';
            }
            elements.screenSelect.appendChild(option);
        });

        // Select default screen
        if (state.defaultScreen) {
            elements.screenSelect.value = state.defaultScreen;
        }
    } catch (error) {
        console.error('Failed to load screens:', error);
        showError('Failed to load screens: ' + error.message);
    }
}

// Setup event listeners
function setupEventListeners() {
    // Model select changes dimensions and grey levels
    elements.modelSelect.addEventListener('change', (e) => {
        const model = e.target.value;
        if (model === 'x') {
            elements.widthInput.value = 1872;
            elements.heightInput.value = 1404;
            elements.greyLevelsSelect.value = '16';
        } else {
            elements.widthInput.value = 800;
            elements.heightInput.value = 480;
            elements.greyLevelsSelect.value = '4';
        }
        updateDeviceFrame();
        saveState();
        if (elements.autoRefresh.checked) {
            render();
        }
    });

    // Dimension changes
    elements.widthInput.addEventListener('change', () => {
        updateDeviceFrame();
        saveState();
    });
    elements.heightInput.addEventListener('change', () => {
        updateDeviceFrame();
        saveState();
    });

    // Screen select
    elements.screenSelect.addEventListener('change', () => {
        saveState();
        if (elements.autoRefresh.checked) {
            render();
        }
    });

    // MAC address input (debounced resolution)
    let macTimeout;
    elements.macInput.addEventListener('input', () => {
        clearTimeout(macTimeout);
        macTimeout = setTimeout(async () => {
            await resolveMac();
            saveState();
            if (elements.autoRefresh.checked && elements.macInput.value.trim()) {
                render();
            }
        }, 500);
    });

    // Battery, RSSI, Time, Grey levels changes
    elements.batteryInput.addEventListener('change', () => {
        saveState();
        if (elements.autoRefresh.checked) {
            render();
        }
    });
    elements.rssiInput.addEventListener('change', () => {
        saveState();
        if (elements.autoRefresh.checked) {
            render();
        }
    });
    elements.timeInput.addEventListener('change', () => {
        saveState();
        if (elements.autoRefresh.checked) {
            render();
        }
    });
    elements.greyLevelsSelect.addEventListener('change', () => {
        saveState();
        if (elements.autoRefresh.checked) {
            render();
        }
    });

    // Render button
    elements.renderBtn.addEventListener('click', render);

    // Dismiss error
    elements.dismissError.addEventListener('click', hideError);

    // Auto-refresh toggle
    elements.autoRefresh.addEventListener('change', saveState);

    // Params input (debounced)
    let paramsTimeout;
    elements.paramsInput.addEventListener('input', () => {
        clearTimeout(paramsTimeout);
        paramsTimeout = setTimeout(() => {
            saveState();
        }, 500);
    });
}

// Resolve MAC address to screen and params
async function resolveMac() {
    const mac = elements.macInput.value.trim();
    if (!mac) {
        elements.macResolved.classList.add('hidden');
        elements.macResolved.textContent = '';
        return;
    }

    try {
        const response = await fetch(`/dev/resolve-mac?mac=${encodeURIComponent(mac)}`);
        if (response.ok) {
            const data = await response.json();
            elements.macResolved.classList.remove('hidden');
            elements.macResolved.classList.remove('error');
            elements.macResolved.textContent = `→ ${data.screen}`;
            if (data.params && Object.keys(data.params).length > 0) {
                elements.macResolved.textContent += ` (${Object.keys(data.params).length} params)`;
            }
        } else {
            elements.macResolved.classList.remove('hidden');
            elements.macResolved.classList.add('error');
            elements.macResolved.textContent = 'MAC not configured';
        }
    } catch (e) {
        elements.macResolved.classList.remove('hidden');
        elements.macResolved.classList.add('error');
        elements.macResolved.textContent = 'Resolution failed';
    }
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
        console.log('Files changed:', files);

        if (elements.autoRefresh.checked) {
            render();
        }
    });

    state.eventSource.addEventListener('refresh', () => {
        if (elements.autoRefresh.checked) {
            render();
        }
    });
}

// Update device frame class based on model
function updateDeviceFrame() {
    const model = elements.modelSelect.value;
    elements.deviceFrame.classList.remove('og', 'x');
    elements.deviceFrame.classList.add(model);

    const width = elements.widthInput.value;
    const height = elements.heightInput.value;
    elements.displaySize.textContent = `${width} x ${height}`;
}

// Render the current screen
async function render() {
    if (state.isRendering) return;

    const mac = elements.macInput.value.trim();
    const screen = elements.screenSelect.value;

    // Need either a screen selected or a MAC address
    if (!screen && !mac) return;

    state.isRendering = true;
    elements.loadingOverlay.classList.remove('hidden');
    elements.renderBtn.disabled = true;
    hideError();

    const startTime = performance.now();

    try {
        // Validate params JSON
        let params = '{}';
        try {
            const parsed = JSON.parse(elements.paramsInput.value || '{}');
            params = JSON.stringify(parsed);
        } catch (e) {
            showError('Invalid JSON in parameters: ' + e.message);
            return;
        }

        // Convert local datetime to UTC timestamp
        let timestamp = '';
        if (elements.timeInput.value) {
            const localDate = new Date(elements.timeInput.value);
            timestamp = Math.floor(localDate.getTime() / 1000).toString();
        }

        const queryParams = new URLSearchParams({
            model: elements.modelSelect.value,
            width: elements.widthInput.value,
            height: elements.heightInput.value,
            battery_voltage: elements.batteryInput.value,
            rssi: elements.rssiInput.value,
            grey_levels: elements.greyLevelsSelect.value,
            params,
        });

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
    } catch (error) {
        console.error('Render error:', error);
        showError(error.message);
    } finally {
        state.isRendering = false;
        elements.loadingOverlay.classList.add('hidden');
        elements.renderBtn.disabled = false;
    }
}

// Show error panel
function showError(message) {
    elements.errorContent.textContent = message;
    elements.errorPanel.classList.remove('hidden');
}

// Hide error panel
function hideError() {
    elements.errorPanel.classList.add('hidden');
}

// Save state to localStorage
function saveState() {
    const savedState = {
        screen: elements.screenSelect.value,
        mac: elements.macInput.value,
        model: elements.modelSelect.value,
        width: elements.widthInput.value,
        height: elements.heightInput.value,
        battery: elements.batteryInput.value,
        rssi: elements.rssiInput.value,
        time: elements.timeInput.value,
        greyLevels: elements.greyLevelsSelect.value,
        params: elements.paramsInput.value,
        autoRefresh: elements.autoRefresh.checked,
    };
    localStorage.setItem('byonk-dev-state', JSON.stringify(savedState));
}

// Load state from localStorage
function loadSavedState() {
    try {
        const saved = localStorage.getItem('byonk-dev-state');
        if (saved) {
            const data = JSON.parse(saved);
            if (data.screen && state.screens.find(s => s.name === data.screen)) {
                elements.screenSelect.value = data.screen;
            }
            if (data.mac) {
                elements.macInput.value = data.mac;
                resolveMac(); // Resolve the MAC to show feedback
            }
            if (data.model) {
                elements.modelSelect.value = data.model;
            }
            if (data.width) {
                elements.widthInput.value = data.width;
            }
            if (data.height) {
                elements.heightInput.value = data.height;
            }
            if (data.battery) {
                elements.batteryInput.value = data.battery;
            }
            if (data.rssi) {
                elements.rssiInput.value = data.rssi;
            }
            if (data.time) {
                elements.timeInput.value = data.time;
            }
            if (data.greyLevels) {
                elements.greyLevelsSelect.value = data.greyLevels;
            }
            if (data.params) {
                elements.paramsInput.value = data.params;
            }
            if (typeof data.autoRefresh === 'boolean') {
                elements.autoRefresh.checked = data.autoRefresh;
            }
            updateDeviceFrame();
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
    // Only show lens for X model (high-res)
    if (elements.modelSelect.value !== 'x') return;
    if (!fullResImage.complete || !fullResImage.naturalWidth) return;

    lensElement.classList.remove('hidden');
    moveLens(e);
}

function hideLens() {
    lensElement.classList.add('hidden');
}

function moveLens(e) {
    if (elements.modelSelect.value !== 'x') {
        hideLens();
        return;
    }
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

    // Source coordinates in the full-res image (centered on cursor position)
    const srcX = (imgX * scaleX) - (sourceSize / 2);
    const srcY = (imgY * scaleY) - (sourceSize / 2);

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
        srcX, srcY, sourceSize, sourceSize,  // Source rectangle (100x100)
        0, 0, lensSize, lensSize              // Destination rectangle (200x200 = 2x magnification)
    );

    lensCtx.restore();
}

// Start the app
init();
