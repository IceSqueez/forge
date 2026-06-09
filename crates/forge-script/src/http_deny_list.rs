use std::net::IpAddr;

/// Covers RFC-1918, RFC-3927, RFC-6598, RFC-4193, RFC-4291 private/special ranges — see RFC-077.
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

    #[test]
    fn private_special_and_imds_addresses_are_blocked() {
        // IPv4 ranges: IMDS, CGNAT, RFC1918, loopback, multicast, broadcast, unspecified.
        // IPv6 ranges: ULA (fc00/fd00), link-local, multicast, loopback, unspecified.
        for addr in [
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "192.168.1.1",
            "127.0.0.1",
            "127.255.255.255",
            "224.0.0.1",
            "239.255.255.255",
            "255.255.255.255",
            "0.0.0.0",
            "fc00::1",
            "fd00::1",
            "fe80::1",
            "ff02::1",
            "::1",
            "::",
        ] {
            assert!(
                is_private_or_special(addr.parse::<IpAddr>().unwrap()),
                "expected blocked: {addr}"
            );
        }
    }

    #[test]
    fn public_ip_addresses_are_allowed() {
        for addr in ["8.8.8.8", "1.1.1.1", "2606:4700::1111"] {
            assert!(
                !is_private_or_special(addr.parse::<IpAddr>().unwrap()),
                "expected allowed: {addr}"
            );
        }
    }
}
