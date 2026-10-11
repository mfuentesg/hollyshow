//! mDNS advertisement: `hollyshow.local` resolves to this machine and the service
//! `_hollyshow._tcp` lets future clients discover it without typing an address.

use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::net::IpAddr;

pub const HOSTNAME: &str = "hollyshow.local";
const SERVICE_TYPE: &str = "_hollyshow._tcp.local.";

pub struct Mdns {
    daemon: ServiceDaemon,
}

impl Mdns {
    /// Starts advertising on `ips`. Failure (no multicast, sandbox...) is returned so the caller
    /// can log it and carry on: the server works fine by IP address.
    pub fn advertise(instance: &str, port: u16, ips: &[IpAddr]) -> Result<Self, mdns_sd::Error> {
        let daemon = ServiceDaemon::new()?;
        let props = [("proto", hollyshow_core::PROTOCOL_VERSION.to_string())];
        let host = format!("{HOSTNAME}.");
        let info = ServiceInfo::new(SERVICE_TYPE, instance, &host, ips, port, &props[..])?;
        if let Err(e) = daemon.register(info) {
            let _ = daemon.shutdown();
            return Err(e);
        }
        Ok(Self { daemon })
    }
}

impl Drop for Mdns {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}
