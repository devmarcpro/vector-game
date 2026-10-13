//! A light profiler: named spans accumulated per frame on the main thread (std only). Off by
//! default (one relaxed atomic load per span); `--profile` or the F10 overlay turns it on.
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static ON: AtomicBool = AtomicBool::new(false);

thread_local! {
    static ACC: RefCell<Vec<(&'static str, f64, u32)>> = const { RefCell::new(Vec::new()) };
}

pub fn set(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

/// A running span: adds its elapsed time to `name` when dropped.
pub struct Span(Option<(&'static str, Instant)>);

pub fn span(name: &'static str) -> Span {
    Span(enabled().then(|| (name, Instant::now())))
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some((name, t)) = self.0.take() {
            add(name, t.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

pub fn add(name: &'static str, ms: f64) {
    ACC.with(|a| {
        let mut a = a.borrow_mut();
        if let Some(e) = a.iter_mut().find(|e| e.0 == name) {
            e.1 += ms;
            e.2 += 1;
        } else {
            a.push((name, ms, 1));
        }
    });
}

/// Take what was accumulated (name, total ms, calls) and start again.
pub fn take() -> Vec<(&'static str, f64, u32)> {
    ACC.with(|a| std::mem::take(&mut *a.borrow_mut()))
}

/// Rolling overlay: the spans of the last frame, as lines of text.
pub fn overlay_lines(frames: u32) -> Vec<String> {
    let mut v = take();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v.iter()
        .take(12)
        .map(|(n, ms, c)| format!("{n:<18}{:>7.2} ms x{c}", ms / f64::from(frames.max(1))))
        .collect()
}

/// CPU time used so far by the whole process (all its threads), in ms: unlike the wall clock it
/// does not grow when the machine is busy with other programs, so benchmarks can compare it.
#[cfg(windows)]
pub fn cpu_ms() -> f64 {
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(h: isize, c: *mut u64, e: *mut u64, k: *mut u64, u: *mut u64) -> i32;
    }
    let (mut c, mut e, mut k, mut u) = (0u64, 0u64, 0u64, 0u64);
    // SAFETY: plain out-parameters of a kernel32 call on the current process.
    unsafe {
        GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u);
    }
    (k + u) as f64 / 10_000.0
}

#[cfg(not(windows))]
pub fn cpu_ms() -> f64 {
    0.0
}
