#![no_main]
#![no_std]

use lets_split as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true, dispatchers = [ TIM4 ])]
mod app {
    use core::convert::{From, Infallible, TryFrom};
    use defmt::println;
    use keyboard_io::{
        buttons::{
            shortcuts::{bs, row},
            ButtonAction, ButtonStatusEvent, GridState, LocalGrid, StatefulInputPin,
        },
        codes::{KeyboardCode, MediaKey},
        hid::keyboard::{KeyboardReport, LedStatus},
        prelude::{HIDClass, PinState, SerializedDescriptor, UsbDeviceBuilder, UsbVidPid},
    };
    use stm32f4xx_hal::{
        gpio::{EPin, Input, Output, PushPull},
        interrupt,
        otg_fs::{UsbBusType, USB},
        pac::{self, USART1},
        prelude::*,
        serial, timer,
    };
    use usb_device::{
        class_prelude::*,
        test_class::{PID, VID},
    };

    type UsbKeyboardClass = HIDClass<'static, UsbBusType>;
    type UsbDevice = keyboard_io::prelude::UsbDevice<'static, UsbBusType>;
    // type DebouncedInputPin = DebouncedPin<EPin<Input<PullUp>>>;
    type InputPin = EPin<Input>;
    type OutputPin = EPin<Output<PushPull>>;

    #[derive(Debug, Clone, Copy)]
    pub enum KbEvent {
        K(KeyboardCode),
        M(MediaKey),
    }

    impl TryFrom<KbEvent> for KeyboardCode {
        type Error = ();

        fn try_from(value: KbEvent) -> Result<Self, Self::Error> {
            match value {
                KbEvent::K(k) => Ok(k),
                KbEvent::M(_) => Err(()),
            }
        }
    }

    impl TryFrom<KbEvent> for MediaKey {
        type Error = ();

        fn try_from(value: KbEvent) -> Result<Self, Self::Error> {
            match value {
                KbEvent::K(_) => Err(()),
                KbEvent::M(m) => Ok(m),
            }
        }
    }

    // Shared resources go here
    #[shared]
    struct Shared {
        status_grid: GridState<KbEvent, 4, 12, 3>,
        usb_dev: UsbDevice,
        usb_class: UsbKeyboardClass,
    }

    // Local resources go here
    #[local]
    struct Local {
        button: StatefulInputPin<InputPin>,
        intra_rx: serial::Rx<USART1>,
        intra_tx: serial::Tx<USART1>,
        is_left_side: bool,
        led: OutputPin,
        local_grid: LocalGrid<InputPin, OutputPin, 6, 4>,
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

        let serial = serial::Serial::new(
            c.device.USART1,
            (gpioa.pa9.into_alternate(), gpioa.pa10.into_alternate()),
            serial::config::Config::default().baudrate(38_400.bps()),
            &clocks,
        )
        .unwrap();
        let (intra_tx, mut intra_rx) = serial.split();
        intra_rx.listen();

        let button = StatefulInputPin::new(gpioa.pa0.into_pull_up_input().erase());

        // Jumper to ground on right side of the keyboard
        let side_jumper_input = gpioa.pa1.into_pull_up_input().erase();
        let mut side_jumper_output = gpioa.pa15.into_push_pull_output().erase();
        side_jumper_output.set_low();
        let is_left_side = side_jumper_input.is_high();
        side_jumper_output.set_high();

        println!("Left side: {}", is_left_side);

        let (inputs, outputs) = if is_left_side {
            (
                [
                    gpioa.pa4.into_pull_up_input().erase(),
                    gpioa.pa3.into_pull_up_input().erase(),
                    gpioa.pa2.into_pull_up_input().erase(),
                    gpiob.pb9.into_pull_up_input().erase(),
                    gpiob.pb8.into_pull_up_input().erase(),
                    gpiob.pb7.into_pull_up_input().erase(),
                ],
                [
                    gpiob.pb3.into_push_pull_output().erase(),
                    gpiob.pb4.into_push_pull_output().erase(),
                    gpiob.pb5.into_push_pull_output().erase(),
                    gpiob.pb6.into_push_pull_output().erase(),
                ],
            )
        } else {
            (
                [
                    gpioa.pa6.into_pull_up_input().erase(),
                    gpioa.pa5.into_pull_up_input().erase(),
                    gpioa.pa4.into_pull_up_input().erase(),
                    gpiob.pb5.into_pull_up_input().erase(),
                    gpiob.pb4.into_pull_up_input().erase(),
                    gpiob.pb3.into_pull_up_input().erase(),
                ],
                [
                    gpiob.pb2.into_push_pull_output().erase(),
                    gpiob.pb1.into_push_pull_output().erase(),
                    gpiob.pb0.into_push_pull_output().erase(),
                    gpioa.pa7.into_push_pull_output().erase(),
                ],
            )
        };

