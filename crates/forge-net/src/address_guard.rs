use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HostRejection {
    #[error("target resolves to a private or special address")]
    PrivateOrSpecial,
    #[error("target host did not resolve")]
    Unresolvable,
}

struct Ipv4Block {
    network: Ipv4Addr,
    prefix_len: u32,
}

impl Ipv4Block {
    const fn new(network: Ipv4Addr, prefix_len: u32) -> Self {
        Self {
            network,
            prefix_len,
        }
    }

    fn contains(&self, ip: Ipv4Addr) -> bool {
        let mask = u32::MAX
            .checked_shl(u32::BITS - self.prefix_len)
            .unwrap_or(0);
        u32::from(ip) & mask == u32::from(self.network) & mask
    }
}

const THIS_NETWORK: Ipv4Block = Ipv4Block::new(Ipv4Addr::new(0, 0, 0, 0), 8);
const SHARED_ADDRESS_SPACE: Ipv4Block = Ipv4Block::new(Ipv4Addr::new(100, 64, 0, 0), 10);
const BENCHMARKING: Ipv4Block = Ipv4Block::new(Ipv4Addr::new(198, 18, 0, 0), 15);
const RESERVED_FOR_FUTURE_USE: Ipv4Block = Ipv4Block::new(Ipv4Addr::new(240, 0, 0, 0), 4);
const SPECIAL_IPV4_BLOCKS: [Ipv4Block; 4] = [
    THIS_NETWORK,
    SHARED_ADDRESS_SPACE,
    BENCHMARKING,
    RESERVED_FOR_FUTURE_USE,
];

const NAT64_WELL_KNOWN_PREFIX: [u16; 6] = [0x64, 0xff9b, 0, 0, 0, 0];

fn unbracket(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host)
}

pub fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    unbracket(host)
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

pub fn ensure_public_ip_literal(host: &str) -> Result<(), HostRejection> {
    if host.is_empty() {
        return Err(HostRejection::Unresolvable);
    }
    match unbracket(host).parse::<IpAddr>() {
        Ok(ip) if is_private_or_special(ip) => Err(HostRejection::PrivateOrSpecial),
        Ok(_) | Err(_) => Ok(()),
    }
}

pub fn is_private_or_special(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(ip) => is_private_or_special_v4(ip),
        IpAddr::V6(ip) => is_private_or_special_v6(ip),
    }
}

fn is_private_or_special_v4(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || SPECIAL_IPV4_BLOCKS.iter().any(|block| block.contains(ip))
}

fn is_private_or_special_v6(ip: Ipv6Addr) -> bool {
    if let Some(embedded) = ip.to_ipv4().or_else(|| nat64_embedded_ipv4(ip)) {
        return is_private_or_special_v4(embedded);
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
}

fn nat64_embedded_ipv4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let segments = ip.segments();
    if segments[..NAT64_WELL_KNOWN_PREFIX.len()] != NAT64_WELL_KNOWN_PREFIX {
        return None;
    }
    let [.., a, b, c, d] = ip.octets();
    Some(Ipv4Addr::new(a, b, c, d))
}

fn retain_public(
    addresses: impl IntoIterator<Item = SocketAddr>,
) -> Result<Vec<SocketAddr>, HostRejection> {
    let mut resolved_any = false;
    let public: Vec<SocketAddr> = addresses
        .into_iter()
        .inspect(|_| resolved_any = true)
        .filter(|addr| !is_private_or_special(addr.ip()))
        .collect();
    match (public.is_empty(), resolved_any) {
        (false, _) => Ok(public),
        (true, true) => Err(HostRejection::PrivateOrSpecial),
        (true, false) => Err(HostRejection::Unresolvable),
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PublicAddressResolver;

impl reqwest::dns::Resolve for PublicAddressResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let resolved = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|_| HostRejection::Unresolvable)?;
            let public = retain_public(resolved)?;
            let addrs: reqwest::dns::Addrs = Box::new(public.into_iter());
            Ok::<_, Box<dyn Error + Send + Sync>>(addrs)
        })
    }
}

