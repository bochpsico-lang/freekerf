//! Number formatting against Mono 6.8: `fixtures/dotnet_format.tsv` holds 4000
//! seeded values formatted by Mono (`fixtures/DotnetFormat.cs`); every column must
//! match.

use freekerf_gcode::dotnet::{double_format, double_to_string, single_format, single_to_string};

#[test]
fn formatting_matches_mono() {
    let mut n = 0;
    for line in include_str!("fixtures/dotnet_format.tsv").lines() {
        let c: Vec<&str> = line.split('\t').collect();
        let v = f64::from_bits(u64::from_str_radix(c[0], 16).unwrap());
        assert_eq!(double_to_string(v), c[1], "{line}");
        assert_eq!(double_format(v, "0.###"), c[2], "{line}");
        assert_eq!(double_format(v, "0.00"), c[3], "{line}");
        assert_eq!(double_format(v, "0"), c[4], "{line}");
        let f = f32::from_bits(u32::from_str_radix(c[5], 16).unwrap());
        assert_eq!(single_to_string(f), c[6], "{line}");
        assert_eq!(single_format(f, "0.###"), c[7], "{line}");
        n += 1;
    }
    assert_eq!(n, 4000);
}
