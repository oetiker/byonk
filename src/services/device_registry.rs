use crate::error::ApiError;
use crate::models::{Device, DeviceId};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for device metadata storage
#[async_trait]
pub trait DeviceRegistry: Send + Sync {
    /// Store or update device metadata
    async fn upsert(&self, device: Device) -> Result<(), ApiError>;

    /// Find device by ID (MAC address)
    async fn find_by_id(&self, device_id: &DeviceId) -> Result<Option<Device>, ApiError>;
}

/// In-memory device metadata storage
pub struct InMemoryRegistry {
    devices: Arc<RwLock<HashMap<DeviceId, Device>>>,
}

impl InMemoryRegistry {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
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
    async fn upsert(&self, device: Device) -> Result<(), ApiError> {
        let mut devices = self.devices.write().await;
        devices.insert(device.device_id.clone(), device);
        Ok(())
    }

    async fn find_by_id(&self, device_id: &DeviceId) -> Result<Option<Device>, ApiError> {
        let devices = self.devices.read().await;
        Ok(devices.get(device_id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DeviceModel;

    #[tokio::test]
    async fn test_upsert_and_find() {
        let registry = InMemoryRegistry::new();
        let device_id = DeviceId::new("AA:BB:CC:DD:EE:FF");
        let device = Device::new(device_id.clone(), DeviceModel::OG, "1.0.0".into());

        registry.upsert(device.clone()).await.unwrap();
        let found = registry.find_by_id(&device_id).await.unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().device_id, device.device_id);
    }

    #[tokio::test]
    async fn test_upsert_updates_existing() {
        let registry = InMemoryRegistry::new();
        let device_id = DeviceId::new("AA:BB:CC:DD:EE:FF");

        let mut device1 = Device::new(device_id.clone(), DeviceModel::OG, "1.0.0".into());
        device1.battery_voltage = Some(4.0);
        registry.upsert(device1).await.unwrap();

        let mut device2 = Device::new(device_id.clone(), DeviceModel::OG, "1.0.0".into());
        device2.battery_voltage = Some(3.5);
        registry.upsert(device2).await.unwrap();

        let found = registry.find_by_id(&device_id).await.unwrap().unwrap();
        assert_eq!(found.battery_voltage, Some(3.5));
    }
}
