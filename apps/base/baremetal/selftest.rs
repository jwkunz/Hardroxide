//! Base overlay, bare metal: an automated check of everything the base overlay applications use,
//! for the board regression script. Needs no button presses; prints `PASS`/`FAIL` lines and
//! `SELFTEST OK` at the end. Base overlay application 2.

#![no_std]
#![no_main]

use core::time::Duration;
use design_1::{Design, halt, ps7};
use hardroxide_ip::gpio::Channel;
use hardroxide_irq::IrqWaiter;
use hardroxide_mmio::time::{Instant, sleep};
use ps7::println;
use ps7::wait::IrqWait;

ps7::entry!(main);

static mut FAILED: u32 = 0;

fn check(name: &str, ok: bool, detail: &str) {
    if ok {
        println!("PASS {name} {detail}");
    } else {
        println!("FAIL {name} {detail}");
        // SAFETY: single core, no interrupt handler touches it.
        unsafe { FAILED += 1 };
    }
}

fn main() -> ! {
    println!("\n== base overlay self-test ==");
    let d = match Design::init() {
        Ok(d) => d,
        Err(e) => {
            println!("FAIL init {e}");
            halt()
        }
    };
    check("init", true, "(PL configured, MMU and caches on)");
    for &(i, want) in design_1::FCLKS {
        let got = d.fclk_hz(i).unwrap_or(0);
        // The design asks for a frequency the clock divider can only approximate: 1 % is the bar.
        check("fclk", got.abs_diff(want) * 100 <= want, "within 1 % of the design");
        println!("     FCLK{i}: {got} Hz (design {want})");
    }

    // LEDs: write and read back the driven value.
    let leds = &d.leds_gpio;
    leds.set_input_mask(Channel::One, 0).unwrap();
    let mut ok = true;
    for v in [0b0001, 0b0010, 0b0100, 0b1000, 0b1010, 0b0000] {
        leds.write(Channel::One, v).unwrap();
        ok &= leds.read(Channel::One).unwrap() & 0xF == v;
        sleep(Duration::from_millis(50));
    }
    check("leds", ok, "walk and read back");
    let rgb = &d.rgbleds_gpio;
    rgb.set_input_mask(Channel::One, 0).unwrap();
    for v in [0b001001, 0b010010, 0b100100, 0] {
        rgb.write(Channel::One, v).unwrap();
        sleep(Duration::from_millis(50));
    }
    check("rgb leds", rgb.read(Channel::One).unwrap() == 0, "walk");

    // The interrupt path with no device involved: the controller in simulation mode raises an input
    // by software, and it must reach the processor through the GIC like a PL interrupt.
    let intc = &d.system_interrupts;
    let irq = IrqWait::for_line(&design_1::pac::system_interrupts::IRQ_IRQ).unwrap();
    const SOFT: u32 = 1 << 12; // an input nothing in the design drives
    intc.start_simulation().unwrap();
    intc.enable(SOFT).unwrap();
    check("intc enable", intc.enabled().unwrap() & SOFT != 0, "mask reads back");
    d.enable_interrupts();
    irq.enable().unwrap();
    let t0 = Instant::now();
    intc.simulate(SOFT).unwrap();
    let got = matches!(irq.wait_event(Some(Duration::from_millis(100))), Ok(Some(_)));
    let us = t0.elapsed().as_micros();
    check("interrupt to GIC", got, "controller -> GIC -> processor");
    println!("     latency (write to wake-up, polled): {us} us");
    check("intc pending", intc.pending().unwrap() & SOFT != 0, "input pending");
    intc.ack(SOFT).unwrap();
    check("intc ack", intc.pending().unwrap() & SOFT == 0, "cleared");

    // The buttons' GPIO interrupt registers (enabled, nothing pressed: nothing pending).
    let btns = &d.btns_gpio;
    btns.enable_interrupt(Channel::One).unwrap();
    check("buttons idle", !btns.interrupt_pending(Channel::One).unwrap(), "no press, nothing pending (do not touch the buttons)");
    btns.disable_interrupt(Channel::One).unwrap();

    // SAFETY: nothing else runs.
    let failed = unsafe { FAILED };
    if failed == 0 {
        println!("SELFTEST OK");
    } else {
        println!("SELFTEST FAILED: {failed} check(s)");
    }
    halt()
}
