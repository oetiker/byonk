// Version selector for Byonk documentation
(function() {
    'use strict';

    // Detect current version from URL path
    function getCurrentVersion() {
        const path = window.location.pathname;
        const match = path.match(/\/byonk\/(v[\d.]+|dev)\//);
        return match ? match[1] : null;
    }

    // Create version selector dropdown
    function createVersionSelector(versions, currentVersion) {
        const container = document.createElement('div');
        container.className = 'version-selector';

        const select = document.createElement('select');
        select.id = 'version-select';
        select.setAttribute('aria-label', 'Select documentation version');

        versions.forEach(v => {
            const option = document.createElement('option');
            option.value = v.path;
            option.textContent = v.version + (v.prerelease ? ' (dev)' : '');
            if (v.version === currentVersion) {
                option.selected = true;
            }
            select.appendChild(option);
        });

        select.addEventListener('change', function() {
            const newPath = this.value;
            // Try to preserve the current page path
            const currentPath = window.location.pathname;
            const pageMatch = currentPath.match(/\/byonk\/(?:v[\d.]+|dev)\/(.*)$/);
            const page = pageMatch ? pageMatch[1] : '';
            window.location.href = newPath + page;
        });

        const label = document.createElement('span');
        label.className = 'version-label';
        label.textContent = 'Version: ';

        container.appendChild(label);
        container.appendChild(select);

        return container;
    }

    // Create dev warning banner
    function createDevBanner() {
        const banner = document.createElement('div');
        banner.className = 'dev-warning-banner';
        banner.innerHTML = `
            <strong>Development Version</strong>
            <span>You are viewing documentation for the development version.
            This may include unreleased features and changes.</span>
        `;
        return banner;
    }

    // Initialize version selector
    function init() {
        const currentVersion = getCurrentVersion();
        if (!currentVersion) return;

        // Fetch versions.json
        fetch('/byonk/versions.json')
            .then(response => response.json())
            .then(data => {
                // Insert version selector into the menu bar
                const menuBar = document.querySelector('.menu-bar');
                if (menuBar) {
                    const rightButtons = menuBar.querySelector('.right-buttons');
                    if (rightButtons) {
                        const selector = createVersionSelector(data.versions, currentVersion);
                        rightButtons.insertBefore(selector, rightButtons.firstChild);
                    }
                }

                // Show dev warning banner if on dev version
                if (currentVersion === 'dev') {
                    const main = document.querySelector('main') || document.querySelector('#content');
                    if (main) {
                        const banner = createDevBanner();
                        main.insertBefore(banner, main.firstChild);
                    }
                }
            })
            .catch(err => {
                console.warn('Could not load versions.json:', err);
            });
    }

    // Run on DOM ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
