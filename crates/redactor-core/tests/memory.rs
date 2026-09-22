//! Memory behaviour, measured with a counting global allocator.
//!
//! Every allocation made by this test binary goes through `Counting`, which
//! tracks live bytes and the peak. Tests take a lock so they never overlap.
//! Lazily initialized statics (compiled regexes) are warmed up before a
//! baseline is taken, since they live for the whole process by design.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use redactor_core::{Config, CustomTerm, Redactor};

struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);
static SERIAL: Mutex<()> = Mutex::new(());

fn track(delta: isize) {
    let now = LIVE.fetch_add(delta, Ordering::SeqCst) + delta;
    PEAK.fetch_max(now, Ordering::SeqCst);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            track(layout.size() as isize);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        track(-(layout.size() as isize));
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            track(new_size as isize - layout.size() as isize);
        }
        new
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn live() -> isize {
    LIVE.load(Ordering::SeqCst)
}

fn reset_peak() -> isize {
    let now = live();
    PEAK.store(now, Ordering::SeqCst);
    now
}

fn fixtures() -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect()
}

fn config() -> Config {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    Config::load(format!("{root}/config.toml"))
        .unwrap()
        .with_engagement(&Config::load(format!("{root}/engagement.toml")).unwrap())
}

#[test]
fn repeated_redactions_do_not_leak() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let inputs = fixtures();
    let redactor = Redactor::new(&config());
    for input in &inputs {
        drop(redactor.redact(input)); // warm up statics and regex caches
    }

    let baseline = live();
    for _ in 0..300 {
        for input in &inputs {
            drop(redactor.redact(input));
        }
    }
    let retained = live() - baseline;
    assert!(
        retained < 16 * 1024,
        "{retained} bytes retained after 2100 redactions"
    );
}

#[test]
fn dropping_a_redactor_frees_its_dictionary() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    drop(Redactor::new(&config()).redact("warm up"));

    let big = Config {
        client: (0..500).map(|i| format!("Client Number {i}")).collect(),
        users: (0..500).map(|i| format!("user.{i}")).collect(),
        terms: (0..500)
            .map(|i| CustomTerm {
                value: format!("device-{i}"),
                replacement: None,
            })
            .collect(),
        ..Default::default()
    };
    let baseline = live();
    let redactor = Redactor::new(&big);
    let held = live() - baseline;
    drop(redactor);
    let retained = live() - baseline;
    assert!(held > 0);
    assert!(retained <= 0, "{retained} of {held} bytes not freed");
}

/// Redacts `input` once to warm up, then measures the peak and what stays
/// allocated after the result is dropped, over several runs.
fn measure(redactor: &Redactor, input: &str) -> (f64, isize, isize) {
    drop(redactor.redact(input));
    let baseline = reset_peak();
    let redaction = redactor.redact(input);
    let peak = PEAK.load(Ordering::SeqCst) - baseline;
    drop(redaction);
    let after_one = live() - baseline;
    for _ in 0..3 {
        drop(redactor.redact(input));
    }
    let after_four = live() - baseline;
    (peak as f64 / input.len() as f64, after_one, after_four)
}

#[test]
fn peak_memory_scales_with_input() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let redactor = Redactor::new(&config());
    let files = fixtures();

    // Worst case: a raw request where almost every line holds a secret
    // (~15 findings per KB), repeated to 1.3 MB.
    let dense = files[3].repeat(1000);
    // Typical case: a HAR export, repeated to ~3 MB.
    let har = files[2].repeat(1200);

    for (name, input, max_ratio) in [("dense", &dense, 5.0), ("har", &har, 4.0)] {
        let (ratio, after_one, after_four) = measure(&redactor, input);
        println!(
            "{name}: input {:.1} MB, peak {ratio:.1}x, retained {after_one} B after 1 run, {after_four} B after 4",
            input.len() as f64 / 1e6
        );
        assert!(ratio < max_ratio, "{name}: peak is {ratio:.1}x the input");
        // Regex engines keep a bounded lazy-DFA cache between calls; it must
        // not grow with the number of redactions.
        assert!(
            after_one < 4 * 1024 * 1024,
            "{name}: {after_one} B retained"
        );
        assert!(after_four <= after_one, "{name}: memory grows across runs");
    }
}
