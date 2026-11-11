use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InterfaceKind {
    Wifi,
    Ethernet,
    Usb,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceInfo {
    pub id: String,
    pub name: String,
    pub address: String,
    pub kind: InterfaceKind,
    /// True for the adapter Windows would use for an unbound connection. Detected with a
    /// route lookup only (a UDP socket that is never sent to), so it costs no traffic.
    pub is_default: bool,
    /// True when Windows has an outbound route from this address. A disconnected or
    /// host-only virtual adapter can keep a private address but has no route; offering it
    /// as a download path would fail every block assigned to it.
    #[serde(default)]
    pub usable: bool,
    /// IPv4 network in CIDR form, e.g. `192.168.1.0/24`. Two adapters on the same network
    /// reach the same router, so spreading connections over them adds no bandwidth.
    #[serde(default)]
    pub network: Option<String>,
}

fn classify(name: &str) -> InterfaceKind {
    let name = name.to_lowercase();
    if name.contains("wi-fi")
        || name.contains("wifi")
        || name.contains("wlan")
        || name.contains("wireless")
        || name.contains("802.11")
    {
        InterfaceKind::Wifi
    } else if name.contains("usb")
        || name.contains("rndis")
        || name.contains("remote ndis")
        || name.contains("tether")
        || name.contains("android")
        || name.contains("iphone")
        || name.contains("mobile")
        || name.contains("cellular")
        || name.contains("wwan")
    {
        InterfaceKind::Usb
    } else if name.contains("ethernet")
        || name.contains("eth")
        || name.contains("local area connection")
        || name.contains("gbe")
    {
        InterfaceKind::Ethernet
    } else {
        InterfaceKind::Other
    }
}

/// The address the OS would pick for a normal outbound connection. Connecting a UDP socket
/// only performs a route lookup; no packet leaves the machine.
fn default_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:53").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(address) => Some(address),
        IpAddr::V6(_) => None,
    }
}

fn network_of(address: Ipv4Addr, netmask: Ipv4Addr) -> Option<String> {
    let mask = u32::from(netmask);
    let prefix = mask.count_ones();
    if prefix == 0 || prefix >= 32 {
        return None;
    }
    // A non-contiguous mask has no CIDR form; refuse it instead of printing a wrong prefix.
    if mask != u32::MAX << (32 - prefix) {
        return None;
    }
    let network = Ipv4Addr::from(u32::from(address) & mask);
    Some(format!("{network}/{prefix}"))
}

/// A route lookup from one interface address to a public address. Connecting a UDP
/// socket only asks the routing table; no packet leaves the machine.
fn has_route(address: Ipv4Addr) -> bool {
    let Ok(socket) = UdpSocket::bind((address, 0)) else {
        return false;
    };
    socket.connect("1.1.1.1:53").is_ok()
}

/// Every non-loopback IPv4 adapter, including ones Windows currently cannot route.
/// The UI can show a dead adapter as "no route" instead of silently offering a path that
/// would fail every block. Loopback is never a second path to the internet.
pub fn list_interfaces() -> Vec<InterfaceInfo> {
    let mut out = Vec::new();
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return out;
    };
    let default = default_address();

    for interface in interfaces {
        if interface.is_loopback() {
            continue;
        }
        let if_addrs::IfAddr::V4(v4) = interface.addr else {
            continue;
        };
        if v4.ip.is_unspecified() {
            continue;
        }

        let address = v4.ip;
        let name = interface.name;
        let kind = classify(&name);
        let network = network_of(address, v4.netmask);
        out.push(InterfaceInfo {
            id: format!("{name}|{address}"),
            name,
            address: address.to_string(),
            kind,
            is_default: default == Some(address),
            usable: has_route(address),
            network,
        });
    }

    // A ready adapter first, then the Windows default, then a stable name order.
    out.sort_by(|a, b| {
        b.usable
            .cmp(&a.usable)
            .then(b.is_default.cmp(&a.is_default))
            .then(a.name.cmp(&b.name))
            .then(a.address.cmp(&b.address))
    });
    out
}

/// Prove that one adapter can really reach the internet: bind a TCP socket to its
/// address and open a connection to a public resolver. Unlike a route lookup this also
/// catches an adapter whose router is up but has no working uplink.
pub async fn test_interface(address: &str) -> Result<u64, String> {
    use tokio::net::TcpSocket;

    let ip = address
        .parse::<Ipv4Addr>()
        .map_err(|_| "invalid IPv4 address".to_string())?;
    let socket = TcpSocket::new_v4().map_err(|error| error.to_string())?;
    socket
        .bind(SocketAddr::new(IpAddr::V4(ip), 0))
        .map_err(|error| format!("could not bind {ip}: {error}"))?;
    let target = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 443);
    let start = Instant::now();
    match tokio::time::timeout(Duration::from_secs(6), socket.connect(target)).await {
        Ok(Ok(stream)) => {
            drop(stream);
            Ok(start.elapsed().as_millis() as u64)
        }
        Ok(Err(error)) => Err(format!("connection failed: {error}")),
        Err(_) => Err("connection timed out".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_windows_adapter_names() {
        assert_eq!(classify("Wi-Fi"), InterfaceKind::Wifi);
        assert_eq!(
            classify("Intel(R) Wi-Fi 6 AX201 160MHz"),
            InterfaceKind::Wifi
        );
        assert_eq!(classify("Ethernet 2"), InterfaceKind::Ethernet);
        assert_eq!(
            classify("Realtek PCIe GbE Family Controller"),
            InterfaceKind::Ethernet
        );
        assert_eq!(
            classify("Remote NDIS based Internet Sharing Device"),
            InterfaceKind::Usb
        );
        assert_eq!(classify("USB Tethering"), InterfaceKind::Usb);
        assert_eq!(classify("Samsung Android Phone"), InterfaceKind::Usb);
    }

    #[test]
    fn loopback_has_no_public_route() {
        assert!(!has_route(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn builds_the_network_prefix() {
        assert_eq!(
            network_of(
                Ipv4Addr::new(192, 168, 1, 4),
                Ipv4Addr::new(255, 255, 255, 0)
            ),
            Some("192.168.1.0/24".into())
        );
        assert_eq!(
            network_of(Ipv4Addr::new(10, 42, 0, 7), Ipv4Addr::new(255, 255, 0, 0)),
            Some("10.42.0.0/16".into())
        );
        assert_eq!(
            network_of(
                Ipv4Addr::new(172, 20, 5, 9),
                Ipv4Addr::new(255, 255, 255, 252)
            ),
            Some("172.20.5.8/30".into())
        );
        assert_eq!(
            network_of(Ipv4Addr::new(1, 2, 3, 4), Ipv4Addr::new(0, 0, 0, 0)),
            None
        );
        assert_eq!(
            network_of(Ipv4Addr::new(1, 2, 3, 4), Ipv4Addr::new(255, 0, 255, 0)),
            None
        );
    }
}
