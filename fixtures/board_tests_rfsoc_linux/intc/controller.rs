//! Two interrupt sources behind the AXI interrupt controller, whose one output is `pl_ps_irq0[0]`: the timer, and a
//! GPIO whose inputs the second GPIO drives.
use std::time::Duration;
use design_1::{Design, pac};
use hardroxide_ip::gpio::Channel;
use hardroxide_mmio::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let d = Design::load()?;
    let timer_input = match pac::axi_timer_0::IRQ_INTERRUPT.via_intc {
        Some((_, bit)) => 1 << bit,
        None => panic!("the timer is not behind the controller"),
    };
    let gpio_input = match pac::axi_gpio_0::IRQ_IP2INTC_IRPT.via_intc {
        Some((_, bit)) => 1 << bit,
        None => panic!("the GPIO is not behind the controller"),
    };
    let irq = pac::axi_intc_0::IRQ_IRQ.open().expect("the controller's output");
    let (controller, timer, input, output) =
        (&d.axi_intc_0, &d.axi_timer_0, &d.axi_gpio_0, &d.axi_gpio_1);
    controller.init().unwrap();
    controller.enable(timer_input | gpio_input).unwrap();
    output.set_input_mask(Channel::One, 0).unwrap();
    output.write(Channel::One, 0).unwrap();
    input.clear_interrupt(Channel::One).unwrap();
    input.enable_interrupt(Channel::One).unwrap();
    controller.ack(timer_input | gpio_input).unwrap();

    for round in 1..=5u32 {
        let t0 = Instant::now();
        timer
            .start(timer.ticks_for_ns(10_000_000) as u32, false, true)
            .unwrap();
        let pending = controller
            .wait(
                &irq,
                timer_input | gpio_input,
                Some(Duration::from_millis(100)),
            )
            .unwrap();
        assert_eq!(pending, Some(timer_input), "round {round}: the timer");
        let took = t0.elapsed().as_micros();
        assert!((9_990..10_100).contains(&took), "timer took {took} us");
        timer.clear().unwrap();
        controller.ack(timer_input).unwrap();

        output.write(Channel::One, round).unwrap();
        let pending = controller
            .wait(
                &irq,
                timer_input | gpio_input,
                Some(Duration::from_millis(100)),
            )
            .unwrap();
        assert_eq!(pending, Some(gpio_input), "round {round}: the GPIO");
        assert_eq!(input.read(Channel::One).unwrap(), round);
        input.clear_interrupt(Channel::One).unwrap();
        controller.ack(gpio_input).unwrap();
    }
    println!("timer and GPIO through the controller: 5 rounds of each");
    println!("INTC OK");
    Ok(())
}
