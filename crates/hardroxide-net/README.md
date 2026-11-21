# hardroxide-net

A small IPv4 network stack written for [Hardroxide](https://github.com/jwkunz/Hardroxide): Ethernet, ARP, ICMP echo, UDP, a DHCP client, TCP
(retransmission, a window, several connections) and an HTTP/1.1 server. `no_std`, no allocation, no dependencies: all buffers are fixed-size and owned by the
stack, and the stack talks to the network through a four-method `Device` trait, so the same code runs on the Zynq's Ethernet MAC, in host tests and in
emulation. The user guide and API reference are at <https://jwkunz.github.io/Hardroxide/>.

This is an educational stack for a trusted network. It is not hardened against hostile traffic.

Licensed under either of Apache-2.0 or MIT at your option.
