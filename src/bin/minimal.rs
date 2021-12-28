#![no_main]
#![no_std]

use lets_split as _; // global logger + panicking-behavior + memory layout

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true, dispatchers = [ TIM3 ])]
mod app {
    const SYS_TICK: u32 = 84_000_000;

    use dwt_systick_monotonic::{DwtSystick, ExtU32 as _};
    use keyboard_io::{
        hid::keyboard::{KeyboardReport, USB_PRODUCT_ID, USB_VENDOR_ID},
        HIDClass, SerializedDescriptor,
    };
    use stm32f4xx_hal::{
        gpio::{gpioc::PC13, Output, PushPull},
        otg_fs::{UsbBusType, USB},
        prelude::*,
        time::U32Ext as _,
    };
    use usb_device::{class_prelude::*, prelude::*};
    use usbd_serial::SerialPort;

    // TODO: Add a monotonic if scheduling will be used
    #[monotonic(binds = SysTick, default = true)]
    type DwtMono = DwtSystick<SYS_TICK>;

    // Shared resources go here
    #[shared]
    struct Shared {
        usb_device: UsbDevice<'static, UsbBusType>,
        usb_keyboard_class: HIDClass<'static, UsbBusType>,
        usb_serial_class: SerialPort<'static, UsbBusType>,
    }

    // Local resources go here
    #[local]
    struct Local {
        // TODO: Add resources
    }

    #[init(local = [
      USB_ALLOCATOR: Option<UsbBusAllocator<UsbBusType>> = None,
      EP_MEMORY: [u32; 1024] = [0; 1024]
    ])]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        defmt::info!("init");
        let usb_allocator = cx.local.USB_ALLOCATOR;
        let ep_memory = cx.local.EP_MEMORY;

        let rcc = cx.device.RCC.constrain();
        let clocks = rcc
            .cfgr
            .use_hse(25.mhz())
            .sysclk(84.mhz())
            .require_pll48clk()
            .freeze();

        let mut dcb = cx.core.DCB;
        dcb.enable_trace();
        let mono_timer = DwtSystick::new(&mut dcb, cx.core.DWT, cx.core.SYST, clocks.sysclk().0);

        let gpioa = cx.device.GPIOA.split();
        // let gpiob = cx.device.GPIOB.split();
        let gpioc = cx.device.GPIOC.split();
        // let gpiod = cx.device.GPIOD.split();

        // let led = gpiod.pd12.into_push_pull_output();
        let mut led = gpioc.pc13.into_push_pull_output();
        led.set_low();

        let usb = USB {
            usb_global: cx.device.OTG_FS_GLOBAL,
            usb_device: cx.device.OTG_FS_DEVICE,
            usb_pwrclk: cx.device.OTG_FS_PWRCLK,
            pin_dm: gpioa.pa11.into_alternate(),
            pin_dp: gpioa.pa12.into_alternate(),
            hclk: clocks.hclk(),
        };
        *usb_allocator = Some(UsbBusType::new(usb, ep_memory));
        let usb_allocator = usb_allocator.as_ref().unwrap();

        let usb_keyboard_class = HIDClass::new(usb_allocator, KeyboardReport::desc(), 10);
        let usb_serial_class = SerialPort::new(usb_allocator);
        let usb_device =
            UsbDeviceBuilder::new(usb_allocator, UsbVidPid(USB_VENDOR_ID, USB_PRODUCT_ID))
                .manufacturer("Bertof")
                .product("lets split")
                .serial_number(env!("CARGO_PKG_VERSION"))
                .build();

        task1::spawn().unwrap();

        // Setup the monotonic timer
        (
            Shared {
                usb_device,
                usb_keyboard_class,
                usb_serial_class,
            },
            Local {
                // Initialization of local resources go here
            },
            init::Monotonics(mono_timer),
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

    #[task(shared = [ usb_serial_class ])]
    fn task1(mut cx: task1::Context) {
        defmt::info!("Hello from task1!");

        cx.shared.usb_serial_class.lock(|usb_serial_class| {
            match usb_serial_class.write("Hello there!\r\n".as_bytes()) {
                Ok(_) => {}
                Err(UsbError::WouldBlock) => {}
                Err(e) => Err(e).unwrap(),
            }
        });

        task1::spawn_after(1.secs()).unwrap();
    }

    #[task(binds = OTG_FS, priority = 2, shared = [usb_device, usb_keyboard_class, usb_serial_class])]
    fn usb_tx(cx: usb_tx::Context) {
        (
            cx.shared.usb_device,
            cx.shared.usb_keyboard_class,
            cx.shared.usb_serial_class,
        )
            .lock(|usb_device, usb_keyboard_class, usb_serial_class| {
                if usb_device.poll(&mut [usb_keyboard_class, usb_serial_class]) {
                    usb_keyboard_class.poll();
                    usb_serial_class.poll();
                }
            });
    }

    #[task(binds = OTG_FS_WKUP, priority = 2, shared = [usb_device, usb_keyboard_class, usb_serial_class])]
    fn usb_rx(cx: usb_rx::Context) {
        (
            cx.shared.usb_device,
            cx.shared.usb_keyboard_class,
            cx.shared.usb_serial_class,
        )
            .lock(|usb_device, usb_keyboard_class, usb_serial_class| {
                if usb_device.poll(&mut [usb_keyboard_class, usb_serial_class]) {
                    usb_keyboard_class.poll();
                    usb_serial_class.poll();
                }
            });
    }
}
