//! Server configuration. Every limit is a field so tests (and the settings screen) can tune it.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

const DAY: u64 = 24 * 60 * 60;

/// Default TCP port of the LAN server.
pub const DEFAULT_PORT: u16 = 7420;

/// Default lifetime of a pairing offer.
pub const DEFAULT_PAIRING_TTL: Duration = Duration::from_secs(2 * 60);
/// Bounds of the user-configurable offer lifetime, in minutes.
pub const PAIRING_TTL_MINUTES: std::ops::RangeInclusive<u64> = 1..=10;

/// The offer lifetime for a number of minutes typed in Settings, forced into 1-10.
pub fn clamp_pairing_ttl(minutes: u64) -> Duration {
    Duration::from_secs(
        minutes.clamp(*PAIRING_TTL_MINUTES.start(), *PAIRING_TTL_MINUTES.end()) * 60,
    )
}

/// Optional HTTPS for the LAN server.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Where `cert.pem`, `key.pem` and `sans.txt` live (the app data directory).
    pub dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address to listen on. `0.0.0.0` listens everywhere but connections from non-private
    /// addresses are dropped at `accept` unless [`Self::allow_public_peers`] is set. Port `0`
    /// picks a free port (tests).
    pub bind: SocketAddr,
    pub server_name: String,
    /// Maximum simultaneous read-only congregation sockets.
    pub max_congregation: usize,
    /// Maximum simultaneous OBS / browser-source output pages.
    pub max_outputs: usize,
    /// Maximum paired devices kept; the least recently seen is forgotten beyond this.
    pub max_devices: usize,
    /// Total simultaneous sockets of every kind.
    pub max_connections: usize,
    /// Accept peers and a bind address outside private ranges. Never enable on purpose.
    pub allow_public_peers: bool,
    /// Whether congregation phones and OBS pages may join without pairing.
    pub congregation_open: bool,
    /// Advertise `hollyshow.local` over mDNS.
    pub mdns: bool,
    /// Heartbeat period (PLAN §3.4: 5 s). A client silent for three periods is dropped.
    pub heartbeat: Duration,
    /// How long a new socket may stay silent before sending `hello`.
    pub hello_timeout: Duration,
    /// How long one write to a client may take before the client is considered dead.
    pub write_timeout: Duration,
    /// Lifetime of a pairing offer (PIN and QR token, one-time use). Clamp user input with
    /// [`clamp_pairing_ttl`].
    pub pairing_ttl: Duration,
    /// Wrong credentials from one address before it is locked out.
    pub max_failures_per_ip: u32,
    /// How long a locked-out address stays locked.
    pub lockout: Duration,
    /// Serve HTTPS (self-signed certificate kept in `dir`) and redirect plain HTTP to it.
    /// `None`: plain HTTP only (the default).
    pub tls: Option<TlsConfig>,
    /// Sustained frames per second a socket may send (burst is `frame_burst`).
    pub frames_per_sec: f64,
    pub frame_burst: u32,
    /// HTTP API requests per address: burst and sustained rate.
    pub http_burst: u32,
    pub http_per_sec: f64,
    /// Raw TCP connections of every kind (half-open, idle, HTTP, WebSocket) accepted at once.
    /// Beyond this the socket is closed at `accept`, before any bytes are read, so a flood from
    /// the Wi-Fi cannot exhaust file descriptors.
    pub max_open_sockets: usize,
    /// Raw TCP connections one client address may hold open.
    pub max_sockets_per_ip: usize,
    /// How long a client may take to send the headers of one HTTP request (slowloris guard).
    pub header_read_timeout: Duration,
    /// How long a device's command-dedupe window outlives its socket, so a command resent after
    /// a reconnect is recognised as a duplicate instead of being applied twice.
    pub session_grace: Duration,
    /// A paired device unused for this long must pair again.
    pub device_idle_expiry: Duration,
    /// A paired device must pair again this long after it first did, however active.
    pub device_max_age: Duration,
    /// A session token older than this is replaced by a fresh one when the device reconnects.
    pub token_rotate_after: Duration,
    /// Minimum spacing between writes of a device's `last seen` stamp to storage.
    pub last_seen_write_interval: Duration,
    /// How often the supervisor re-reads the machine's LAN addresses.
    pub network_poll: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT)),
            server_name: "HollyShow".into(),
            max_congregation: 200,
            max_outputs: 16,
            max_devices: 64,
            max_connections: 512,
            allow_public_peers: false,
            congregation_open: true,
            mdns: true,
            heartbeat: Duration::from_secs(5),
            hello_timeout: Duration::from_secs(10),
            write_timeout: Duration::from_secs(10),
            pairing_ttl: DEFAULT_PAIRING_TTL,
            max_failures_per_ip: 5,
            lockout: Duration::from_secs(5 * 60),
            tls: None,
            frames_per_sec: 30.0,
            frame_burst: 60,
            http_burst: 30,
            http_per_sec: 10.0,
            max_open_sockets: 1024,
            max_sockets_per_ip: 64,
            header_read_timeout: Duration::from_secs(10),
            session_grace: Duration::from_secs(60),
            device_idle_expiry: Duration::from_secs(30 * DAY),
            device_max_age: Duration::from_secs(180 * DAY),
            token_rotate_after: Duration::from_secs(7 * DAY),
            last_seen_write_interval: Duration::from_secs(5 * 60),
            network_poll: Duration::from_secs(5),
        }
    }
}

impl ServerConfig {
    /// Silence tolerated from a client before it is dropped.
    pub fn idle_timeout(&self) -> Duration {
        self.heartbeat * 3
    }
}
