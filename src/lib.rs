#![no_std]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(static_mut_refs)]

use asr::{future::{sleep}, settings::Gui, Process, PointerSize};
use core::{str, time::Duration};

asr::async_main!(stable);
asr::panic_handler!();

#[derive(Gui)]
struct Settings {
    #[default = true]
    Autosplit_per_level: bool,
}

struct Addr {
    loadAddress: u32,
    noControlAddress: u32,
    isPausedAddress: u32,
    syncAddress: u32,
    levelAddress: [u64; 7],
    endAddress: [u64; 8]
}

impl Addr {
    fn version0() -> Self {
        Self {
            loadAddress: 0xFAC4,
            noControlAddress: 0x54C2F9,
            isPausedAddress: 0x1047C0,
            syncAddress: 0x104928,
            levelAddress: [0xBA040, 0x4, 0x0, 0x40, 0x8, 0x20, 0x14],
            endAddress: [0x1048BC, 0x54, 0x14, 0x0, 0x0, 0x44, 0xC, 0x12]
        }
    }

    fn version0wine() -> Self {
        Self {
            loadAddress: 0xFAC4,
            noControlAddress: 0x54C2F9,
            isPausedAddress: 0x1047C0,
            syncAddress: 0x104928,
            levelAddress: [0xBA040, 0x4, 0x0, 0x40, 0x8, 0x20, 0x14],
            endAddress: [0x104858, 0x3C, 0x10, 0x0, 0x0, 0x44, 0xC, 0x12]
        }
    }

    fn version6() -> Self {
        Self {
            loadAddress: 0x13E84,
            noControlAddress: 0x560668,
            isPausedAddress: 0x10BCD0,
            syncAddress: 0x10BE80,
            levelAddress: [0xBF368, 0x4, 0x0, 0x40, 0x8, 0x28, 0x4],
            endAddress: [0x10BDB0, 0x3C, 0x10, 0x0, 0x0, 0x44, 0xC, 0x12]
        }
    }
}

async fn main() {
    let mut settings = Settings::register();
    
    static mut loadByte: u8 = 0;
    static mut oldLoad: u8 = 0;
    static mut syncFloat: f32 = 0.0;
    let mut noControlByte: u8 = 0;
    let mut isPausedByte: u8 = 0;

    static mut level: u8 = 0;
    static mut oldLevel: u8 = 0;

    static mut endByte: [u8; 5] = *b"     ";
    static mut endStr: &str = "";

    let mut baseAddress = asr::Address::new(0);
    let mut xrNetServerAddress = asr::Address::new(0);
    let mut xrGameAddress = asr::Address::new(0);
    let mut xrCoreAddress = asr::Address::new(0);

    let mut addrStruct = Addr::version6();
    loop {
        asr::timer::pause_game_time();
        let process = Process::wait_attach("XR_3DA.exe").await;
        process.until_closes(async {
            unsafe {
                if let Ok(moduleSize) = process.get_module_size("XR_3DA.exe") {
                    if moduleSize == 1662976 || moduleSize == 1613824 || moduleSize == 1597440 { //module sizes of patch 1.0000 | 1597440 = ENG Wine/Proton
                        if *asr::get_os().unwrap() != *"windows" {
                            addrStruct = Addr::version0wine();
                        }
                        else {
                            addrStruct = Addr::version0();
                        }
                    }
                }

                baseAddress = process.get_module_address("XR_3DA.exe").unwrap();
                loop {
                    syncFloat = process.read::<f32>(baseAddress + addrStruct.syncAddress).unwrap();
                    if syncFloat != 0.0 {
                        xrNetServerAddress = process.get_module_address("xrNetServer.dll").unwrap();
                        xrGameAddress = process.get_module_address("xrGame.dll").unwrap();
                        xrCoreAddress = process.get_module_address("xrCore.dll").unwrap();
                        break;
                    }
                    sleep(Duration::from_millis(250)).await;
                }

                let start = || {
                    if loadByte == 1 && oldLoad == 0 {
                        asr::timer::start();
                    }
                };

                let mut isLoading = || {
                    noControlByte = process.read::<u8>(xrGameAddress + addrStruct.noControlAddress).unwrap_or(2);
                    isPausedByte = process.read::<u8>(baseAddress + addrStruct.isPausedAddress).unwrap_or(2);

                    if loadByte == 0
                    || (syncFloat > 0.09 && syncFloat < 0.11)
                    || noControlByte == 1
                    || isPausedByte == 0 && syncFloat == 0.0 {
                        asr::timer::pause_game_time();
                    }
                    else {
                        asr::timer::resume_game_time();
                    }
                };

                let levelSplit = || {
                    level = process.read_pointer_path::<u8>(xrCoreAddress, PointerSize::Bit32, &addrStruct.levelAddress).unwrap_or(0);

                    if level != oldLevel && level != 0 && loadByte == 0 {
                        asr::timer::split();
                    }
                };

                let lastSplit = || {
                    endByte = process.read_pointer_path(baseAddress, PointerSize::Bit32, &addrStruct.endAddress).unwrap_or(*b"     ");
                    endStr = str::from_utf8(&endByte).unwrap_or("").split('\0').next().unwrap_or("");

                    if endStr == "final" && syncFloat == 0.0 {
                        asr::timer::split();
                    }
                };

                loop {
                    settings.update();

                    syncFloat = process.read::<f32>(baseAddress + addrStruct.syncAddress).unwrap_or(1.0);
                    loadByte = process.read::<u8>(xrNetServerAddress + addrStruct.loadAddress).unwrap_or(2);

                    start();
                    isLoading();
                    if settings.Autosplit_per_level {
                        levelSplit();
                    }
                    lastSplit();

                    oldLoad = loadByte;
                    oldLevel = level;
                    sleep(Duration::from_nanos(16666667)).await;
                }
            }
        }).await;
    }
}