pub fn host_rejection_cause(error: &(dyn Error + 'static)) -> Option<HostRejection> {
    let mut current = Some(error);
    while let Some(cause) = current {
        if let Some(rejection) = cause.downcast_ref::<HostRejection>() {
            return Some(*rejection);
        }
        current = cause.source();
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use reqwest::dns::{Name, Resolve};

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn sock(s: &str) -> SocketAddr {
        SocketAddr::new(ip(s), 443)
    }

    #[test]
    fn private_loopback_and_special_addresses_are_blocked() {
        let blocked = [
            "127.0.0.1",
            "127.255.255.254",
            "::1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "fc00::1",
            "fe80::1",
            "224.0.0.1",
            "239.255.255.255",
            "ff02::1",
            "0.0.0.0",
            "0.0.0.1",
            "0.255.255.255",
            "::",
            "198.18.0.1",
            "198.19.255.255",
            "240.0.0.1",
            "255.255.255.254",
            "255.255.255.255",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "64:ff9b::7f00:1",
            "64:ff9b::a00:1",
        ];
        for addr in blocked {
            assert!(
                is_private_or_special(ip(addr)),
                "expected {addr} to be classified private/special"
            );
        }
    }

    #[test]
    fn public_addresses_are_allowed() {
        let allowed = [
            "8.8.8.8",
            "1.1.1.1",
            "192.0.2.1",
            "198.51.100.1",
            "2606:4700::1",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
        ];
        for addr in allowed {
            assert!(
                !is_private_or_special(ip(addr)),
                "expected {addr} to be classified public"
            );
        }
    }

    #[test]
    fn special_block_edges_are_blocked_and_their_neighbours_are_public() {
        let cases = [
            ("1.0.0.0", false),
            ("9.255.255.255", false),
            ("10.0.0.0", true),
            ("10.255.255.255", true),
            ("11.0.0.0", false),
            ("126.255.255.255", false),
            ("127.0.0.0", true),
            ("128.0.0.0", false),
            ("169.253.255.255", false),
            ("169.254.0.0", true),
            ("169.254.255.255", true),
            ("169.255.0.0", false),
            ("192.167.255.255", false),
            ("192.168.0.0", true),
            ("192.168.255.255", true),
            ("192.169.0.0", false),
            ("224.0.0.0", true),
            ("100.63.255.255", false),
            ("100.64.0.0", true),
            ("100.127.255.255", true),
            ("100.128.0.0", false),
            ("172.15.255.255", false),
            ("172.16.0.0", true),
            ("172.31.255.255", true),
            ("172.32.0.0", false),
            ("198.17.255.255", false),
            ("198.18.0.0", true),
            ("198.19.255.255", true),
            ("198.20.0.0", false),
            ("223.255.255.255", false),
            ("239.255.255.255", true),
            ("240.0.0.0", true),
        ];
        for (addr, blocked) in cases {
            assert_eq!(
                is_private_or_special(ip(addr)),
                blocked,
                "classification of {addr}"
            );
        }
    }

    #[test]
    fn ipv6_prefix_edges_are_blocked_and_their_neighbours_are_public() {
        let cases = [
            ("fbff:ffff:ffff:ffff:ffff:ffff:ffff:ffff", false),
            ("fc00::", true),
            ("fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff", true),
            ("fe00::1", false),
            ("fe7f:ffff:ffff:ffff:ffff:ffff:ffff:ffff", false),
            ("fe80::", true),
            ("febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff", true),
            ("feff:ffff:ffff:ffff:ffff:ffff:ffff:ffff", false),
            ("ff00::", true),
            ("::ffff:0.0.0.0", true),
            ("::ffff:100.63.255.255", false),
            ("::ffff:100.64.0.0", true),
            ("64:ff9a::a00:1", false),
            ("64:ff9b::1:a00:1", false),
        ];
        for (addr, blocked) in cases {
            assert_eq!(
                is_private_or_special(ip(addr)),
                blocked,
                "classification of {addr}"
            );
        }
    }

    #[test]
    fn ip_literal_check_screens_literals_and_brackets_but_defers_names() {
        let cases = [
            ("10.0.0.1", Err(HostRejection::PrivateOrSpecial)),
            ("169.254.169.254", Err(HostRejection::PrivateOrSpecial)),
            ("fe80::1", Err(HostRejection::PrivateOrSpecial)),
            ("[::1]", Err(HostRejection::PrivateOrSpecial)),
            ("[::ffff:7f00:1]", Err(HostRejection::PrivateOrSpecial)),
            ("[64:ff9b::a9fe:a9fe]", Err(HostRejection::PrivateOrSpecial)),
            ("localhost", Ok(())),
            ("example.invalid", Ok(())),
            ("192.0.2.1", Ok(())),
            ("2606:4700::1", Ok(())),
            ("[2606:4700::1]", Ok(())),
            ("", Err(HostRejection::Unresolvable)),
        ];
        for (host, expected) in cases {
            assert_eq!(
                ensure_public_ip_literal(host),
                expected,
                "outcome for {host:?}"
            );
        }
    }

    #[test]
    fn loopback_host_check_accepts_only_loopback_names_and_literals() {
        let cases = [
            ("localhost", true),
            ("LocalHost", true),
            ("127.0.0.1", true),
            ("127.1.2.3", true),
            ("[::1]", true),
            ("[::ffff:127.0.0.1]", false),
            ("[::ffff:7f00:1]", false),
            ("::1", true),
            ("localhost.example.com", false),
            ("10.0.0.1", false),
            ("[fe80::1]", false),
            ("", false),
        ];
        for (host, loopback) in cases {
            assert_eq!(
                is_loopback_host(host),
                loopback,
                "loopback check of {host:?}"
            );
        }
    }

    #[test]
    fn retaining_public_from_no_addresses_reports_unresolvable() {
        assert_eq!(
            retain_public(std::iter::empty()),
            Err(HostRejection::Unresolvable)
        );
    }

    #[test]
    fn retaining_public_from_only_private_addresses_reports_private() {
        assert_eq!(
            retain_public([sock("127.0.0.1"), sock("::1"), sock("10.0.0.1")]),
            Err(HostRejection::PrivateOrSpecial)
        );
    }

    #[test]
    fn retaining_public_from_mixed_addresses_keeps_only_public_in_order() {
        assert_eq!(
            retain_public([
                sock("10.0.0.1"),
                sock("192.0.2.1"),
                sock("::ffff:127.0.0.1"),
                sock("2606:4700::1"),
            ]),
            Ok(vec![sock("192.0.2.1"), sock("2606:4700::1")])
        );
    }

    #[tokio::test]
    async fn resolver_rejects_a_name_resolving_only_to_loopback() {
        let name: Name = "localhost".parse().unwrap();
        let Err(error) = PublicAddressResolver.resolve(name).await else {
            panic!("localhost must not resolve to a usable address");
        };
        assert_eq!(
            host_rejection_cause(error.as_ref()),
            Some(HostRejection::PrivateOrSpecial)
        );
    }

    #[tokio::test]
    async fn resolver_passes_through_a_public_address() {
        let name: Name = "192.0.2.1".parse().unwrap();
        let Ok(addrs) = PublicAddressResolver.resolve(name).await else {
            panic!("a public literal must resolve");
        };
        let ips: Vec<IpAddr> = addrs.map(|addr| addr.ip()).collect();
        assert_eq!(ips, vec![ip("192.0.2.1")]);
    }

    #[derive(Debug, thiserror::Error)]
    #[error("connect failed")]
    struct Wrapper(#[source] Box<dyn Error + Send + Sync>);

    #[test]
    fn rejection_cause_is_found_through_nested_sources() {
        let nested = Wrapper(Box::new(Wrapper(Box::new(HostRejection::Unresolvable))));
        assert_eq!(
            host_rejection_cause(&nested),
            Some(HostRejection::Unresolvable)
        );
    }

    #[test]
    fn rejection_cause_is_absent_from_unrelated_error_chains() {
        let unrelated = Wrapper(Box::new(std::io::Error::other("refused")));
        assert_eq!(host_rejection_cause(&unrelated), None);
    }
}
