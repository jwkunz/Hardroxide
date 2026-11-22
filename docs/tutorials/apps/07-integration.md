# Applications 7: Everything at once

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Needs:** [Applications 3](03-audio.md), [5](05-network.md) and [6](06-https.md) (the audio, the cable, HTTPS), an HDMI monitor, headphones in **HPH OUT**, a card (optional)

> **Status: run on a PYNQ-Z2 (2026-11-21).** An HTTPS dashboard controls the HDMI graphics, the audio and the LEDs while a 10 ms control loop keeps its deadline, with a live per-task processor monitor. With three HTTPS clients,
> a processor burner, display mode changes and audio start and stop at once, the control loop's worst deviation was **4 microseconds over 5,179 releases**. **Not run:** listening to the audio (the counters say it played, 0 underruns, but this
> tutorial was written without anyone at the headphones), an interactive browser session, and HDMI input capture (section 8). A 17-minute soak is in section 7 (the plan was two hours).

The whole stack of this series in one RTOS program, [`apps/base/rtos/rt_station.rs`](../../../apps/base/rtos/rt_station.rs): the kernel ([RTOS 1](../rtos/01-control-loop.md)), the HDMI pipeline ([Applications 2](02-hdmi.md)), the codec ([Applications 3](03-audio.md)), the SD card
([Applications 4](04-sd.md)), the network and TLS ([Applications 5](05-network.md) and [6](06-https.md)), sharing one 650 MHz core.

## 1. The tasks

