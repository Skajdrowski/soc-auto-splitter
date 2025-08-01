#![no_std]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![warn(
    clippy::complexity,
    clippy::correctness,
    clippy::perf,
    clippy::style,
    clippy::undocumented_unsafe_blocks,
    rust_2018_idioms
)]

use asr::{
    Address, Process, PointerSize, deep_pointer::DeepPointer,
    file_format::pe,
    future::{next_tick, retry},
    settings::Gui,
    string::ArrayCString,
    timer::{self, TimerState},
    watcher::Watcher
};

asr::async_main!(stable);
asr::panic_handler!();

#[derive(Gui)]
struct Settings {
    #[default = true]
    Autosplit_per_level: bool,
    #[default = false]
    Slow_PC_mode: bool
}

#[derive(Default)]
struct Watchers {
    loadByte: Watcher<u8>,
    noControlByte: Watcher<u8>,
    isPausedByte: Watcher<u8>,
    syncFloat: Watcher<f32>,
    levelByte: Watcher<u8>,
    end: Watcher<ArrayCString<5>>
}

struct Memory {
    load: Address,
    noControl: Address,
    isPaused: Address,
    sync: Address,
    level: DeepPointer<7>,
    end: DeepPointer<8>
}

impl Memory {
    async fn init(process: &Process) -> Self {
        let baseModule = retry(|| process.get_module_address("XR_3DA.exe")).await;
        let xrNetServer = retry(|| process.get_module_address("xrNetServer.dll")).await;
        let xrGame = retry(|| process.get_module_address("xrGame.dll")).await;
        let xrCore = retry(|| process.get_module_address("xrCore.dll")).await;

        let baseModuleSize = retry(|| pe::read_size_of_image(process, baseModule)).await;
        //asr::print_limited::<128>(&format_args!("{}", baseModule));

        match baseModuleSize {
            1662976 | 1613824 | 1597440 => Self {
                load: xrNetServer + 0xFAC4,
                noControl: xrGame + 0x54C2F9,
                isPaused: baseModule + 0x1047C0,
                sync: baseModule + 0x104928,
                level: DeepPointer::new(xrCore, PointerSize::Bit32, &[0xBA040, 0x4, 0x0, 0x40, 0x8, 0x20, 0x14]),
                end: DeepPointer::new(baseModule, PointerSize::Bit32, &[0x1048BC, 0x54, 0x14, 0x0, 0x0, 0x44, 0xC, 0x12])
            },
            _ => Self {
                load: xrNetServer + 0x13E84,
                noControl: xrGame + 0x560668,
                isPaused: baseModule + 0x10BCD0,
                sync: baseModule + 0x10BE80,
                level: DeepPointer::new(xrCore, PointerSize::Bit32, &[0xBF368, 0x4, 0x0, 0x40, 0x8, 0x28, 0x4]),
                end: DeepPointer::new(baseModule, PointerSize::Bit32, &[0x10BDB0, 0x3C, 0x10, 0x0, 0x0, 0x44, 0xC, 0x12])
            }
        }
    }
}

fn start(watchers: &Watchers) -> bool {
    watchers.loadByte.pair.unwrap().changed_from_to(&0, &1)
}

fn isLoading(watchers: &Watchers) -> Option<bool> {
    let syncFloat = watchers.syncFloat.pair.unwrap();
    
    Some(
        watchers.loadByte.pair.unwrap().current == 0
        || watchers.isPausedByte.pair.unwrap().current == 0
        && syncFloat.current == 0.0 || syncFloat.current >= 0.057 && syncFloat.current <= 0.11
        || watchers.noControlByte.pair.unwrap().current == 1
    )
}

fn split(watchers: &Watchers, settings: &Settings) -> bool {
    match settings.Autosplit_per_level {
        true => {
            let levelByte = watchers.levelByte.pair.unwrap();

            levelByte.changed()
            && levelByte.old != 0
            && levelByte.current != 0
            || watchers.end.pair.unwrap().current.matches("final")
        },
        false => watchers.end.pair.unwrap().current.matches("final")
    }
}

fn mainLoop(process: &Process, memory: &Memory, watchers: &mut Watchers) {
    watchers.loadByte.update_infallible(process.read(memory.load).unwrap_or(1));

    watchers.noControlByte.update_infallible(process.read(memory.noControl).unwrap_or(0));
    watchers.isPausedByte.update_infallible(process.read(memory.isPaused).unwrap_or(0));
    watchers.syncFloat.update_infallible(process.read(memory.sync).unwrap_or(1337.69));

    watchers.levelByte.update_infallible(memory.level.deref(process).unwrap_or(0));
    watchers.end.update_infallible(memory.end.deref(process).unwrap_or_default());
}

async fn main() {
    let mut settings = Settings::register();

    asr::set_tick_rate(60.0);
    let mut tickToggled = false;

    loop {
        let process = Process::wait_attach("XR_3DA.exe").await;

        process.until_closes(async {
            let mut watchers = Watchers::default();
            let memory = Memory::init(&process).await;

            loop {
                settings.update();

                if settings.Slow_PC_mode && !tickToggled {
                    asr::set_tick_rate(30.0);
                    tickToggled = true;
                }
                else if !settings.Slow_PC_mode && tickToggled {
                    asr::set_tick_rate(60.0);
                    tickToggled = false;
                }

                mainLoop(&process, &memory, &mut watchers);

                if [TimerState::Running, TimerState::Paused].contains(&timer::state()) {
                    match isLoading(&watchers) {
                        Some(true) => timer::pause_game_time(),
                        Some(false) => timer::resume_game_time(),
                        _ => ()
                    }

                    if split(&watchers, &settings) {
                        timer::split();
                    }
                }

                if timer::state().eq(&TimerState::NotRunning) && start(&watchers) {
                    timer::pause_game_time();
                    timer::start();
                }

                next_tick().await;
            }
        }).await;
    }
}
