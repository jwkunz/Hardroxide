# hardroxide-net

A small IPv4 network stack written for this project: Ethernet, ARP, ICMP echo, UDP, a DHCP client, TCP and an HTTP/1.1 server. `no_std`, no
allocation, no dependencies. It talks to the network through the four-method `Device` trait, so the same code runs on the Zynq's Ethernet MAC
([`ps7::gem`](ps7.md#ethernet-mac-gem)), in host tests over a virtual wire and, in `scripts/test-qemu.sh`, in QEMU against `curl`.
**It is an educational stack for a trusted network, not a hardened one**; see "What it does not do" below.

## The stack and its sockets

```rust,ignore
pub trait Device
fn mac(&self) -> Mac
fn receive(&mut self, buf: &mut [u8]) -> Option<usize>
fn transmit(&mut self, frame: &[u8]) -> bool
pub struct StaticConfig
pub enum Config
pub struct Stats
pub const UDP_PAYLOAD: usize = 512
pub struct UdpSocket
pub fn bind(&mut self, port: u16)
pub fn close(&mut self)
pub fn recv_from(&mut self, buf: &mut [u8]) -> Option<(Ip4, u16, usize)>
pub struct Stack<D: Device, const T: usize, const U: usize, const BUF: usize>
pub fn new(dev: D, config: Config, now_ms: u64) -> Self
pub fn ip(&self) -> Ip4
pub fn is_configured(&self) -> bool
pub fn dhcp_bound(&self) -> bool
pub fn gateway(&self) -> Option<Ip4>
pub fn netmask(&self) -> Ip4
pub fn stats(&self) -> Stats
pub fn device(&mut self) -> &mut D
pub fn tcp(&mut self, i: usize) -> &mut TcpSocket<BUF>
pub fn udp(&mut self, i: usize) -> &mut UdpSocket
pub fn tcp_connect(&mut self, i: usize, local_port: u16, remote: Ip4, remote_port: u16)
pub fn udp_send(&mut self, now_ms: u64, i: usize, to: Ip4, to_port: u16, data: &[u8]) -> bool
pub fn poll(&mut self, now_ms: u64)
```

`Stack<D, T, U, BUF>` owns the device, `T` TCP sockets with `BUF`-byte buffers each way, `U` UDP sockets and two 1536-byte frame buffers; nothing is
allocated, so a stack of 4 TCP sockets with 2 KiB buffers is about 20 KiB. The application calls `poll(now_ms)` as often as it can (the time is any
millisecond counter) and uses the sockets between polls: `poll` takes the frames the device has, runs the protocols and timers, and sends what the
sockets and protocols have to send. A `Config` is either `Static` or `Dhcp`: DHCP broadcasts DISCOVER and REQUEST (repeating every two seconds) and takes
the address, mask and router from the ACK; if no server answers within `fallback_after_ms` the optional static fallback is used (and DHCP goes on trying,
replacing it when a lease arrives). Leases are not renewed (the lease time is read but ignored).

Hardware addresses are found with ARP (an eight-entry cache, learned from ARP and from the IP packets that arrive from the local network); a packet whose next
hop is not known yet is not sent and an ARP request goes out instead, so the first packet to a new host is lost and the protocols above recover (TCP retransmits;
a first `udp_send` returns `false`). ICMP echo requests to our address are answered. Frames that are not for our address or broadcast are ignored; IP fragments
are dropped (not reassembled).

## UDP

`udp(i).bind(port)` makes socket `i` receive datagrams for `port`; `recv_from` returns the one datagram it holds (up to `UDP_PAYLOAD`, 512 bytes; a second
datagram before the first is read replaces it), and `Stack::udp_send` sends one from the bound port. There is no "port unreachable" reply.

## TCP

```rust,ignore
pub const MSS: u16 = 1460
pub enum State
pub struct SegOut
pub struct TcpSocket<const N: usize>
pub const fn new() -> Self
pub fn listen(&mut self, port: u16)
pub fn connect(&mut self, local_port: u16, remote_ip: Ip4, remote_port: u16, iss: u32)
pub fn state(&self) -> State
pub fn local_port(&self) -> u16
pub fn remote(&self) -> (Ip4, u16)
pub fn was_reset(&self) -> bool
pub fn recv_len(&self) -> usize
pub fn peer_closed(&self) -> bool
pub fn send_space(&self) -> usize
pub fn is_established(&self) -> bool
pub fn is_closed(&self) -> bool
pub fn all_sent_acked(&self) -> bool
pub fn recv(&mut self, buf: &mut [u8]) -> usize
pub fn send(&mut self, data: &[u8]) -> usize
pub fn close(&mut self)
pub fn abort(&mut self)
```

A `TcpSocket` is one connection, or one listener waiting for one: `listen(port)` waits for a connection (and becomes that connection when a SYN arrives; use
several sockets to serve several clients), `Stack::tcp_connect` opens one. The application moves bytes with `send` (returns how many fitted) and `recv`, and calls
`close` to finish: the FIN follows the queued data. **What it does:** the three-way handshake, in-order delivery with cumulative acknowledgement, the peer's window
as the send window, retransmission with a timeout of 400 ms doubling to 4 s (go-back-N; the connection is given up after 8 tries and reported by
`was_reset`), the close handshake with `TIME-WAIT` (two seconds), resets, the maximum segment size option (1460 sent and accepted). **What it does not:**
buffer out-of-order segments (a segment past a gap is dropped and the gap acknowledged again), congestion control, window scaling, selective
acknowledgement, urgent data, delayed acknowledgements and keep-alives. A SYN for a port that is served but has no free socket is ignored (the client retries); a
SYN for a port nobody serves is answered with a reset.

## HTTP

```rust,ignore
pub enum ParseError
pub struct Request<'a>
pub fn parse(buf: &[u8]) -> Result<Request<'_>, ParseError>
pub fn header(&self, name: &str) -> Option<&'a str>
pub fn param(&self, key: &str) -> Option<&'a str>
pub fn percent_decode(s: &str, out: &mut [u8]) -> usize
pub type Generator = fn(arg: u32, offset: usize, out: &mut [u8]) -> usize
pub struct Response<'a>
pub fn text(&mut self, status: u16, content_type: &'static str, args: core::fmt::Arguments)
pub fn bytes(&mut self, status: u16, content_type: &'static str, body: &'static [u8])
pub fn generated(
pub fn empty(&mut self, status: u16)
pub fn scratch(&mut self) -> &mut [u8]
pub fn scratch_body(&mut self, status: u16, content_type: &'static str, len: usize)
pub struct Server<const CONNS: usize, const REQ: usize, const SCRATCH: usize>
pub const fn new(port: u16, first_socket: usize) -> Self
pub fn poll<D: Device, const T: usize, const U: usize, const BUF: usize>(
```

`Server<CONNS, REQ, SCRATCH>::poll(stack, handler)` serves a port on `CONNS` of the stack's TCP sockets (from the index given to `new`): it accepts, reads a request into
a `REQ`-byte buffer, calls `handler(&Request, &mut Response)` when the headers and the `Content-Length` bytes of body have all arrived, sends the response and
closes (one request per connection, `Connection: close`). A malformed request gets 400 and one that does not fit the request buffer gets 413. The handler builds the
response with `text` (formatted into the `SCRATCH`-byte scratch buffer), `bytes` (a fixed `&'static [u8]` of any size), `generated(status, content_type, len, producer, arg)`
(a body of `len` bytes produced in pieces by `producer(arg, offset, out)` as the connection has room: for pictures and other large things), `empty`, or
`scratch` and `scratch_body` to build a body by hand. `Request` has the `method`, `path`, `query`, `body`, `header(name)` and `param(key)` (a query-string or form
value, still percent-encoded; `percent_decode` decodes it). The "routing table" is the handler's `match` on method and path. A connection that reaches `TIME-WAIT` is
taken back for listening at once, so a handful of sockets serves thousands of requests a second.

## DHCP and the wire formats

```rust,ignore
pub const SERVER_PORT: u16 = 67
pub const CLIENT_PORT: u16 = 68
pub const DISCOVER: u8 = 1
pub const OFFER: u8 = 2
pub const REQUEST: u8 = 3
pub const ACK: u8 = 5
pub const NAK: u8 = 6
pub fn build(
pub struct Reply
pub fn parse(p: &[u8], mac: Mac) -> Option<Reply>
```

`dhcp` builds DISCOVER and REQUEST messages and reads OFFER, ACK and NAK. `wire` has the header parsers and writers (`parse_eth`, `parse_arp`, `parse_ipv4`, `parse_udp`,
`parse_tcp`, `emit_*`) and the Internet checksum (`checksum`, `checksum_add`, `checksum_finish`, `pseudo_sum`); a parser returns `None` for anything malformed
(short, bad length, bad checksum, a fragment).

## Testing

The stack is tested against itself and against others. **Host tests** (`cargo test -p hardroxide-net`): two stacks over a virtual wire transfer 200 kB intact, and
30 kB over wires that lose every 11th, 5th and 3rd frame (checks retransmission); ping, ARP, resets for closed ports, silence for busy ports, a DHCP exchange and
its static fallback, the web server (pages, a form, a 404, a 30 kB generated body, 400 and 413), and 30,000 random and corrupted frames fed in with overflow
checks on (it must not panic). **QEMU** (`scripts/test-qemu.sh`): the same code runs on the MAC driver under QEMU's user-mode network and is exercised by real `curl` and
Python sockets: DHCP, a page, a 404, a 300 kB download compared byte for byte, a POST echoed, 40 requests in a row, 6 at once, a UDP echo and a 100 kB TCP echo.

## What it does not do

IPv6; IP options and fragment reassembly; ICMP other than echo replies (no "unreachable", no sending pings); DHCP lease renewal; DNS, so no names; TCP congestion control
and out-of-order buffering (it is a LAN stack: on a clean wire the lack of both does not show, on a lossy one throughput suffers); HTTP keep-alive, chunked requests, `Expect: 100-continue`,
and request pipelining; TLS (that is the next version). Sequence numbers come from a xorshift generator seeded with the MAC and the time: good enough for a trusted
network, **not unpredictable**. It was not audited and not tested against hostile traffic beyond the random-frame test above.
