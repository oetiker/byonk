use crate::error::ApiError;
use crate::models::{ApiKey, Device, DeviceId};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for device storage - enables future DB migration
#[async_trait]
pub trait DeviceRegistry: Send + Sync {
    /// Register a new device or retrieve existing
    async fn register(&self, device_id: DeviceId, device: Device) -> Result<Device, ApiError>;

    /// Find device by ID (MAC address)
    async fn find_by_id(&self, device_id: &DeviceId) -> Result<Option<Device>, ApiError>;

    /// Find device by API key
    async fn find_by_api_key(&self, api_key: &ApiKey) -> Result<Option<Device>, ApiError>;

    /// Update device metadata (battery, RSSI, last_seen)
    async fn update(&self, device: Device) -> Result<(), ApiError>;
}

/// In-memory implementation (POC)
pub struct InMemoryRegistry {
    devices_by_id: Arc<RwLock<HashMap<DeviceId, Device>>>,
    devices_by_key: Arc<RwLock<HashMap<String, DeviceId>>>, // api_key -> device_id
}

impl InMemoryRegistry {
    pub fn new() -> Self {
        Self {
            devices_by_id: Arc::new(RwLock::new(HashMap::new())),
            devices_by_key: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DeviceRegistry for InMemoryRegistry {
    async fn register(&self, device_id: DeviceId, device: Device) -> Result<Device, ApiError> {
        let mut by_id = self.devices_by_id.write().await;
        let mut by_key = self.devices_by_key.write().await;

        // Check if already exists
        if let Some(existing) = by_id.get(&device_id) {
            return Ok(existing.clone());
        }

        // Insert new device
        by_key.insert(device.api_key.as_str().to_string(), device_id.clone());
        by_id.insert(device_id, device.clone());

        Ok(device)
    }

    async fn find_by_id(&self, device_id: &DeviceId) -> Result<Option<Device>, ApiError> {
        let by_id = self.devices_by_id.read().await;
        Ok(by_id.get(device_id).cloned())
    }

    async fn find_by_api_key(&self, api_key: &ApiKey) -> Result<Option<Device>, ApiError> {
        let by_key = self.devices_by_key.read().await;
        let by_id = self.devices_by_id.read().await;

        if let Some(device_id) = by_key.get(api_key.as_str()) {
            Ok(by_id.get(device_id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn update(&self, device: Device) -> Result<(), ApiError> {
        let mut by_id = self.devices_by_id.write().await;
        by_id.insert(device.device_id.clone(), device);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DeviceModel;

    #[tokio::test]
    async fn test_device_registration() {
        let registry = InMemoryRegistry::new();
        let device_id = DeviceId::new("AA:BB:CC:DD:EE:FF");
        let device = Device::new(device_id.clone(), DeviceModel::OG, "1.0.0".into());

        let registered = registry.register(device_id.clone(), device).await.unwrap();
        let found = registry.find_by_id(&device_id).await.unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().friendly_id, registered.friendly_id);
    }

    #[tokio::test]
    async fn test_find_by_api_key() {
        let registry = InMemoryRegistry::new();
        let device_id = DeviceId::new("AA:BB:CC:DD:EE:FF");
        let device = Device::new(device_id.clone(), DeviceModel::OG, "1.0.0".into());
        let api_key = device.api_key.clone();

        registry.register(device_id, device).await.unwrap();

        let found = registry.find_by_api_key(&api_key).await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_duplicate_registration_returns_existing() {
        let registry = InMemoryRegistry::new();
        let device_id = DeviceId::new("AA:BB:CC:DD:EE:FF");
        let device1 = Device::new(device_id.clone(), DeviceModel::OG, "1.0.0".into());
        let device2 = Device::new(device_id.clone(), DeviceModel::OG, "2.0.0".into());

        let registered1 = registry.register(device_id.clone(), device1).await.unwrap();
        let registered2 = registry.register(device_id, device2).await.unwrap();

        // Should return the original device, not create a new one
        assert_eq!(registered1.api_key.as_str(), registered2.api_key.as_str());
        assert_eq!(registered1.firmware_version, "1.0.0");
    }
}
