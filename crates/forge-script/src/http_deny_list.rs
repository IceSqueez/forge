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
    fn imds_endpoint_169_254_169_254_blocked() {
        assert!(is_private_or_special(
            "169.254.169.254".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn cgnat_100_64_x_blocked() {
        assert!(is_private_or_special(
            "100.64.0.1".parse::<IpAddr>().unwrap()
        ));
        assert!(is_private_or_special(
            "100.127.255.255".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn private_192_168_blocked() {
        assert!(is_private_or_special(
            "192.168.1.1".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn loopback_127_x_blocked() {
        assert!(is_private_or_special(
            "127.0.0.1".parse::<IpAddr>().unwrap()
        ));
        assert!(is_private_or_special(
            "127.255.255.255".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn multicast_224_x_blocked() {
        assert!(is_private_or_special(
            "224.0.0.1".parse::<IpAddr>().unwrap()
        ));
        assert!(is_private_or_special(
            "239.255.255.255".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn broadcast_255_255_255_255_blocked() {
        assert!(is_private_or_special(
            "255.255.255.255".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn unspecified_0_0_0_0_blocked() {
        assert!(is_private_or_special("0.0.0.0".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn public_8_8_8_8_allowed() {
        assert!(!is_private_or_special("8.8.8.8".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn public_1_1_1_1_allowed() {
        assert!(!is_private_or_special("1.1.1.1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn ipv6_ula_fc00_blocked() {
        assert!(is_private_or_special("fc00::1".parse::<IpAddr>().unwrap()));
        assert!(is_private_or_special("fd00::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn ipv6_linklocal_fe80_blocked() {
        assert!(is_private_or_special("fe80::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn ipv6_multicast_ff_blocked() {
        assert!(is_private_or_special("ff02::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn ipv6_loopback_blocked() {
        assert!(is_private_or_special("::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn ipv6_unspecified_blocked() {
        assert!(is_private_or_special("::".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn ipv6_public_2606_allowed() {
        assert!(!is_private_or_special(
            "2606:4700::1111".parse::<IpAddr>().unwrap()
        ));
    }
}
