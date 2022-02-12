#![no_main]
#![no_std]

use lets_split as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true, dispatchers = [ TIM4 ])]
mod app {
    use core::convert::{From, Infallible};
    use defmt::println;
    use keyboard_io::{
        buttons::{Button, ButtonAction, ButtonStatusEvent, GridState, LocalGrid},
        codes::KeyboardCode,
        hid::{
            keyboard::{KeyboardReport, LedStatus},
            PID, VID,
        },
        prelude::{HIDClass, PinState, SerializedDescriptor, UsbDeviceBuilder, UsbVidPid},
    };
    use stm32f4xx_hal::{
        gpio::{EPin, Input, Output, PullUp, PushPull},
        otg_fs::{UsbBusType, USB},
        pac::{self, USART1},
        prelude::*,
        serial, timer,
    };
    use usb_device::class_prelude::*;

    type UsbKeyboardClass = HIDClass<'static, UsbBusType>;
    type UsbDevice = keyboard_io::prelude::UsbDevice<'static, UsbBusType>;
    // type DebouncedInputPin = DebouncedPin<EPin<Input<PullUp>>>;
    type InputPin = EPin<Input<PullUp>>;
    type OutputPin = EPin<Output<PushPull>>;

    // Shared resources go here
    #[shared]
    struct Shared {
        status_grid: GridState<KeyboardCode, 2, 2>,
        usb_dev: UsbDevice,
        usb_class: UsbKeyboardClass,
    }

    // Local resources go here
    #[local]
    struct Local {
        pa0: InputPin,
        led: OutputPin,
        local_grid: LocalGrid<InputPin, OutputPin, 2, 2>,
        timer: timer::CountDownTimer<pac::TIM3>,
        intra_tx: serial::Tx<USART1>,
        intra_rx: serial::Rx<USART1>,
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

        let mut led = gpioc.pc13.into_push_pull_output().erase();
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
        let usb_dev: UsbDevice = UsbDeviceBuilder::new(usb_allocator, UsbVidPid(VID, PID))
            .manufacturer("Bertof - RIIR Task Force")
            .product("Let's Split I")
            .serial_number(env!("CARGO_PKG_VERSION"))
            .build();

        let mut serial = serial::Serial::new(
            c.device.USART1,
            (gpioa.pa9.into_alternate(), gpioa.pa10.into_alternate()),
            serial::config::Config::default().baudrate(38_400.bps()),
            &clocks,
        )
        .unwrap();
        serial.listen(serial::Event::Rxne);
        let (intra_tx, intra_rx) = serial.split();

        let inputs = [
            gpiob.pb1.into_pull_up_input().erase(),
            gpiob.pb0.into_pull_up_input().erase(),
        ];
        let outputs = [
            gpioa.pa7.into_push_pull_output().erase(),
            gpioa.pa6.into_push_pull_output().erase(),
        ];

        let local_grid = LocalGrid::new(inputs, outputs, PinState::Low);

        let status_grid = GridState::new([
            [
                Button::new(ButtonAction::Simple(KeyboardCode::A)),
                Button::new(ButtonAction::Simple(KeyboardCode::B)),
            ],
            [
                Button::new(ButtonAction::Simple(KeyboardCode::C)),
                Button::new(ButtonAction::Simple(KeyboardCode::D)),
            ],
        ]);

        let pa0 = gpioa.pa0.into_pull_up_input().erase();

        println!("Init completed");

        (
            Shared {
                usb_dev,
                usb_class,
                status_grid,
            },
            Local {
                pa0,
                led,
                local_grid,
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
        println!("idle");

        loop {
            continue;
        }
    }

    #[task(binds = TIM3, priority = 4, shared = [status_grid, usb_class], local = [local_grid, timer, pa0, led])]
    fn local_tick(c: local_tick::Context) {
        c.local.timer.wait().ok();

        // for event in c.local.local_grid.get_events() {
        //     handle_event::spawn(event).ok();
        // }

        if c.local.pa0.is_low() {
            c.local.led.set_low();
            send_event::spawn(ButtonStatusEvent::new(true, 0, 0)).ok();
        } else {
            c.local.led.set_high();
        };

        keyboard_tick::spawn().ok();
    }

    #[task(priority = 3, capacity = 8, shared = [status_grid], local = [intra_tx])]
    fn send_event(c: send_event::Context, event: ButtonStatusEvent) {
        println!("Sending event: {:?}", event);
        let buf: [u8; 4] = (&event).into();
        let tx = c.local.intra_tx;
        tx.bwrite_all(&buf).and_then(|_| tx.bflush()).ok();
    }

    #[task(priority = 3, capacity = 8, shared = [status_grid])]
    fn handle_event(mut c: handle_event::Context, event: ButtonStatusEvent) {
        println!("Event: {:?}", event);
        c.shared.status_grid.lock(|status_grid| {
            status_grid.set_pressed(event.inp, event.out, event.pressed);
        })
    }

    #[task(priority = 3, shared = [usb_class, status_grid])]
    fn keyboard_tick(c: keyboard_tick::Context) {
        (c.shared.usb_class, c.shared.status_grid).lock(|usb_class, status_grid| {
            let report: KeyboardReport = status_grid
                .to_report::<KeyboardReport, LedStatus, Infallible>()
                .unwrap();
            while let Ok(0) = usb_class.push_input(&report) {}
        })
    }

    #[task(binds = OTG_FS, priority = 2, shared = [usb_dev, usb_class])]
    fn usb_tx(cx: usb_tx::Context) {
        (cx.shared.usb_dev, cx.shared.usb_class).lock(usb_poll);
    }

    #[task(binds = OTG_FS_WKUP, priority = 2, shared = [usb_dev, usb_class])]
    fn usb_rx(cx: usb_rx::Context) {
        (cx.shared.usb_dev, cx.shared.usb_class).lock(usb_poll);
    }

    fn usb_poll(usb_dev: &mut UsbDevice, keyboard: &mut UsbKeyboardClass) {
        if usb_dev.poll(&mut [keyboard]) {
            keyboard.poll();
        }
    }
}
