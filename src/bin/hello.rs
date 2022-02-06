#![no_main]
#![no_std]

use lets_split as _; // global logger + panicking-behavior + memory layout

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::println!("Hello, world!");

    lets_split::exit()
}
