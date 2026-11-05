use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use aegis::aegis256::Aegis256;
use aes_gcm::{
    AeadCore, Aes256Gcm,
    aead::{Aead, KeyInit, Payload},
};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

const AAD: &[u8] = b"rustnote-e0-benchmark-v1";
const KEY: [u8; 32] = [0xA7; 32];

#[derive(Clone, Copy)]
enum Mode {
    Quick,
    Full,
}

#[derive(Clone, Copy)]
struct PayloadCase {
    label: &'static str,
    bytes: usize,
    quick_iterations: usize,
    full_iterations: usize,
}

const PAYLOADS: &[PayloadCase] = &[
    PayloadCase {
        label: "note-4KiB",
        bytes: 4 * 1024,
        quick_iterations: 1_000,
        full_iterations: 20_000,
    },
    PayloadCase {
        label: "document-1MiB",
        bytes: 1024 * 1024,
        quick_iterations: 20,
        full_iterations: 250,
    },
    PayloadCase {
        label: "attachment-16MiB",
        bytes: 16 * 1024 * 1024,
        quick_iterations: 2,
        full_iterations: 24,
    },
];

fn main() {
    let mode = match std::env::args().nth(1).as_deref() {
        None | Some("--quick") => Mode::Quick,
        Some("--full") => Mode::Full,
        Some(other) => {
            eprintln!("unknown argument: {other}\nusage: cargo run --release -- [--quick|--full]");
            std::process::exit(2);
        }
    };

    print_environment(mode);
    println!("\nAEAD throughput (allocation-inclusive one-shot API; higher is better)");
    println!("algorithm,payload,direction,MiB/s,iterations");

    for case in PAYLOADS {
        let iterations = match mode {
            Mode::Quick => case.quick_iterations,
            Mode::Full => case.full_iterations,
        };
        let input = deterministic_payload(case.bytes);

        bench_aegis(case.label, &input, iterations);
        bench_xchacha(case.label, &input, iterations);
        bench_aes_gcm(case.label, &input, iterations);
    }

    println!("\nArgon2id unlock-cost probes (lower latency is better; memory is deliberate)");
    println!("profile,memory_MiB,iterations,lanes,latency_ms");
    bench_argon2id("rfc9106-64MiB", 64 * 1024, 3, 4);
    if matches!(mode, Mode::Full) {
        bench_argon2id("single-lane-64MiB", 64 * 1024, 3, 1);
        bench_argon2id("stronger-128MiB", 128 * 1024, 3, 4);
    }

    println!("\nE0 note: these numbers characterize only this machine/build.");
    println!("Run --full on each minimum supported device before E1 selects defaults.");
}

