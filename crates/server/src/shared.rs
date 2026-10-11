//! State shared by every route and socket.

use crate::config::ServerConfig;
use crate::connections::{Connections, Limits};
use crate::devices::DeviceStore;
use crate::pairing::Pairing;
use crate::ratelimit::IpLimiter;
use hollyshow_app::AppService;
use std::sync::atomic::{AtomicBool, AtomicU16};
use std::sync::Arc;
use tokio::sync::watch;

pub struct Shared {
    pub service: Arc<AppService>,
    pub config: ServerConfig,
    pub pairing: Pairing,
    pub devices: DeviceStore,
    pub conns: Arc<Connections>,
    pub http_limiter: IpLimiter,
    pub hello_limiter: IpLimiter,
    /// Flips to `true` when the server is shutting down; sockets close promptly.
    pub shutdown: watch::Sender<bool>,
    /// Actual listening port (differs from `config.bind` when that asked for port 0).
    pub port: AtomicU16,
    pub mdns_active: AtomicBool,
}

impl Shared {
    pub fn new(service: Arc<AppService>, config: ServerConfig) -> Arc<Self> {
        let settings = service.repos().settings.clone();
        let (shutdown, _) = watch::channel(false);
        Arc::new(Self {
            pairing: Pairing::new(
                config.pairing_ttl,
                config.pin_max_uses,
                config.max_failures_per_ip,
                config.lockout,
            ),
            devices: DeviceStore::load(settings, config.max_devices),
            conns: Connections::new(Limits {
                congregation: config.max_congregation,
                outputs: config.max_outputs,
                total: config.max_connections,
            }),
            http_limiter: IpLimiter::new(config.http_burst, config.http_per_sec),
            // Handshakes are rarer than frames; a reconnect storm still fits the burst.
            hello_limiter: IpLimiter::new(20, 2.0),
            shutdown,
            port: AtomicU16::new(config.bind.port()),
            mdns_active: AtomicBool::new(false),
            service,
            config,
        })
    }
}
