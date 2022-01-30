#![no_main]
#![no_std]

use lets_split as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {
    use core::convert::Infallible;
    use keyboard_io::{
        buttons::{Button, Grid, ReportGenerator},
        codes::KeyboardCode,
        debouncer::DebouncedPin,
        hid::{keyboard::KeyboardReport, PID, VID},
        prelude::*,
    };
    use stm32f4xx_hal::{
        gpio::{EPin, Input, Output, PullUp, PushPull},
        otg_fs::{UsbBusType, USB},
        pac,
        prelude::*,
        timer,
    };

    type UsbKeyboardClass = HIDClass<'static, UsbBusType>;
    type UsbDevice = keyboard_io::prelude::UsbDevice<'static, UsbBusType>;
    type DebouncedInputPin = DebouncedPin<EPin<Input<PullUp>>>;
    type InputPin = EPin<Input<PullUp>>;
    type OutputPin = EPin<Output<PushPull>>;

    // Shared resources go here
    #[shared]
    struct Shared {
        usb_dev: UsbDevice,
        usb_class: UsbKeyboardClass,
    }

    // Local resources go here
    #[local]
    struct Local {
        grid: Grid<Infallible, KeyboardCode, InputPin, OutputPin, 2, 2>,
        timer: timer::CountDownTimer<pac::TIM3>,
    }

    #[init(local = [
      usb_allocator: Option<UsbBusAllocator<UsbBusType>> = None,
      ep_memory: [u32; 1024] = [0; 1024],
    ])]
    fn init(c: init::Context) -> (Shared, Local, init::Monotonics) {
        let usb_allocator = c.local.usb_allocator;
        let ep_memory = c.local.ep_memory;

        let rcc = c.device.RCC.constrain();
        let clocks = rcc
            .cfgr
            .use_hse(25.mhz())
            .sysclk(84.mhz())
            .require_pll48clk()
            .freeze();

        let mut timer = timer::Timer::new(c.device.TIM3, &clocks).start_count_down(1.khz());
        timer.listen(timer::Event::TimeOut);

        let gpioa = c.device.GPIOA.split();
        let gpiob = c.device.GPIOB.split();
        let gpioc = c.device.GPIOC.split();

        let mut led = gpioc.pc13.into_push_pull_output();
        led.set_low();

        let usb = USB {
            usb_global: c.device.OTG_FS_GLOBAL,
            usb_device: c.device.OTG_FS_DEVICE,
            usb_pwrclk: c.device.OTG_FS_PWRCLK,
            pin_dm: gpioa.pa11.into_alternate(),
            pin_dp: gpioa.pa12.into_alternate(),
            hclk: clocks.hclk(),
        };
        *usb_allocator = Some(UsbBusType::new(usb, ep_memory));
        let usb_allocator = usb_allocator.as_ref().unwrap();

        let usb_class = HIDClass::new(usb_allocator, KeyboardReport::desc(), 10);
        let usb_dev = UsbDeviceBuilder::new(usb_allocator, UsbVidPid(VID, PID))
            .manufacturer("Bertof - RIIR Task Force")
            .product("Let's Split I")
            .serial_number(env!("CARGO_PKG_VERSION"))
            .build();

        let inputs = [
            gpiob.pb1.into_pull_up_input().erase(),
            gpiob.pb0.into_pull_up_input().erase(),
        ];
        let outputs = [
            gpioa.pa7.into_push_pull_output().erase(),
            gpioa.pa6.into_push_pull_output().erase(),
        ];

        let grid = Grid {
            inputs,
            outputs,
            buttons: [
                [
                    Button::Simple(KeyboardCode::A),
                    Button::Simple(KeyboardCode::B),
                ],
                [
                    Button::Simple(KeyboardCode::C),
                    Button::Simple(KeyboardCode::D),
                ],
            ],
        };

        (
            Shared { usb_dev, usb_class },
            Local { grid, timer },
            init::Monotonics(),
        )
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

    #[task(binds = TIM3, priority = 1, shared = [usb_class], local = [timer, grid])]
    fn tick(mut c: tick::Context) {
        c.local.timer.clear_interrupt(timer::Event::TimeOut);
        // let pressed = c.local.in_pins[0].is_low().unwrap();
        let report = ReportGenerator::keyboard_report(c.local.grid).unwrap();
        c.shared.usb_class.lock(
            |usb_class| {
                while let Ok(0) = usb_class.push_input(&report) {}
            },
        );
    }

    fn usb_poll(usb_dev: &mut UsbDevice, keyboard: &mut UsbKeyboardClass) {
        if usb_dev.poll(&mut [keyboard]) {
            keyboard.poll();
        }
    }

    #[task(binds = OTG_FS, priority = 2, shared = [usb_dev, usb_class])]
    fn usb_tx(cx: usb_tx::Context) {
        (cx.shared.usb_dev, cx.shared.usb_class).lock(usb_poll);
    }

    #[task(binds = OTG_FS_WKUP, priority = 2, shared = [usb_dev, usb_class])]
    fn usb_rx(cx: usb_rx::Context) {
        (cx.shared.usb_dev, cx.shared.usb_class).lock(usb_poll);
    }
}
