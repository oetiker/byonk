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
    modelSelect: document.getElementById('model-select'),
    widthInput: document.getElementById('width-input'),
    heightInput: document.getElementById('height-input'),
    paramsInput: document.getElementById('params-input'),
    renderBtn: document.getElementById('render-btn'),
    autoRefresh: document.getElementById('auto-refresh'),
    displayImage: document.getElementById('display-image'),
    loadingOverlay: document.getElementById('loading-overlay'),
    deviceFrame: document.getElementById('device-frame'),
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
    loadSavedState();
    render();
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
    // Model select changes dimensions
    elements.modelSelect.addEventListener('change', (e) => {
        const model = e.target.value;
        if (model === 'x') {
            elements.widthInput.value = 1872;
            elements.heightInput.value = 1404;
        } else {
            elements.widthInput.value = 800;
            elements.heightInput.value = 480;
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

    const screen = elements.screenSelect.value;
    if (!screen) return;

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

        const queryParams = new URLSearchParams({
            screen,
            model: elements.modelSelect.value,
            width: elements.widthInput.value,
            height: elements.heightInput.value,
            params,
        });

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
        model: elements.modelSelect.value,
        width: elements.widthInput.value,
        height: elements.heightInput.value,
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
            if (data.model) {
                elements.modelSelect.value = data.model;
            }
            if (data.width) {
                elements.widthInput.value = data.width;
            }
            if (data.height) {
                elements.heightInput.value = data.height;
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

// Start the app
init();
