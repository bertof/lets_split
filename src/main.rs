#![no_main]
#![no_std]
use keyboard_io::{
    hid::keyboard::{KeyboardReport, USB_PRODUCT_ID, USB_VENDOR_ID},
    HIDClass, SerializedDescriptor,
};
use panic_semihosting as _;
use rtic::app;
use stm32f4xx_hal::{
    gpio::{gpioc::PC13, Output, PushPull},
    otg_fs::{UsbBusType, USB},
    prelude::*,
    timer,
};
use usb_device::{class_prelude::*, prelude::*};

#[app(device = stm32f4xx_hal::pac, peripherals = true)]
const APP: () = {
    struct Resources {
        // usb_dev: UsbDevice<'static, UsbBusType>,
        led: PC13<Output<PushPull>>,
        usb_device: UsbDevice<'static, UsbBusType>,
        usb_keyboard_class: HIDClass<'static, UsbBusType>,
    }

    #[init]
    fn init(ctx: init::Context) -> init::LateResources {
        static mut USB_ALLOCATOR: Option<UsbBusAllocator<UsbBusType>> = None;
        static mut EP_MEMORY: [u32; 1024] = [0; 1024];

        let rcc = ctx.device.RCC.constrain();
        let clocks = rcc
            .cfgr
            .use_hse(25.mhz())
            .sysclk(84.mhz())
            .require_pll48clk()
            .freeze();

        let gpioa = ctx.device.GPIOA.split();
        // let gpiob = ctx.device.GPIOB.split();
        let gpioc = ctx.device.GPIOC.split();
        // let gpiod = ctx.device.GPIOD.split();

        // let led = gpiod.pd12.into_push_pull_output();
        let mut led = gpioc.pc13.into_push_pull_output();
        led.set_low();

        let usb = USB {
            usb_global: ctx.device.OTG_FS_GLOBAL,
            usb_device: ctx.device.OTG_FS_DEVICE,
            usb_pwrclk: ctx.device.OTG_FS_PWRCLK,
            pin_dm: gpioa.pa11.into_alternate(),
            pin_dp: gpioa.pa12.into_alternate(),
            hclk: clocks.hclk(),
        };
        *USB_ALLOCATOR = Some(UsbBusType::new(usb, EP_MEMORY));
        let usb_allocator = USB_ALLOCATOR.as_ref().unwrap();

        let usb_keyboard_class = HIDClass::new(usb_allocator, KeyboardReport::desc(), 10);
        let usb_device =
            UsbDeviceBuilder::new(usb_allocator, UsbVidPid(USB_VENDOR_ID, USB_PRODUCT_ID))
                .manufacturer("Bertof")
                .product("lets split")
                .serial_number(env!("CARGO_PKG_VERSION"))
                .build();

        let mut timer = timer::Timer::new(ctx.device.TIM3, &clocks).start_count_down(1.khz());
        timer.listen(timer::Event::TimeOut);

        init::LateResources {
            led,
            usb_device,
            usb_keyboard_class,
        }
    }

    #[task(binds = OTG_FS, priority = 2, resources = [usb_device, usb_keyboard_class])]
    fn usb_tx(ctx: usb_tx::Context) {
        if ctx
            .resources
            .usb_device
            .poll(&mut [ctx.resources.usb_keyboard_class])
        {
            ctx.resources.usb_keyboard_class.poll();
        }
    }

    #[task(binds = OTG_FS_WKUP, priority = 2, resources = [usb_device, usb_keyboard_class])]
    fn usb_rx(ctx: usb_rx::Context) {
        if ctx
            .resources
            .usb_device
            .poll(&mut [ctx.resources.usb_keyboard_class])
        {
            ctx.resources.usb_keyboard_class.poll();
        }
    }
};
