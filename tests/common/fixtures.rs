//! Test fixtures and constants.

/// Test MAC addresses for different scenarios
pub mod macs {
    /// Generic test device MAC
    pub const TEST_DEVICE: &str = "TE:ST:DE:VI:CE:01";

    /// MAC configured for hello screen in config.yaml
    pub const HELLO_DEVICE: &str = "TE:ST:HE:LL:00:00";

    /// MAC configured for graytest screen in config.yaml
    pub const GRAY_DEVICE: &str = "TE:ST:GR:AY:00:00";

    /// MAC configured for transit screen (requires external API)
    pub const TRANSIT_DEVICE: &str = "TR:AN:SI:T0:00:00";

    /// Unconfigured device (will use default screen)
    pub const UNKNOWN_DEVICE: &str = "UN:KN:OW:N0:00:00";
}

/// Build headers for /api/setup request
pub fn setup_headers(mac: &str) -> Vec<(&'static str, String)> {
    vec![
        ("ID", mac.to_string()),
        ("FW-Version", "1.7.1".to_string()),
        ("Model", "og".to_string()),
    ]
}

/// Build headers for /api/display request
pub fn display_headers(mac: &str, api_key: &str) -> Vec<(&'static str, String)> {
    vec![
        ("ID", mac.to_string()),
        ("Access-Token", api_key.to_string()),
        ("Width", "800".to_string()),
        ("Height", "480".to_string()),
        ("Battery-Voltage", "4.12".to_string()),
        ("RSSI", "-67".to_string()),
        ("FW-Version", "1.7.1".to_string()),
        ("Model", "og".to_string()),
        ("Host", "localhost:3000".to_string()),
    ]
}

/// Build headers for /api/display request with custom dimensions
pub fn display_headers_with_size(
    mac: &str,
    api_key: &str,
    width: u32,
    height: u32,
) -> Vec<(&'static str, String)> {
    vec![
        ("ID", mac.to_string()),
        ("Access-Token", api_key.to_string()),
        ("Width", width.to_string()),
        ("Height", height.to_string()),
        ("Battery-Voltage", "4.12".to_string()),
        ("RSSI", "-67".to_string()),
        ("FW-Version", "1.7.1".to_string()),
        ("Model", "og".to_string()),
        ("Host", "localhost:3000".to_string()),
    ]
}

/// Convert owned headers to borrowed for TestApp methods
pub fn as_str_pairs<'a>(headers: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    headers.iter().map(|(k, v)| (*k, v.as_str())).collect()
}
