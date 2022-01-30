#![no_main]
#![no_std]

use lets_split as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {
    use core::iter;
    use keyberon::{
        self,
        key_code::{KbHidReport, KeyCode},
    };
    use stm32f4xx_hal::{
        gpio::{self, EPin, Input, PullUp},
        otg_fs::{UsbBusType, USB},
        pac::{self, USART1},
        prelude::*,
        serial, timer,
    };
    use usb_device::class_prelude::*;

    type UsbKeyboardClass = keyberon::Class<'static, UsbBusType, Leds>;
    type UsbDevice = usb_device::device::UsbDevice<'static, UsbBusType>;

    pub struct Leds {
        caps_lock: gpio::gpioc::PC13<gpio::Output<gpio::PushPull>>,
    }

    impl keyberon::keyboard::Leds for Leds {
        fn caps_lock(&mut self, status: bool) {
            if status {
                self.caps_lock.set_low()
            } else {
                self.caps_lock.set_high()
            }
        }
    }

    // Shared resources go here
    #[shared]
    struct Shared {
        usb_dev: UsbDevice,
        usb_class: UsbKeyboardClass,
    }

    // Local resources go here
    #[local]
    struct Local {
        in_pins: [EPin<Input<PullUp>>; 1],
        // out_pins: [EPin<Output<PushPull>>; 1],
        timer: timer::CountDownTimer<pac::TIM3>,

        intra_tx: serial::Tx<USART1>,
        intra_rx: serial::Rx<USART1>,
    }

    #[init(local = [
      usb_allocator: Option<UsbBusAllocator<UsbBusType>> = None,
      ep_memory: [u32; 1024] = [0; 1024]
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
        let leds = Leds { caps_lock: led };

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

        let usb_class = keyberon::new_class(usb_allocator, leds);
        let usb_dev = keyberon::new_device(usb_allocator);

        let mut serial = serial::Serial::new(
            c.device.USART1,
            (gpioa.pa9.into_alternate(), gpioa.pa10.into_alternate()),
            serial::config::Config::default().baudrate(38_400.bps()),
            clocks,
        )
        .unwrap();
        serial.listen(serial::Event::Rxne);
        let (intra_tx, intra_rx) = serial.split();

        let in_pins = [gpioa.pa0.into_pull_up_input().erase()];
        // let out_pins = [gpioa.pa3.into_push_pull_output().erase()];

        (
            Shared { usb_dev, usb_class },
            Local {
                in_pins,
                // out_pins,
                timer,
                intra_tx,
                intra_rx,
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

    fn send_report(iter: impl Iterator<Item = KeyCode>, usb_class: &mut UsbKeyboardClass) {
        let report: KbHidReport = iter.collect();
        if usb_class.device_mut().set_keyboard_report(report.clone()) {
            while let Ok(0) = usb_class.write(report.as_bytes()) {}
        }
    }

    #[task(binds = TIM3, priority = 1, shared = [usb_class], local = [timer, in_pins])]
    fn tick(mut c: tick::Context) {
        c.local.timer.clear_interrupt(timer::Event::TimeOut);
        let pressed = c.local.in_pins[0].is_low();
        c.shared.usb_class.lock(|usb_class| {
            if pressed {
                send_report(iter::once(KeyCode::A), usb_class);
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
