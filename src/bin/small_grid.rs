#![no_main]
#![no_std]

use lets_split as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true, dispatchers = [ TIM4 ])]
mod app {
    use core::convert::Infallible;

    use defmt::info;
    use keyboard_io::{
        buttons::{Button, ButtonAction, ButtonStatusEvent, GridState, LocalGrid},
        codes::KeyboardCode,
        // debouncer::DebouncedPin,
        hid::keyboard::{KeyboardReport, LedStatus},
        prelude::*,
    };
    use stm32f4xx_hal::{
        gpio::{EPin, Input, Output, PushPull},
        interrupt,
        otg_fs::{UsbBusType, USB},
        pac,
        prelude::*,
        timer,
    };
    use usb_device::test_class::{PID, VID};

    type UsbKeyboardClass = HIDClass<'static, UsbBusType>;
    type UsbDevice = keyboard_io::prelude::UsbDevice<'static, UsbBusType>;
    // type DebouncedInputPin = DebouncedPin<EPin<Input<PullUp>>>;
    type InputPin = EPin<Input>;
    type OutputPin = EPin<Output<PushPull>>;

    // Shared resources go here
    #[shared]
    struct Shared {
        status_grid: GridState<KeyboardCode, 2, 2, 0>,
        usb_dev: UsbDevice,
        usb_class: UsbKeyboardClass,
    }

    // Local resources go here
    #[local]
    struct Local {
        local_grid: LocalGrid<InputPin, OutputPin, 2, 2>,
        timer: timer::CounterUs<pac::TIM3>,
    }

    #[init(local = [
      usb_allocator: Option<UsbBusAllocator<UsbBusType>> = None,
      ep_memory: [u32; 1024] = [0; 1024],
    ])]
    fn init(c: init::Context) -> (Shared, Local, init::Monotonics) {
        rtic::pend(interrupt::TIM3);

        let usb_allocator = c.local.usb_allocator;
        let ep_memory = c.local.ep_memory;

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

        info!("Init completed");

        (
            Shared {
                usb_dev,
                usb_class,
                status_grid,
            },
            Local { local_grid, timer },
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

    #[task(binds = TIM3, priority = 1, shared = [ status_grid, usb_class ], local = [ local_grid, timer ])]
    fn local_tick(c: local_tick::Context) {
        c.local.timer.wait().ok();

        for event in c.local.local_grid.get_events() {
            handle_event::spawn(event).ok();
        }

        keyboard_tick::spawn().ok();
    }

    #[task(priority = 3, capacity = 8, shared = [ status_grid ])]
    fn handle_event(mut c: handle_event::Context, event: ButtonStatusEvent) {
        info!("Event: {:?}", event);
        c.shared.status_grid.lock(|status_grid| {
            status_grid.set_pressed(event.inp, event.out, event.pressed);
        })
    }

    #[task(priority = 3, shared = [ usb_class, status_grid ])]
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
