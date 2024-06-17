use honggfuzz::fuzz;

/// see https://github.com/rust-fuzz/honggfuzz-rs/tree/master
fn main() {
    loop {
        fuzz!(|data: &[u8]| {
            // Bit
        });
    }
}