        let local_grid = LocalGrid::new(inputs, outputs, PinState::Low);

        let status_grid = {
            GridState::new([
                row([
                    bs(KbEvent::K(KeyboardCode::Escape))
                        .add_layer(KbEvent::K(KeyboardCode::Grave), 0),
                    bs(KbEvent::K(KeyboardCode::Q)).add_layer(KbEvent::K(KeyboardCode::Kb1), 0),
                    bs(KbEvent::K(KeyboardCode::W)).add_layer(KbEvent::K(KeyboardCode::Kb2), 0),
                    bs(KbEvent::K(KeyboardCode::E)).add_layer(KbEvent::K(KeyboardCode::Kb3), 0),
                    bs(KbEvent::K(KeyboardCode::R)).add_layer(KbEvent::K(KeyboardCode::Kb4), 0),
                    bs(KbEvent::K(KeyboardCode::T)).add_layer(KbEvent::K(KeyboardCode::Kb5), 0),
                    bs(KbEvent::K(KeyboardCode::Y)).add_layer(KbEvent::K(KeyboardCode::Kb6), 0),
                    bs(KbEvent::K(KeyboardCode::U)).add_layer(KbEvent::K(KeyboardCode::Kb7), 0),
                    bs(KbEvent::K(KeyboardCode::I)).add_layer(KbEvent::K(KeyboardCode::Kb8), 0),
                    bs(KbEvent::K(KeyboardCode::O))
                        .add_layer(KbEvent::K(KeyboardCode::Kb9), 0)
                        .add_layer(KbEvent::K(KeyboardCode::Minus), 1),
                    bs(KbEvent::K(KeyboardCode::P))
                        .add_layer(KbEvent::K(KeyboardCode::Kb0), 0)
                        .add_layer(KbEvent::K(KeyboardCode::Equal), 1),
                    bs(KbEvent::K(KeyboardCode::BSpace))
                        .add_layer(KbEvent::K(KeyboardCode::Delete), 1)
                        .add_layer(KbEvent::K(KeyboardCode::PScreen), 2),
                ]),
                row([
                    bs(KbEvent::K(KeyboardCode::Tab)).add_layer(KbEvent::K(KeyboardCode::F1), 1),
                    bs(KbEvent::K(KeyboardCode::A)).add_layer(KbEvent::K(KeyboardCode::F2), 1),
                    bs(KbEvent::K(KeyboardCode::S)).add_layer(KbEvent::K(KeyboardCode::F3), 1),
                    bs(KbEvent::K(KeyboardCode::D)).add_layer(KbEvent::K(KeyboardCode::F4), 1),
                    bs(KbEvent::K(KeyboardCode::F)).add_layer(KbEvent::K(KeyboardCode::F5), 1),
                    bs(KbEvent::K(KeyboardCode::G)).add_layer(KbEvent::K(KeyboardCode::F6), 1),
                    bs(KbEvent::K(KeyboardCode::H)).add_layer(KbEvent::K(KeyboardCode::F7), 1),
                    bs(KbEvent::K(KeyboardCode::J)).add_layer(KbEvent::K(KeyboardCode::F8), 1),
                    bs(KbEvent::K(KeyboardCode::K))
                        .add_layer(KbEvent::K(KeyboardCode::LBracket), 0)
                        .add_layer(KbEvent::K(KeyboardCode::F9), 1),
                    bs(KbEvent::K(KeyboardCode::L))
                        .add_layer(KbEvent::K(KeyboardCode::RBracket), 0)
                        .add_layer(KbEvent::K(KeyboardCode::F10), 1),
                    bs(KbEvent::K(KeyboardCode::SColon))
                        .add_layer(KbEvent::K(KeyboardCode::BSlash), 0)
                        .add_layer(KbEvent::K(KeyboardCode::F11), 1),
                    bs(KbEvent::K(KeyboardCode::Quote))
                        .add_layer(KbEvent::K(KeyboardCode::NonUsBSlash), 0)
                        .add_layer(KbEvent::K(KeyboardCode::F12), 1),
                ]),
                row([
                    bs(KbEvent::K(KeyboardCode::LShift)),
                    bs(KbEvent::K(KeyboardCode::Z)),
                    bs(KbEvent::K(KeyboardCode::X)),
                    bs(KbEvent::K(KeyboardCode::C)),
                    bs(KbEvent::K(KeyboardCode::V)),
                    bs(KbEvent::K(KeyboardCode::B)),
                    bs(KbEvent::K(KeyboardCode::N)),
                    bs(KbEvent::K(KeyboardCode::M)),
                    bs(KbEvent::K(KeyboardCode::Comma))
                        .add_layer(KbEvent::K(KeyboardCode::Menu), 1),
                    bs(KbEvent::K(KeyboardCode::Dot)).add_layer(KbEvent::K(KeyboardCode::RCtrl), 1),
                    bs(KbEvent::K(KeyboardCode::Slash))
                        .add_layer(KbEvent::K(KeyboardCode::Insert), 1),
                    bs(KbEvent::K(KeyboardCode::Enter)),
                ]),
                row([
                    bs(KbEvent::K(KeyboardCode::LCtrl)),
                    ButtonAction::MomentaryLayer(2),
                    bs(KbEvent::K(KeyboardCode::LAlt)),
                    bs(KbEvent::K(KeyboardCode::LGui)),
                    ButtonAction::MomentaryLayer(0),
                    bs(KbEvent::K(KeyboardCode::Space)),
                    bs(KbEvent::K(KeyboardCode::Space)),
                    ButtonAction::MomentaryLayer(1),
                    bs(KbEvent::K(KeyboardCode::Left)).add_layer(KbEvent::K(KeyboardCode::Home), 1),
                    bs(KbEvent::K(KeyboardCode::Down))
                        .add_layer(KbEvent::K(KeyboardCode::VolDown), 0)
                        .add_layer(KbEvent::K(KeyboardCode::PgDown), 1),
                    bs(KbEvent::K(KeyboardCode::Up))
                        .add_layer(KbEvent::K(KeyboardCode::VolUp), 0)
                        .add_layer(KbEvent::K(KeyboardCode::PgUp), 1),
                    bs(KbEvent::K(KeyboardCode::Right))
                        .add_layer(KbEvent::K(KeyboardCode::Mute), 0)
                        .add_layer(KbEvent::K(KeyboardCode::End), 1),
                ]),
            ])
        };

