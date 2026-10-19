use mazbri_core::metrics::health::BoxedConfigCenterCheck;
use mazbri_registry::{ConfigCenterRegistry, TomlConfigHandler};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type TomlServiceConfigHandler<T> = TomlConfigHandler<T>;

pub fn create_toml_config_handler<T>(
    service_name: &'static str,
) -> Arc<TomlServiceConfigHandler<T>> {
    Arc::new(TomlServiceConfigHandler::new(service_name))
}

pub fn make_config_center_check(
    registry: Option<&Arc<ConfigCenterRegistry>>,
) -> Option<BoxedConfigCenterCheck> {
    registry.map(|reg| {
        let cm = reg.connection_manager().clone();
        Arc::new(move || {
            let cm = cm.clone();
            Box::pin(async move { cm.is_connected().await })
                as Pin<Box<dyn Future<Output = bool> + Send>>
        }) as BoxedConfigCenterCheck
    })
}
