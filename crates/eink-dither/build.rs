use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// IEC 61966-2-1 exact formula: sRGB to linear
fn srgb_to_linear_exact(srgb: f64) -> f64 {
    if srgb <= 0.04045 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}

/// IEC 61966-2-1 exact formula: linear to sRGB
fn linear_to_srgb_exact(linear: f64) -> f64 {
    if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("gamma_lut.rs");
    let mut file = File::create(&dest_path).unwrap();

    // Generate SRGB_TO_LINEAR LUT (4096 entries)
    writeln!(file, "/// Lookup table for sRGB to linear conversion").unwrap();
    writeln!(file, "/// Index: srgb value * 4095.0, Value: linear value").unwrap();
    writeln!(file, "pub static SRGB_TO_LINEAR: [f32; 4096] = [").unwrap();
    for i in 0..4096 {
        let srgb = i as f64 / 4095.0;
        let linear = srgb_to_linear_exact(srgb);
        if i > 0 && i % 8 == 0 {
            writeln!(file).unwrap();
        }
        write!(file, "    {:.9},", linear as f32).unwrap();
    }
    writeln!(file, "\n];").unwrap();

    writeln!(file).unwrap();

    // Generate LINEAR_TO_SRGB LUT (4096 entries)
    writeln!(file, "/// Lookup table for linear to sRGB conversion").unwrap();
    writeln!(file, "/// Index: linear value * 4095.0, Value: sRGB value").unwrap();
    writeln!(file, "pub static LINEAR_TO_SRGB: [f32; 4096] = [").unwrap();
    for i in 0..4096 {
        let linear = i as f64 / 4095.0;
        let srgb = linear_to_srgb_exact(linear);
        if i > 0 && i % 8 == 0 {
            writeln!(file).unwrap();
        }
        write!(file, "    {:.9},", srgb as f32).unwrap();
    }
    writeln!(file, "\n];").unwrap();

    // Rerun if build.rs changes
    println!("cargo::rerun-if-changed=build.rs");
}
