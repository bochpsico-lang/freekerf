//! Property: whatever the lines and however the board acknowledges them, the host
//! never puts more bytes in the RX buffer than the board has, and every line
//! arrives once, in order.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use freekerf_emulator::DeviceConfig;
use freekerf_protocol::{HostOptions, StreamingMode};
use freekerf_testkit::{Bench, wire};
use proptest::prelude::*;

fn line() -> impl Strategy<Value = String> {
    // lines of 1 to 60 bytes on the wire
    "G1 X[0-9]{1,6}(\\.[0-9]{1,3})? Y[0-9]{1,6} F[0-9]{2,4}( S[0-9]{1,4})?( ; [a-z ]{0,20})?"
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    #[test]
    fn rx_buffer_never_overflows(
        lines in proptest::collection::vec(line(), 1..60),
        releases in proptest::collection::vec(1usize..4, 1..200),
        rx in prop_oneof![Just(127usize), Just(128), Just(256)],
        mode in prop_oneof![Just(StreamingMode::Buffered), Just(StreamingMode::Synchronous)],
    ) {
        let opt = format!("[OPT:V,15,{rx}]");
        let dev = DeviceConfig { rx_size: rx, opt_line: opt, report_buffer: false, ..DeviceConfig::default() };
        let mut b = Bench::with(HostOptions { streaming_mode: mode, ..HostOptions::default() }, dev);
        b.connect();
        b.device.set_auto_ack(false);
        b.load(&lines.join("\n"));
        b.host.run_program(false, b.now).unwrap();
        b.exchange();
        for n in releases {
            b.device.release(n);
            b.exchange();
            prop_assert!(b.device.rx_used() <= rx);
        }
        b.device.set_auto_ack(true);
        prop_assert!(b.run_until(|b| !b.host.in_program() && b.host.ready(), 60_000));
        prop_assert_eq!(b.device.overflows(), 0);
        prop_assert!(b.device.max_rx_used() <= rx);
        let mut expected = vec!["G90".to_owned()];
        expected.extend(lines.iter().map(|l| wire(l.split(';').next().unwrap().trim())));
        expected.push("G0X0Y0Z0".to_owned());
        prop_assert_eq!(b.lines(), expected);
    }
}
