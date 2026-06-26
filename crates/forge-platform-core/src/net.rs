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
