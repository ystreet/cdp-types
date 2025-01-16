#![no_main]
use libfuzzer_sys::fuzz_target;

use cdp_types::{CDPParser, CDPWriter};

use std::sync::OnceLock;

use log::info;

pub fn debug_init() {
    static TRACING: OnceLock<()> = OnceLock::new();
    TRACING.get_or_init(|| {
        env_logger::init()
    });
}

fuzz_target!(|data: &[u8]| {
    debug_init();
    let mut parser = CDPParser::new();
    if let Ok(_) = parser.parse(data) {
        let mut writer = CDPWriter::new();
        while let Some(p) = parser.pop_packet() {
            info!("parsed {p:?}");
            writer.push_packet(p);
        }
        if let Some(cea608) = parser.cea608() {
            for pair in cea608.iter() {
                writer.push_cea608(*pair);
            }
        }
        writer.set_time_code(parser.time_code());
        let mut written = vec![];
        let _ = writer.write(parser.framerate().unwrap(), &mut written);
    }
});
