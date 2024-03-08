#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use microbit_bsp::{
    embassy_nrf::{
        bind_interrupts,
        gpio::{AnyPin, Level, Output, OutputDrive},
        saadc,
    },
    Microbit,
};
use panic_probe as _;

type Adc = saadc::Saadc<'static, 1>;

struct Knob(Adc);

impl Knob {
    async fn new(adc: Adc) -> Self {
        adc.calibrate().await;
        Self(adc)
    }

    async fn _measure(&mut self) -> f32 {
        let mut buf = [0];
        self.0.sample(&mut buf).await;
        buf[0].clamp(0, 0x7fff) as f32 / 0x7fff as f32
    }
}

type RgbPins = [Output<'static, AnyPin>; 3];

struct Rgb<const LEVELS: u32> {
    rgb: RgbPins,
    levels: [u32; 3],
    tick: u32,
    tick_time: Duration,
}

impl<const LEVELS: u32> Rgb<LEVELS> {
    fn frame_tick_time(frame_rate: u64) -> Duration {
        Duration::from_micros(1_000_000 / (3 * frame_rate * LEVELS as u64))
    }

    fn new(rgb: RgbPins, frame_rate: u64) -> Self {
        let tick_time = Self::frame_tick_time(frame_rate);
        Self { rgb, levels: [0, 0, 0], tick: 0, tick_time }
    }

    async fn step(&mut self) {
        let led = self.tick / LEVELS;
        let level = self.tick % LEVELS;
        if level == 0 {
            let prev = (led + 2) % 3;
            if self.rgb[prev as usize].is_set_high() {
                self.rgb[prev as usize].set_low();
            }
        }
        if level < self.levels[led as usize] {
            self.rgb[led as usize].set_high();
        } else if self.rgb[led as usize].is_set_high() {
            self.rgb[led as usize].set_low();
        }
        self.tick = (self.tick + 1) % (3 * LEVELS);
        Timer::after(self.tick_time).await;
    }

    fn set_rgb<F>(&mut self, setter: F)
        where F: FnOnce(&mut [u32; 3])
    {
        setter(&mut self.levels);
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let board = Microbit::default();

    bind_interrupts!(struct Irqs {
        SAADC => saadc::InterruptHandler;
    });

    let led_pin = |p| Output::new(p, Level::Low, OutputDrive::Standard);
    let red = led_pin(AnyPin::from(board.p9));
    let green = led_pin(AnyPin::from(board.p8));
    let blue = led_pin(AnyPin::from(board.p16));
    let mut rgb: Rgb<8> = Rgb::new([red, green, blue], 100);

    let mut saadc_config = saadc::Config::default();
    saadc_config.resolution = saadc::Resolution::_14BIT;
    let saadc = saadc::Saadc::new(
        board.saadc,
        Irqs,
        saadc_config,
        [saadc::ChannelConfig::single_ended(board.p2)],
    );
    let _knob = Knob::new(saadc).await;

    rgb.set_rgb(|rgb| {
        for color in rgb {
            *color = 7;
        }
    });
    loop {
        rgb.step().await;
    }
}