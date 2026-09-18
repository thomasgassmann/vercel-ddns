use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};

use futures::StreamExt;
use public_ip::{Version, dns, http};

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

/// Akamai answers `whoami.akamai.net` (IN A) with the querying address.
const DNS_AKAMAI_V4: &dyn public_ip::Resolver<'static> = &dns::Resolver::new_static(
    "whoami.akamai.net",
    &[IpAddr::V4(Ipv4Addr::new(193, 108, 91, 1))], // ns1-1.akamaitech.net
    53,
    dns::QueryMethod::A,
);

const HTTP_ICANHAZIP_V4: &dyn public_ip::Resolver<'static> =
    &http::Resolver::new_static("http://ipv4.icanhazip.com", http::ExtractMethod::PlainText);
const HTTP_ICANHAZIP_V6: &dyn public_ip::Resolver<'static> =
    &http::Resolver::new_static("http://ipv6.icanhazip.com", http::ExtractMethod::PlainText);
const HTTP_IDENT_ME_V4: &dyn public_ip::Resolver<'static> =
    &http::Resolver::new_static("http://v4.ident.me", http::ExtractMethod::PlainText);
const HTTP_IDENT_ME_V6: &dyn public_ip::Resolver<'static> =
    &http::Resolver::new_static("http://v6.ident.me", http::ExtractMethod::PlainText);
const HTTP_CHECKIP_AMAZONAWS_COM: &dyn public_ip::Resolver<'static> = &http::Resolver::new_static(
    "http://checkip.amazonaws.com",
    http::ExtractMethod::PlainText,
);

/// Eight independent providers across two protocols: three DNS-based
/// (OpenDNS, Google DNS, Akamai) and five HTTP-based (ipify,
/// whatismyipaddress, icanhazip, ident.me, AWS). public-ip's builtin `ALL`
/// only covers the first four of these.
const RESOLVERS: &dyn public_ip::Resolver<'static> = &&[
    dns::OPENDNS,
    dns::GOOGLE,
    DNS_AKAMAI_V4,
    http::HTTP_IPIFY_ORG,
    http::HTTP_WHATISMYIPADDRESS_COM,
    HTTP_ICANHAZIP_V4,
    HTTP_ICANHAZIP_V6,
    HTTP_IDENT_ME_V4,
    HTTP_IDENT_ME_V6,
    HTTP_CHECKIP_AMAZONAWS_COM,
];

/// Queries all resolvers above and takes the address with the most votes,
/// requiring at least two agreeing answers. public_ip::addr_v4() alone would
/// return the first answer to arrive, which a single malicious resolver could
/// spoof.
async fn consensus_addr(version: Version) -> Option<IpAddr> {
    let answers: Vec<IpAddr> = public_ip::resolve(RESOLVERS, version)
        .filter_map(|result| async move { result.ok().map(|(addr, _)| addr) })
        .collect()
        .await;

    let mut votes: HashMap<IpAddr, usize> = HashMap::new();
    for addr in &answers {
        *votes.entry(*addr).or_default() += 1;
    }
    let (addr, count) = votes.into_iter().max_by_key(|(_, count)| *count)?;
    if count < 2 {
        tracing::warn!(
            ?version,
            ?answers,
            "no two resolvers agree on the public IP, not updating"
        );
        return None;
    }
    tracing::debug!(?version, %addr, agreeing = count, total = answers.len(), "public IP resolved by consensus");
    Some(addr)
}

/// Router-provided address if we got one, otherwise multi-resolver consensus.
pub async fn resolve_ipv4(hint: Option<Ipv4Addr>) -> Option<Ipv4Addr> {
    if hint.is_some() {
        return hint;
    }
    match consensus_addr(Version::V4).await? {
        IpAddr::V4(addr) => Some(addr),
        IpAddr::V6(_) => None,
    }
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
    match consensus_addr(Version::V6).await? {
        IpAddr::V6(addr) => Some(addr),
        IpAddr::V4(_) => None,
    }
}
