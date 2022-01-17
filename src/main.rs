#![no_main]
#![no_std]

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {

    use keyboard_io::{
        hid::keyboard::{KeyboardReport, USB_PRODUCT_ID, USB_VENDOR_ID},
        HIDClass, SerializedDescriptor,
    };
    use panic_semihosting as _;
    use stm32f4xx_hal::{
        otg_fs::{UsbBusType, USB},
        prelude::*,
        timer,
    };
    use usb_device::{class_prelude::*, prelude::*};

    #[shared]
    struct Shared {
        // usb_dev: UsbDevice<'static, UsbBusType>,
        usb_device: UsbDevice<'static, UsbBusType>,
        usb_keyboard_class: HIDClass<'static, UsbBusType>,
    }

    #[local]
    struct Local {}

    #[init(local=[
      usb_allocator: Option<UsbBusAllocator<UsbBusType>> = None,
      ep_memory: [u32; 1024] = [0; 1024]
    ])]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        let usb_allocator: &'static mut Option<UsbBusAllocator<UsbBusType>> =
            ctx.local.usb_allocator;
        let ep_memory: &'static mut [u32; 1024] = ctx.local.ep_memory;

        let rcc = ctx.device.RCC.constrain();
        let clocks = rcc
            .cfgr
            .use_hse(25.mhz())
            .sysclk(84.mhz())
            .require_pll48clk()
            .freeze();

        let gpioa = ctx.device.GPIOA.split();

        let usb = USB {
            usb_global: ctx.device.OTG_FS_GLOBAL,
            usb_device: ctx.device.OTG_FS_DEVICE,
            usb_pwrclk: ctx.device.OTG_FS_PWRCLK,
            pin_dm: gpioa.pa11.into_alternate(),
            pin_dp: gpioa.pa12.into_alternate(),
            hclk: clocks.hclk(),
        };
        *usb_allocator = Some(UsbBusType::new(usb, ep_memory));
        let usb_allocator = usb_allocator.as_ref().unwrap();

        let usb_keyboard_class = HIDClass::new(usb_allocator, KeyboardReport::desc(), 10);
        let usb_device =
            UsbDeviceBuilder::new(usb_allocator, UsbVidPid(USB_VENDOR_ID, USB_PRODUCT_ID))
                .manufacturer("Bertof")
                .product("lets split")
                .serial_number(env!("CARGO_PKG_VERSION"))
                .build();

        let mut timer = timer::Timer::new(ctx.device.TIM3, &clocks).start_count_down(1.khz());
        timer.listen(timer::Event::TimeOut);

        (
            Shared {
                usb_device,
                usb_keyboard_class,
            },
            Local {},
            init::Monotonics(),
        )
    }

    #[task(binds = OTG_FS, priority = 2, shared = [usb_device, usb_keyboard_class])]
    fn usb_tx(ctx: usb_tx::Context) {
        (ctx.shared.usb_device, ctx.shared.usb_keyboard_class).lock(
            |usb_device, usb_keyboard_class| {
                if usb_device.poll(&mut [usb_keyboard_class]) {
                    usb_keyboard_class.poll();
                }
            },
        )
    }

    #[task(binds = OTG_FS_WKUP, priority = 2, shared = [usb_device, usb_keyboard_class])]
    fn usb_rx(ctx: usb_rx::Context) {
        (ctx.shared.usb_device, ctx.shared.usb_keyboard_class).lock(
            |usb_device, usb_keyboard_class| {
                if usb_device.poll(&mut [usb_keyboard_class]) {
                    usb_keyboard_class.poll();
                }
            },
        )
    }
}
