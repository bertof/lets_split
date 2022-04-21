#![no_main]
#![no_std]

use lets_split as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {
    use core::iter;
    use keyboard_io::{
        codes::KeyboardCode, debouncer::DebouncedPin, hid::keyboard::KeyboardReport, prelude::*,
    };
    use stm32f4xx_hal::{
        gpio::{EPin, Input, PullUp},
        otg_fs::{UsbBusType, USB},
        pac,
        prelude::*,
        timer,
    };
    use usb_device::test_class::{PID, VID};

    type UsbKeyboardClass = HIDClass<'static, UsbBusType>;
    type UsbDevice = keyboard_io::prelude::UsbDevice<'static, UsbBusType>;

    // Shared resources go here
    #[shared]
    struct Shared {
        usb_dev: UsbDevice,
        usb_class: UsbKeyboardClass,
    }

    // Local resources go here
    #[local]
    struct Local {
        in_pins: [DebouncedPin<EPin<Input<PullUp>>>; 1],
        // out_pins: [EPin<Output<PushPull>>; 1],
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
        // let gpiob = c.device.GPIOB.split();
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

        let in_pins = [DebouncedPin::new(gpioa.pa0.into_pull_up_input().erase(), 2)];
        // let out_pins = [gpioa.pa3.into_push_pull_output().erase()];

        (
            Shared { usb_dev, usb_class },
            Local {
                in_pins,
                // out_pins,
                timer,
            },
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

    fn send_report(iter: impl Iterator<Item = KeyboardCode>, usb_class: &mut UsbKeyboardClass) {
        let mut report = KeyboardReport {
            modifier: Default::default(),
            reserved: Default::default(),
            leds: Default::default(),
            keycodes: Default::default(),
        };
        for key in iter {
            if let Some(v) = report.keycodes.iter_mut().find(|v| **v == 0x00) {
                *v = key as u8;
            }
        }
        while let Ok(0) = usb_class.push_input(&report) {}
    }

    #[task(binds = TIM3, priority = 1, shared = [usb_class], local = [timer, in_pins])]
    fn tick(mut c: tick::Context) {
        c.local.timer.clear_interrupt(timer::Event::TimeOut);
        let pressed = c.local.in_pins[0].is_low().unwrap();
        c.shared.usb_class.lock(|usb_class| {
            if pressed {
                send_report(iter::once(KeyboardCode::B), usb_class);
            } else {
                send_report(iter::empty(), usb_class);
            };
        });
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
