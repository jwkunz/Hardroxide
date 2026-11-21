//! A DHCP client's messages (RFC 2131): building DISCOVER and REQUEST, reading OFFER and ACK.

use crate::wire::{Ip4, Mac};

pub const SERVER_PORT: u16 = 67;
pub const CLIENT_PORT: u16 = 68;
const MAGIC: [u8; 4] = [99, 130, 83, 99];
const HEADER: usize = 240;

pub const DISCOVER: u8 = 1;
pub const OFFER: u8 = 2;
pub const REQUEST: u8 = 3;
pub const ACK: u8 = 5;
pub const NAK: u8 = 6;

/// Write a DISCOVER or REQUEST. For a REQUEST pass `(address, server)` to ask for. Returns the length.
pub fn build(
    buf: &mut [u8],
    msg_type: u8,
    xid: u32,
    mac: Mac,
    asking: Option<(Ip4, Ip4)>,
) -> usize {
    buf[..HEADER].fill(0);
    buf[0] = 1; // BOOTREQUEST
    buf[1] = 1; // Ethernet
    buf[2] = 6;
    buf[4..8].copy_from_slice(&xid.to_be_bytes());
    buf[10..12].copy_from_slice(&0x8000u16.to_be_bytes()); // ask for a broadcast reply
    buf[28..34].copy_from_slice(&mac);
    buf[236..240].copy_from_slice(&MAGIC);
    let mut i = HEADER;
    let mut opt = |code: u8, data: &[u8], i: &mut usize| {
        buf[*i] = code;
        buf[*i + 1] = data.len() as u8;
        buf[*i + 2..*i + 2 + data.len()].copy_from_slice(data);
        *i += 2 + data.len();
    };
    opt(53, &[msg_type], &mut i);
    if let Some((addr, server)) = asking {
        opt(50, &addr, &mut i);
        opt(54, &server, &mut i);
    }
    opt(55, &[1, 3, 6, 51], &mut i); // parameter request list: mask, router, DNS, lease time
    buf[i] = 255;
    i + 1
}

/// What a server's OFFER or ACK says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reply {
    pub msg_type: u8,
    pub xid: u32,
    pub yiaddr: Ip4,
    pub mask: Option<Ip4>,
    pub router: Option<Ip4>,
    pub server: Option<Ip4>,
    pub lease_seconds: Option<u32>,
}

/// Read a reply addressed to `mac`.
pub fn parse(p: &[u8], mac: Mac) -> Option<Reply> {
    if p.len() < HEADER || p[0] != 2 || p[236..240] != MAGIC || p[28..34] != mac {
        return None;
    }
    let mut r = Reply {
        msg_type: 0,
        xid: u32::from_be_bytes(p[4..8].try_into().ok()?),
        yiaddr: p[16..20].try_into().ok()?,
        mask: None,
        router: None,
        server: None,
        lease_seconds: None,
    };
    let mut o = &p[HEADER..];
    while let Some(&code) = o.first() {
        match code {
            0 => o = &o[1..],
            255 => break,
            _ => {
                let len = usize::from(*o.get(1)?);
                let data = o.get(2..2 + len)?;
                match (code, len) {
                    (53, 1) => r.msg_type = data[0],
                    (1, 4) => r.mask = data.try_into().ok(),
                    (3, l) if l >= 4 => r.router = data[..4].try_into().ok(),
                    (54, 4) => r.server = data.try_into().ok(),
                    (51, 4) => r.lease_seconds = Some(u32::from_be_bytes(data.try_into().ok()?)),
                    _ => {}
                }
                o = &o[2 + len..];
            }
        }
    }
    (r.msg_type != 0).then_some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_discover_has_the_right_shape_and_an_offer_is_read() {
        let mac = [2, 0x11, 0x22, 0x33, 0x44, 0x55];
        let mut b = [0u8; 400];
        let n = build(&mut b, DISCOVER, 0xDEADBEEF, mac, None);
        assert_eq!(&b[236..240], &MAGIC);
        assert_eq!(&b[4..8], &0xDEADBEEFu32.to_be_bytes());
        assert_eq!(&b[240..243], &[53, 1, DISCOVER]);
        assert_eq!(b[n - 1], 255);

        // An OFFER from a server, built by hand.
        let mut o = [0u8; 300];
        o[0] = 2;
        o[4..8].copy_from_slice(&0xDEADBEEFu32.to_be_bytes());
        o[16..20].copy_from_slice(&[10, 0, 2, 15]);
        o[28..34].copy_from_slice(&mac);
        o[236..240].copy_from_slice(&MAGIC);
        let opts = [
            53, 1, OFFER, 1, 4, 255, 255, 255, 0, 3, 4, 10, 0, 2, 2, 54, 4, 10, 0, 2, 2, 51, 4, 0,
            0, 0x0e, 0x10, 255,
        ];
        o[240..240 + opts.len()].copy_from_slice(&opts);
        let r = parse(&o, mac).unwrap();
        assert_eq!(
            r,
            Reply {
                msg_type: OFFER,
                xid: 0xDEADBEEF,
                yiaddr: [10, 0, 2, 15],
                mask: Some([255, 255, 255, 0]),
                router: Some([10, 0, 2, 2]),
                server: Some([10, 0, 2, 2]),
                lease_seconds: Some(3600)
            }
        );
        // Addressed to another client: ignored.
        assert!(parse(&o, [2, 0, 0, 0, 0, 9]).is_none());
        // A request carries the address and the server.
        let n = build(
            &mut b,
            REQUEST,
            1,
            mac,
            Some(([10, 0, 2, 15], [10, 0, 2, 2])),
        );
        assert!(b[240..n].windows(6).any(|w| w == [50, 4, 10, 0, 2, 15]));
    }
}
