//! Address classification for the opt-in `block_private_network` guard.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// True when `ip` is not a public unicast destination: the local host, a
/// private or shared network, or any other special-purpose range. This is the
/// single classifier behind the opt-in SSRF guard, applied to literal hosts,
/// redirect targets and every DNS answer. `docs/nodes/http_request.md` lists
/// the refused ranges; keep the two in step.
pub(super) fn ip_is_internal(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => ipv4_is_internal(v4),
        IpAddr::V6(v6) => ipv6_is_internal(v6),
    }
}

/// IPv4 is public unless it falls in a special-purpose range of the IANA
/// registry. The few globally reachable anycast addresses inside
/// `192.0.0.0/24` are refused with the rest of that block.
fn ipv4_is_internal(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    match a {
        0 | 10 | 127 => true,  // this network, private, loopback
        100 => b & 0xc0 == 64, // 100.64.0.0/10 shared address space
        169 => b == 254,       // link-local, cloud metadata
        172 => b & 0xf0 == 16, // 172.16.0.0/12 private
        // protocol assignments, documentation, 6to4 relay anycast, private
        192 => b == 168 || matches!((b, c), (0, 0) | (0, 2) | (88, 99)),
        198 => b & 0xfe == 18 || (b, c) == (51, 100), // benchmarking, documentation
        203 => (b, c) == (0, 113),                    // documentation
        _ => a >= 224,                                // multicast, reserved, broadcast
    }
}

/// IPv6 is public only inside global unicast `2000::/3`, minus its
/// special-purpose blocks. The forms that carry an IPv4 destination
/// (IPv4-mapped, well-known NAT64 and 6to4) are classified by that embedded
/// address, because that is where the packet ends up. Everything else,
/// including the deprecated IPv4-compatible form, is refused outright.
fn ipv6_is_internal(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return ipv4_is_internal(mapped);
    }
    if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        return ipv4_is_internal(embedded_ipv4(s[6], s[7])); // NAT64 64:ff9b::/96
    }
    if s[0] == 0x2002 {
        return ipv4_is_internal(embedded_ipv4(s[1], s[2])); // 6to4 2002::/16
    }
    if s[0] & 0xe000 != 0x2000 {
        return true; // loopback, ULA, link-local, multicast, reserved space
    }
    (s[0] == 0x2001 && (s[1] & 0xfe00 == 0 || s[1] == 0x0db8)) // 2001::/23, 2001:db8::/32
        || (s[0] == 0x3fff && s[1] & 0xf000 == 0) // 3fff::/20 documentation
}

fn embedded_ipv4(high: u16, low: u16) -> Ipv4Addr {
    Ipv4Addr::from((u32::from(high) << 16) | u32::from(low))
}

#[cfg(test)]
#[path = "address/tests.rs"]
mod tests;
