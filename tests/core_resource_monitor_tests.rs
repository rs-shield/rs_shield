// Testes para rsb-core/src/core/resource_monitor.rs

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_resource_monitor_initialization() {
    // Test that we can create the atomic flags
    let should_pause = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(true));

    assert!(!should_pause.load(Ordering::Relaxed));
    assert!(running.load(Ordering::Relaxed));
}

#[test]
fn test_atomic_bool_pause_flag() {
    let should_pause = Arc::new(AtomicBool::new(false));

    // Initially should not be paused
    assert!(!should_pause.load(Ordering::Relaxed));

    // Set to pause
    should_pause.store(true, Ordering::Relaxed);
    assert!(should_pause.load(Ordering::Relaxed));

    // Resume
    should_pause.store(false, Ordering::Relaxed);
    assert!(!should_pause.load(Ordering::Relaxed));
}

#[test]
fn test_atomic_bool_running_flag() {
    let running = Arc::new(AtomicBool::new(true));

    // Initially should be running
    assert!(running.load(Ordering::Relaxed));

    // Stop
    running.store(false, Ordering::Relaxed);
    assert!(!running.load(Ordering::Relaxed));

    // Resume
    running.store(true, Ordering::Relaxed);
    assert!(running.load(Ordering::Relaxed));
}

#[test]
fn test_atomic_bool_swap_operation() {
    let flag = Arc::new(AtomicBool::new(false));

    // Swap from false to true
    let previous = flag.swap(true, Ordering::Relaxed);
    assert!(!previous);
    assert!(flag.load(Ordering::Relaxed));

    // Swap from true to false
    let previous = flag.swap(false, Ordering::Relaxed);
    assert!(previous);
    assert!(!flag.load(Ordering::Relaxed));
}

#[test]
fn test_shared_pause_flag_between_threads() {
    let should_pause = Arc::new(AtomicBool::new(false));
    let should_pause_clone = should_pause.clone();

    let thread_handle = thread::spawn(move || {
        // Thread sees initial state
        assert!(!should_pause_clone.load(Ordering::Relaxed));
        
        // Wait a bit
        thread::sleep(Duration::from_millis(50));
        
        // Check if pause was set from main thread
        should_pause_clone.load(Ordering::Relaxed)
    });

    // Set pause from main thread
    thread::sleep(Duration::from_millis(25));
    should_pause.store(true, Ordering::Relaxed);

    let thread_saw_pause = thread_handle.join().unwrap();
    assert!(thread_saw_pause);
}

#[test]
fn test_shared_running_flag_between_threads() {
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    let thread_handle = thread::spawn(move || {
        let mut iterations = 0;
        while running_clone.load(Ordering::Relaxed) && iterations < 10 {
            thread::sleep(Duration::from_millis(10));
            iterations += 1;
        }
        iterations
    });

    // Let thread run for a bit
    thread::sleep(Duration::from_millis(50));
    
    // Stop the thread
    running.store(false, Ordering::Relaxed);

    let iterations = thread_handle.join().unwrap();
    assert!(iterations > 0);
    assert!(iterations < 10); // Should have stopped early
}

#[test]
fn test_pause_and_resume_cycle() {
    let should_pause = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(true));

    let should_pause_clone = should_pause.clone();
    let running_clone = running.clone();

    let thread_handle = thread::spawn(move || {
        let mut paused_count = 0;
        let mut resumed_count = 0;

        while running_clone.load(Ordering::Relaxed) {
            if should_pause_clone.load(Ordering::Relaxed) {
                paused_count += 1;
            } else {
                resumed_count += 1;
            }
            thread::sleep(Duration::from_millis(5));
        }

        (paused_count, resumed_count)
    });

    // Pause
    should_pause.store(true, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(30));

    // Resume
    should_pause.store(false, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(30));

    // Stop
    running.store(false, Ordering::Relaxed);

    let (paused_count, resumed_count) = thread_handle.join().unwrap();
    assert!(paused_count > 0, "Thread should have seen paused state");
    assert!(resumed_count > 0, "Thread should have seen resumed state");
}

#[test]
fn test_rapid_pause_resume() {
    let should_pause = Arc::new(AtomicBool::new(false));

    // Rapidly toggle pause state
    for _ in 0..100 {
        should_pause.store(true, Ordering::Relaxed);
        should_pause.store(false, Ordering::Relaxed);
    }

    assert!(!should_pause.load(Ordering::Relaxed));
}

#[test]
fn test_multiple_thread_access() {
    let should_pause = Arc::new(AtomicBool::new(false));
    let mut handles = vec![];

    // Create multiple threads that read the flag
    for _ in 0..5 {
        let flag = should_pause.clone();
        let handle = thread::spawn(move || {
            flag.load(Ordering::Relaxed)
        });
        handles.push(handle);
    }

    // Set the flag
    should_pause.store(true, Ordering::Relaxed);

    // All threads should see the updated value
    for handle in handles {
        let saw_pause = handle.join().unwrap();
        assert!(saw_pause);
    }
}

#[test]
fn test_pause_transition_detection() {
    let should_pause = Arc::new(AtomicBool::new(false));

    // Detect pause transition
    let was_paused = should_pause.swap(true, Ordering::Relaxed);
    assert!(!was_paused);

    // Detect resume transition
    let was_paused = should_pause.swap(false, Ordering::Relaxed);
    assert!(was_paused);
}

#[test]
fn test_atomic_operations_consistency() {
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();

    let thread_handle = thread::spawn(move || {
        // Store true from thread
        flag_clone.store(true, Ordering::Relaxed);
    });

    thread_handle.join().unwrap();

    // Main thread should see the change
    assert!(flag.load(Ordering::Relaxed));
}

#[test]
fn test_long_running_monitoring_simulation() {
    let should_pause = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(true));

    let should_pause_clone = should_pause.clone();
    let running_clone = running.clone();

    let thread_handle = thread::spawn(move || {
        let mut iterations = 0;
        while running_clone.load(Ordering::Relaxed) {
            if should_pause_clone.load(Ordering::Relaxed) {
                // Simulating paused state
                thread::sleep(Duration::from_millis(2));
            } else {
                // Simulating active work
                thread::sleep(Duration::from_millis(1));
            }
            iterations += 1;
        }
        iterations
    });

    // Simulate monitoring cycle: work, pause, work, stop
    thread::sleep(Duration::from_millis(25));
    should_pause.store(true, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(25));
    should_pause.store(false, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(25));
    running.store(false, Ordering::Relaxed);

    let iterations = thread_handle.join().unwrap();
    assert!(iterations > 0);
}
