use std::net::IpAddr;

/// Single SSRF first-layer denylist shared by every egress surface (script HTTP sandbox,
/// sub-action egress client): loopback, RFC-1918/3927/6598/4193 private ranges (incl. the
/// `169.254.169.254` cloud-metadata endpoint), IPv6 link-local/multicast/unspecified, broadcast.
pub fn is_private_or_special(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(ip) => {
            let o = ip.octets();
            o[0] == 10
                || (o[0] == 172 && (o[1] & 0xF0) == 16)
                || (o[0] == 192 && o[1] == 168)
                || (o[0] == 100 && (o[1] & 0xC0) == 64)
                || o[0] == 127
                || (o[0] == 169 && o[1] == 254)
                || (o[0] & 0xF0) == 0xE0
                || o == [255, 255, 255, 255]
                || o == [0, 0, 0, 0]
        }
        IpAddr::V6(ip) => {
            let o = ip.octets();
            ip.is_loopback()
                || ip.is_unspecified()
                || (o[0] & 0xFE) == 0xFC
                || (o[0] == 0xFE && (o[1] & 0xC0) == 0x80)
                || o[0] == 0xFF
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn private_loopback_and_special_addresses_are_blocked() {
        let blocked = [
            "127.0.0.1",
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
            "ff02::1",
            "0.0.0.0",
            "::",
            "255.255.255.255",
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
        ];
        for addr in allowed {
            assert!(
                !is_private_or_special(ip(addr)),
                "expected {addr} to be classified public"
            );
        }
    }

    #[test]
    fn cgnat_boundary_distinguishes_100_64_from_public_100_63() {
        assert!(is_private_or_special(ip("100.64.0.0")));
        assert!(is_private_or_special(ip("100.127.255.255")));
        assert!(!is_private_or_special(ip("100.63.255.255")));
        assert!(!is_private_or_special(ip("100.128.0.0")));
    }

    #[test]
    fn rfc1918_172_boundary_excludes_neighbouring_public_blocks() {
        assert!(is_private_or_special(ip("172.16.0.0")));
        assert!(is_private_or_special(ip("172.31.255.255")));
        assert!(!is_private_or_special(ip("172.15.255.255")));
        assert!(!is_private_or_special(ip("172.32.0.0")));
    }
}
