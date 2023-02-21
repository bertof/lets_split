#![no_main]
#![no_std]

use lets_split as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {
    use defmt::info;
    use keyboard_io::{buttons::LocalGrid, prelude::PinState};
    use stm32f4xx_hal::{
        gpio::{EPin, Input, Output, PushPull},
        interrupt, pac,
        prelude::*,
        timer,
    };

    // Shared resources go here
    #[shared]
    struct Shared {}

    // Local resources go here
    #[local]
    struct Local {
        local_grid: LocalGrid<EPin<Input>, EPin<Output<PushPull>>, 2, 3>,
        timer: timer::CounterUs<pac::TIM3>,
    }

    #[init()]
    fn init(c: init::Context) -> (Shared, Local, init::Monotonics) {
        rtic::pend(interrupt::TIM3);

        let rcc = c.device.RCC.constrain();
        let clocks = rcc
            .cfgr
            .use_hse(25.MHz())
            .sysclk(84.MHz())
            .require_pll48clk()
            .freeze();

        let mut timer = c.device.TIM3.counter_us(&clocks);
        timer.start(100.micros()).unwrap();
        timer.listen(timer::Event::Update);

        let gpioa = c.device.GPIOA.split();
        let gpiob = c.device.GPIOB.split();
        let gpioc = c.device.GPIOC.split();

        let mut led = gpioc.pc13.into_push_pull_output();
        led.set_low();

        let inputs = [
            gpiob.pb1.into_pull_up_input().erase(),
            gpiob.pb0.into_pull_up_input().erase(),
        ];
        let outputs = [
            gpioa.pa6.into_push_pull_output().erase(),
            gpioa.pa7.into_push_pull_output().erase(),
            gpioa.pa5.into_push_pull_output().erase(),
        ];

        let local_grid = LocalGrid::new(inputs, outputs, PinState::Low);

        (Shared {}, Local { local_grid, timer }, init::Monotonics())
    }

    // Optional idle, can be removed if not needed.
    // Note that removing this will put the MCU to sleep when no task is running, and this
    // generally breaks RTT based printing.
    #[idle]
    fn idle(_: idle::Context) -> ! {
        defmt::info!("idle");

        loop {
            continue;
        }
    }

    #[task(binds = TIM3, priority = 1, local = [timer, local_grid])]
    fn tick(c: tick::Context) {
        c.local.timer.wait().ok();

        for e in c.local.local_grid.get_events() {
            info!("Event: {:?}", e);
        }
    }
}
