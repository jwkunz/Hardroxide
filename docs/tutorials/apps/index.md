# Application tutorials

Programs that do something worth showing, built on the PYNQ **base overlay** (`fixtures/board/pynq_base_overlay.xsa`, see
[PYNQ base overlay](../../reference/base-overlay.md)) with the [bare-metal](../baremetal/index.md) and [RTOS](../rtos/index.md)
flavors. They are planned in [ADR 0004](../../decisions/0004-base-overlay-applications.md): lights and buttons (2.1),
a graphics library on the HDMI output (2.2), audio (2.3), an SD card (2.4), a web server (2.5), HTTPS (2.6).

Setup is the [bare-metal setup](../baremetal/index.md#host-setup): the Vitis tools, the JTAG udev rule and the board in JTAG
boot mode. Do [Bare metal 1](../baremetal/01-jtag.md) first.

| # | Tutorial | You learn |
|---|---|---|
| 1 | [Lights and buttons](01-base-lights.md) | the base overlay on bare metal and under the kernel; hardware interrupts through the interrupt controller; `IrqEvent` |
| 2 | [Pixels on the HDMI output](02-hdmi.md) | the HDMI pipeline, `hardroxide-video` and `hardroxide-gfx`, double buffering |
| 3 | [Sound](03-audio.md) | the ADAU1761 codec, WAV playback, synthesis, recording, a one-cable loopback self-test |
| 4 | [An SD card and files](04-sd.md) | the SD host controller, the block trait, FAT32, a data logger, WAV and BMP files from the card |
| 5 | [Ethernet and a web server](05-network.md) | the Ethernet MAC, our own TCP/IP stack, ping, UDP, a web server on bare metal and beside a 10 ms control loop on the RTOS |
| 6 | [HTTPS with a self-signed certificate](06-https.md) | TLS 1.3 written from scratch, ECDSA P-256 and Ed25519, certificates with `make-cert`, the random-number problem, a browser loading the page, TLS beside a control loop |
