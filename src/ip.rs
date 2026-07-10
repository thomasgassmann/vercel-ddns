use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};

use futures::StreamExt;
use public_ip::Version;

pub fn is_public_ipv4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    // 100.64.0.0/10 (CGNAT) has no stable is_shared() yet.
    let cgnat = octets[0] == 100 && (64..128).contains(&octets[1]);
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || cgnat)
}

pub fn is_global_ipv6(ip: &Ipv6Addr) -> bool {
    // Global unicast is currently allocated out of 2000::/3; this excludes
    // loopback, link-local, ULA, multicast and v4-mapped addresses in one test.
    (ip.segments()[0] & 0xe000) == 0x2000
}

/// First public IPv4 in a dyndns2 `myip` parameter (routers may send a
/// comma-separated list, possibly mixing in IPv6).
pub fn parse_myip(raw: &str) -> Option<Ipv4Addr> {
    raw.split(',')
        .filter_map(|part| part.trim().parse::<Ipv4Addr>().ok())
        .find(is_public_ipv4)
}

/// Router-provided address if we got one, otherwise ask multiple public
/// resolvers (public-ip crate) so no single resolver can spoof us.
pub async fn resolve_ipv4(hint: Option<Ipv4Addr>) -> Option<Ipv4Addr> {
    if hint.is_some() {
        return hint;
    }

    // TODO: does this really query multiple resolvers and compare them?
    public_ip::addr_v4().await
}

/// The host's own global IPv6, read from the kernel's source-address choice
/// for an outbound route. connect() on UDP sends no packets.
/// TODO: does this relliably work in a vm on proxmox?
pub fn local_ipv6() -> Option<Ipv6Addr> {
    let socket = UdpSocket::bind(("::", 0)).ok()?;
    socket.connect("[2001:4860:4860::8888]:53").ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V6(addr) if is_global_ipv6(addr.ip()) => Some(*addr.ip()),
        _ => None,
    }
}

pub async fn resolve_host_ipv6() -> Option<Ipv6Addr> {
    if let Some(ip) = local_ipv6() {
        return Some(ip);
    }

    // TODO: does this really query multiple resolvers and compare them?
    public_ip::addr_v6().await
}
