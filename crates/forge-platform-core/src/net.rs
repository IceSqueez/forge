use std::net::IpAddr;

/// Blocks loopback, RFC-1918 private, RFC-3927 link-local (including the
/// `169.254.169.254` cloud-metadata endpoint), RFC-6598 CGNAT, RFC-4193 ULA,
/// IPv6 link-local, multicast, broadcast and unspecified addresses. This is the
/// single SSRF first-layer denylist shared by every egress surface; neither the
/// script HTTP sandbox nor the sub-action egress client keeps its own copy.
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

    // Why: this classifier is the single first-layer SSRF denylist shared by the
    // script HTTP sandbox AND the sub-action egress client. Each row below is
    // hand-derived from the cited RFC, not echoed from the production match arms,
    // so a swapped boolean / wrong mask in `is_private_or_special` flips exactly
    // one assertion. Public rows use RFC 5737 TEST-NET addresses that never route.

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn private_loopback_and_special_addresses_are_blocked() {
        let blocked = [
            // loopback
            "127.0.0.1",
            "::1",
            // RFC-1918 private
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            // RFC-3927 link-local incl. the cloud-metadata endpoint
            "169.254.1.1",
            "169.254.169.254",
            // RFC-6598 CGNAT
            "100.64.0.1",
            // RFC-4193 ULA
            "fc00::1",
            // IPv6 link-local
            "fe80::1",
            // multicast
            "224.0.0.1",
            "ff02::1",
            // unspecified
            "0.0.0.0",
            "::",
            // limited broadcast
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
            // RFC 5737 TEST-NET-1 / TEST-NET-2 - public for classification, never routes
            "192.0.2.1",
            "198.51.100.1",
            // public IPv6
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
        // RFC-6598 is 100.64.0.0/10 → 100.64-100.127. The low edge 100.64 is in
        // range; 100.63 sits just below it and must read as public. This pins the
        // `& 0xC0 == 64` mask against an off-by-one widening to all of 100.0.0.0/8.
        assert!(is_private_or_special(ip("100.64.0.0")));
        assert!(is_private_or_special(ip("100.127.255.255")));
        assert!(!is_private_or_special(ip("100.63.255.255")));
        assert!(!is_private_or_special(ip("100.128.0.0")));
    }

    #[test]
    fn rfc1918_172_boundary_excludes_neighbouring_public_blocks() {
        // 172.16.0.0/12 → 172.16-172.31. Guards the `o[1] & 0xF0 == 16` mask.
        assert!(is_private_or_special(ip("172.16.0.0")));
        assert!(is_private_or_special(ip("172.31.255.255")));
        assert!(!is_private_or_special(ip("172.15.255.255")));
        assert!(!is_private_or_special(ip("172.32.0.0")));
    }
}