        println!("Init completed");
        led.set_high();

        (
            Shared {
                usb_dev,
                usb_class,
                status_grid,
            },
            Local {
                button,
                intra_rx,
                intra_tx,
                is_left_side,
                led,
                local_grid,
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
        println!("idle");

        loop {
            continue;
        }
    }

    #[task(binds = TIM3, priority = 4, local = [local_grid, timer, button, led, is_left_side])]
    fn local_tick(c: local_tick::Context) {
        c.local.timer.wait().ok();

        for mut event in c.local.local_grid.get_events() {
            // Handle shift in coordinates
            if !*c.local.is_left_side {
                event.inp += 6;
            };

            handle_event::spawn(event.clone()).ok();
            send_event::spawn(event).ok();
        }

        keyboard_tick::spawn().ok();
    }

    #[task(binds = USART1, priority = 5, local = [intra_rx, buf: [u8;4] = [0;4]])]
    fn rx(c: rx::Context) {
        let buf = c.local.buf;

        if let Ok(b) = c.local.intra_rx.read() {
            buf.rotate_left(1);
            buf[3] = b;
            // println!("Serial buf: {:?}", buf);

            if let Ok(event) = ButtonStatusEvent::try_from(&*buf) {
                handle_event::spawn(event).ok();
            }
        }
    }

    #[task(priority = 3, capacity = 8, local = [intra_tx])]
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
            status_grid.set_pressed(event.out, event.inp, event.pressed);
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