| Task | Priority | Does |
|---|---|---|
| `control` | 6 | every 10 ms; records how far each release is from its time |
| *the audio interrupt* | (interrupt) | a global-timer interrupt at 97.656 kHz moves one frame to the codec per sample tick ([the pump](../../reference/audio.md#the-interrupt-driven-pump-pump)) |
| `feed` | 4 | keeps the audio ring full from the built-in chime or a `.wav` from the card, looping |
| `display` | 3 | 1280x720 at about 60 frames per second: a ball over a colour field and a status panel with live numbers |
| `network` | 3 | Ethernet, TCP/IP, TLS 1.3 and the web pages |
| `report` | 2 | the console line every 5 seconds |
| `hog` | 1 | burns the processor when the page says so (to show the others do not suffer) |

`display` and `network` share a priority and so share the processor in turn, one kernel tick each; both are below `feed` and `control`, which sleep between short jobs. Everything above `hog` finishes its work and sleeps, so `hog` gets what is left.

## 2. Run it

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform rtos -o work/rt --hardroxide-path . --name design_1
$ cd work/rt
$ cargo add --dev --path ../../crates/hardroxide-net --path ../../crates/hardroxide-crypto --path ../../crates/hardroxide-tls --path ../../crates/hardroxide-fat \
      --path ../../crates/hardroxide-gfx --path ../../crates/hardroxide-video --path ../../crates/hardroxide-audio
$ sed -i 's/"panic-handler"\]/"panic-handler", "net"]/' Cargo.toml
$ cp ../../apps/base/rtos/rt_station.rs ../../apps/base/data/chime.wav ../../fixtures/tls/test-cert.der ../../fixtures/tls/test-key.bin examples/
$ cargo build --release --example rt_station
$ hardroxide run target/armv7a-none-eabihf/release/examples/rt_station --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 150
```

```console
== rt_station: HTTPS, graphics, audio and a 10 ms control loop ==
display up, pixel clock 74285600 Hz
card: 30040 MiB, 1 sound file(s)
STATION READY: https://192.168.2.135/
control: 5351 releases, worst 3 us | display: 3341 frames, draw worst 15133 us | audio: 2613037 frames, 0 underruns | web: 0 requests | irq 10.5%
```

Open `https://<address>/` (accept the warning: the certificate is the throwaway test one) for the dashboard: LED buttons, audio, display and load controls, the task table and a live picture of the screen. The same things from a shell:

| Request | Does |
|---|---|
| `/api/status` | JSON: the control loop, audio, display, web and SD numbers, the LEDs and switches, and a table of tasks |
| `/led?v=N` | the LEDs (0 to 15) |
| `/audio?cmd=play`, `stop`, `next` | start, stop, or load the next `.wav` from the card (16-bit PCM up to 512 KiB, in the card's root) |
| `/volume?v=N` | the headphone volume, 0 to 62 |
| `/display?mode=N` | 0 a ball over blue, 1 colour bars, 2 a rainbow |
| `/load?on=1` | the processor burner |
| `/screen.bmp` | the screen at a quarter of its size |

## 3. The task monitor

`/api/status` carries, for every task, its share of the processor and how much of its stack it has used. This is measured, with the kernel's new statistics ([RTOS reference](../../reference/rt.md#statistics)), **from a timestamp at every context switch** with the interrupts' time counted on their own line.
With the three clients, the burner and the controls all going (the numbers from the combined test below):

| Task | Priority | Share of the processor | Stack used |
|---|---|---|---|
| idle | 0 | 5.4 % | 12 of 1,024 bytes |
| control | 6 | 0.0 % | 275 of 8,192 |
| feed | 4 | 1.1 % | 479 of 8,192 |
| display | 3 | 1.9 % | 596 of 65,536 |
| **network** | 3 | **76.7 %** | 153,748 of 262,144 |
| hog | 1 | 4.0 % | 72 of 8,192 |
| report | 2 | 0.0 % | 344 of 8,192 |
| *interrupts* | | 10.6 % | |

The audio interrupt (97.7 thousand a second) is the 10.6 %; the network task's 76.7 % is the TLS handshakes of about nine connections a second, about 80 ms of processor each; and the burner, at the lowest priority, gets only what the others leave: 4 %.
(In an earlier run, with the sleep in the polling loop and only a few requests, the network task showed about 22 %; the control loop's worst deviation stayed at 3 to 4 us in every run once the statistics bug in section 5 was fixed.)

## 4. The combined test

A Python script hit the board over HTTPS for 45 seconds with three clients (two asking for `/api/status`, one for `/screen.bmp`) while the burner was on, the display mode was changed six times, the audio was stopped and restarted, and the LEDs were driven:

| | |
|---|---|
| requests served | 203 + 189 status, 21 screen grabs, **0 errors** |
| control loop | worst deviation **4 us** over 5,179 releases (3 us before the test) |
| audio | 2,528,966 frames written, **0 underruns** |
| display | 3,233 frames; the longest single frame took 126 ms (a full repaint of both buffers on a mode change, while the processor was busy) |

## 5. What the integration found

1. **The first processor accounting was wrong.** It counted, at every kernel tick, which task was running. A periodic task released by the tick is not running when the tick samples, so the 10 ms control task showed 0 %, the processor hog 99 %, and the audio interrupt (which is
   short and frequent) was charged to whichever task it interrupted. The monitor now timestamps every context switch and counts interrupt time separately.
2. **Asking for statistics delayed interrupts.** `task_info` scanned each task's stack for the high-water mark inside the kernel's critical section; the network task's stack is 256 KiB, and every status request masked interrupts for about 400 us, which
   showed as a 409 us worst deviation of the control loop. The scan now runs outside the critical section. The control loop's worst deviation did not move again.
3. **The network task used 87 % of the processor** because its polling loop had no sleep; the monitor made it obvious in the first run, and one millisecond of `delay_ms` brought it to 22 % idle.
4. **The status panel on the screen flickered between two versions.** With two frame buffers, repainting the panel into only the buffer that happened to be hidden left the other one stale. Each buffer is now marked and repainted when it is next hidden.

## 6. What it costs

On this 650 MHz core: the audio interrupt is 10.4 % (the price of a codec with no FIFO), the display 1 to 2 % (the video DMA does the scan-out; the processor only draws the ball and the panel), and a TLS handshake about 80 ms of the network task. The display and
network tasks share a priority, so they take turns while a handshake runs (the display's frame times during handshakes alone were not measured; the longest frame in the combined test, 126 ms, was a mode change's full repaint under load); the control loop and the audio never waited.

## 7. The soak

A Python script (`soak.py`, in the history of this work, not kept in the repository) ran against `rt_station` on the board for **17 minutes**: a random mix of status requests, screen grabs, LED, display-mode, audio and burner commands, one request
every 0.2 to 1.5 seconds, and bursts of six concurrent TLS handshakes, logging the status once a minute.

| After 17 minutes | |
|---|---|
| requests | 1,202 served, **0 failed**, 55 bursts of six concurrent handshakes |
| control loop | 105,866 releases, worst deviation **4 us** (4 us in every one of the 17 minutes) |
| audio | 51,692,831 frames written, **0 underruns** |
| display | 66,164 frames |
| processor | idle 39 %, network 10.5 %, burner 36 % (when it was on), display 2.1 %, audio and tick interrupts 10.5 % |
| stack use | every task's high-water mark **identical in the first and the last minute** (the network task 153,748 bytes, the rest under 700), so nothing grew |

**This is shorter than planned**: the plan was two hours, the method of the 2.0 benchmark, and the run was stopped at 17 minutes when the release was due. A leak or a drift that takes longer than that to show would not have been seen; the separate two-hour run of
the bare kernel (726,197 releases of a 10 ms task, none missed, the worst 10.2 us late, [RTOS reference](../../reference/rt.md#measured)) predates the network, the display and the audio interrupt.

## 8. What was not done

**HDMI input capture** (the other half of the base overlay's video path) was left out. It needs a driver for the VDMA's write channel and bring-up of the overlay's HDMI receiver front end, neither of which exists in this project yet, and the board had no video source to
test it with. The audio was never listened to in these tests: the evidence is the pump's counters (every sample tick served, no underruns, a constant -17 ppm clock ratio), and an earlier session heard the polled player. And the dashboard was driven with `curl`, Python and a headless Edge; an interactive session in a browser has not been done.
