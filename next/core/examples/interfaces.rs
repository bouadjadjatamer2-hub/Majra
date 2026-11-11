use std::net::{IpAddr, UdpSocket};

fn main() {
    for interface in majra_core::list_interfaces() {
        let route = interface
            .address
            .parse::<IpAddr>()
            .ok()
            .and_then(|address| UdpSocket::bind((address, 0)).ok())
            .and_then(|socket| socket.connect("1.1.1.1:53").ok().map(|_| socket));
        let local = route
            .as_ref()
            .and_then(|socket| socket.local_addr().ok())
            .map(|address| address.to_string())
            .unwrap_or_else(|| "no route".into());
        println!(
            "id={} name={} address={} kind={:?} default={} usable={} network={:?} route={}",
            interface.id,
            interface.name,
            interface.address,
            interface.kind,
            interface.is_default,
            interface.usable,
            interface.network,
            local
        );
    }
}