fn print_environment(mode: Mode) {
    println!("Rustnote E0 cryptography benchmark");
    println!("mode={}", match mode { Mode::Quick => "quick", Mode::Full => "full" });
    println!("os={}", std::env::consts::OS);
    println!("arch={}", std::env::consts::ARCH);
    println!("hardware_aes={}", hardware_aes_available());
    println!("AEGIS-256 tag=256-bit nonce=256-bit key=256-bit");
    println!("XChaCha20-Poly1305 tag=128-bit nonce=192-bit key=256-bit");
    println!("AES-256-GCM tag=128-bit nonce=96-bit key=256-bit");
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn hardware_aes_available() -> bool {
    std::is_x86_feature_detected!("aes")
}

#[cfg(target_arch = "aarch64")]
fn hardware_aes_available() -> bool {
    std::arch::is_aarch64_feature_detected!("aes")
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
fn hardware_aes_available() -> bool {
    false
}

fn deterministic_payload(bytes: usize) -> Vec<u8> {
    (0..bytes)
        .map(|index| ((index.wrapping_mul(131).wrapping_add(17)) & 0xff) as u8)
        .collect()
}

fn bench_aegis(label: &str, input: &[u8], iterations: usize) {
    let start = Instant::now();
    let mut last = None;
    for iteration in 0..iterations {
        let nonce = wide_nonce(benchmark_counter(input.len(), iteration));
        let encrypted = Aegis256::<32>::new(&KEY, &nonce).encrypt(input, AAD);
        black_box(&encrypted);
        last = Some((nonce, encrypted));
    }
    report("AEGIS-256", label, "encrypt", input.len(), iterations, start.elapsed());

    let (nonce, (ciphertext, tag)) = last.expect("AEGIS benchmark must run at least once");
    let cipher = Aegis256::<32>::new(&KEY, &nonce);
    let start = Instant::now();
    for _ in 0..iterations {
        let plaintext = cipher
            .decrypt(&ciphertext, &tag, AAD)
            .expect("AEGIS benchmark ciphertext must authenticate");
        black_box(plaintext);
    }
    report("AEGIS-256", label, "decrypt", input.len(), iterations, start.elapsed());
}

fn bench_xchacha(label: &str, input: &[u8], iterations: usize) {
    let cipher = XChaCha20Poly1305::new_from_slice(&KEY).expect("32-byte XChaCha key");
    let start = Instant::now();
    let mut last = None;
    for iteration in 0..iterations {
        let nonce_bytes = xchacha_nonce(benchmark_counter(input.len(), iteration));
        let nonce = XNonce::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(&nonce, Payload { msg: input, aad: AAD })
            .expect("XChaCha benchmark encryption");
        black_box(&ciphertext);
        last = Some((nonce_bytes, ciphertext));
    }
    report(
        "XChaCha20-Poly1305",
        label,
        "encrypt",
        input.len(),
        iterations,
        start.elapsed(),
    );

    let (nonce_bytes, ciphertext) = last.expect("XChaCha benchmark must run at least once");
    let nonce = XNonce::from(nonce_bytes);
    let start = Instant::now();
    for _ in 0..iterations {
        let plaintext = cipher
            .decrypt(&nonce, Payload { msg: &ciphertext, aad: AAD })
            .expect("XChaCha benchmark ciphertext must authenticate");
        black_box(plaintext);
    }
    report(
        "XChaCha20-Poly1305",
        label,
        "decrypt",
        input.len(),
        iterations,
        start.elapsed(),
    );
}

fn bench_aes_gcm(label: &str, input: &[u8], iterations: usize) {
    type Aes256Nonce = aes_gcm::Nonce<<Aes256Gcm as AeadCore>::NonceSize>;

    let cipher = Aes256Gcm::new_from_slice(&KEY).expect("32-byte AES key");
    let start = Instant::now();
    let mut last = None;
    for iteration in 0..iterations {
        let nonce_bytes = aes_nonce(benchmark_counter(input.len(), iteration));
        let nonce = Aes256Nonce::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(&nonce, Payload { msg: input, aad: AAD })
            .expect("AES-GCM benchmark encryption");
        black_box(&ciphertext);
        last = Some((nonce_bytes, ciphertext));
    }
    report("AES-256-GCM", label, "encrypt", input.len(), iterations, start.elapsed());

    let (nonce_bytes, ciphertext) = last.expect("AES-GCM benchmark must run at least once");
    let nonce = Aes256Nonce::from(nonce_bytes);
    let start = Instant::now();
    for _ in 0..iterations {
        let plaintext = cipher
            .decrypt(&nonce, Payload { msg: &ciphertext, aad: AAD })
            .expect("AES-GCM benchmark ciphertext must authenticate");
        black_box(plaintext);
    }
    report("AES-256-GCM", label, "decrypt", input.len(), iterations, start.elapsed());
}

fn bench_argon2id(label: &str, memory_kib: u32, time_cost: u32, lanes: u32) {
    let params = Params::new(memory_kib, time_cost, lanes, Some(32))
        .expect("valid E0 Argon2id parameters");
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let password = b"rustnote-e0-representative-password";
    let salt = b"rustnote-e0-salt";
    let mut output = [0u8; 32];

    let start = Instant::now();
    argon
        .hash_password_into(password, salt, &mut output)
        .expect("Argon2id benchmark derivation");
    black_box(output);
    let latency = start.elapsed();

    println!(
        "{label},{:.0},{time_cost},{lanes},{:.2}",
        f64::from(memory_kib) / 1024.0,
        latency.as_secs_f64() * 1000.0
    );
}

fn report(
    algorithm: &str,
    payload: &str,
    direction: &str,
    bytes: usize,
    iterations: usize,
    elapsed: Duration,
) {
    let total_mib = (bytes as f64 * iterations as f64) / (1024.0 * 1024.0);
    let mib_per_second = total_mib / elapsed.as_secs_f64();
    println!("{algorithm},{payload},{direction},{mib_per_second:.2},{iterations}");
}

fn benchmark_counter(bytes: usize, iteration: usize) -> u64 {
    ((bytes as u64) << 32) ^ iteration as u64
}

fn wide_nonce(counter: u64) -> [u8; 32] {
    let mut nonce = [0u8; 32];
    nonce[24..].copy_from_slice(&counter.to_be_bytes());
    nonce
}

fn xchacha_nonce(counter: u64) -> [u8; 24] {
    let mut nonce = [0u8; 24];
    nonce[16..].copy_from_slice(&counter.to_be_bytes());
    nonce
}

fn aes_nonce(counter: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    nonce
}
