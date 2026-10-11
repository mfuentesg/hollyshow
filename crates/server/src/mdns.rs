//! mDNS advertisement: `hollyshow.local` resolves to this machine and the service
//! `_hollyshow._tcp` lets future clients discover it without typing an address.

use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::net::IpAddr;

pub const HOSTNAME: &str = "hollyshow.local";
const SERVICE_TYPE: &str = "_hollyshow._tcp.local.";

pub struct Mdns {
    daemon: ServiceDaemon,
    instance: String,
    port: u16,
    /// Addresses currently announced; empty when the service is withdrawn.
    ips: Vec<IpAddr>,
}

impl Mdns {
    /// Starts the responder and advertises on `ips`. Failure (no multicast, sandbox...) is
    /// returned so the caller can log it and carry on: the server works fine by IP address.
    pub fn advertise(instance: &str, port: u16, ips: &[IpAddr]) -> Result<Self, mdns_sd::Error> {
        let mut m = Self {
            daemon: ServiceDaemon::new()?,
            instance: instance.to_owned(),
            port,
            ips: Vec::new(),
        };
        m.set_addresses(ips)?;
        Ok(m)
    }

    fn info(&self, ips: &[IpAddr]) -> Result<ServiceInfo, mdns_sd::Error> {
        let props = [("proto", hollyshow_core::PROTOCOL_VERSION.to_string())];
        let host = format!("{HOSTNAME}.");
        ServiceInfo::new(
            SERVICE_TYPE,
            &self.instance,
            &host,
            ips,
            self.port,
            &props[..],
        )
    }

    /// Re-announces the service on a new address set (the Wi-Fi changed, DHCP renewed...).
    /// An empty set withdraws it, so peers stop resolving `hollyshow.local` to a dead address.
    pub fn set_addresses(&mut self, ips: &[IpAddr]) -> Result<(), mdns_sd::Error> {
        if ips == self.ips.as_slice() {
            return Ok(());
        }
        if ips.is_empty() {
            let fullname = self.info(&self.ips)?.get_fullname().to_owned();
            let _ = self.daemon.unregister(&fullname);
        } else {
            // `register` again re-announces with the updated records.
            self.daemon.register(self.info(ips)?)?;
        }
        self.ips = ips.to_vec();
        Ok(())
    }

    pub fn is_advertising(&self) -> bool {
        !self.ips.is_empty()
    }
}

impl Drop for Mdns {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}
