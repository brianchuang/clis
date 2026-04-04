use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::config::{self, Config};
use crate::terminal;

// Core Foundation types
type CFMachPortRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFAllocatorRef = *const c_void;
type CFIndex = isize;
type CFStringRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFBooleanRef = *const c_void;

// Core Graphics types
type CGEventRef = *mut c_void;
type CGEventTapProxy = *mut c_void;
type CGEventType = u32;
type CGEventMask = u64;

type CGEventTapCallBack = unsafe extern "C" fn(
    proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef;

const K_CG_SESSION_EVENT_TAP: u32 = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
const K_CG_EVENT_KEY_DOWN: u32 = 10;
const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;
const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: CGEventMask,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

// kAXTrustedCheckOptionPrompt key from HIServices
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopCommonModes: CFStringRef;
    static kCFBooleanTrue: CFBooleanRef;
    fn CFDictionaryCreate(
        allocator: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: CFIndex,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFDictionaryRef;
    fn CFRelease(cf: *const c_void);
    fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: CFRunLoopRef);
}

/// Check if the process has Accessibility permissions.
///
/// We use `AXIsProcessTrustedWithOptions` instead of the simpler `AXIsProcessTrusted`
/// because when called with `kAXTrustedCheckOptionPrompt: true`, macOS shows its
/// native permission dialog and adds the calling binary to the Accessibility list.
/// This only works when the binary lives inside a .app bundle (see `create_app_bundle`
/// in main.rs) — raw binaries are silently ignored by the prompt.
pub fn check_accessibility(prompt: bool) -> bool {
    unsafe {
        if !prompt {
            let opts = std::ptr::null();
            return AXIsProcessTrustedWithOptions(opts);
        }
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let dict = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            std::ptr::null(),
            std::ptr::null(),
        );
        let trusted = AXIsProcessTrustedWithOptions(dict);
        CFRelease(dict);
        trusted
    }
}

struct HotkeyContext {
    keycode: u16,
    modifier_mask: u64,
    terminal_app: String,
    tap: CFMachPortRef,
}

// Mask to isolate the modifier keys we care about (cmd, shift, ctrl, alt).
// Ignores device-dependent flags and caps lock.
const MODIFIER_FILTER: u64 = (1 << 20) | (1 << 17) | (1 << 18) | (1 << 19);

unsafe extern "C" fn hotkey_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    let ctx = &*(user_info as *const HotkeyContext);

    // macOS disables event taps after a timeout if the callback is too slow
    // or the system is under load. Re-enable it when that happens.
    if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT {
        eprintln!("Event tap was disabled by timeout, re-enabling...");
        CGEventTapEnable(ctx.tap, true);
        return event;
    }

    if event_type != K_CG_EVENT_KEY_DOWN {
        return event;
    }

    let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16;
    let flags = CGEventGetFlags(event) & MODIFIER_FILTER;

    if keycode == ctx.keycode && flags == ctx.modifier_mask {
        terminal::launch_tui(&ctx.terminal_app);
    }

    event
}

/// Install the global hotkey and run the event loop.
/// This blocks the calling thread until `running` is set to false.
pub fn install_and_run(cfg: &Config, running: Arc<AtomicBool>) {
    let keycode = match config::keycode_for(&cfg.hotkey.key) {
        Some(k) => k,
        None => {
            eprintln!(
                "Unknown hotkey key: '{}'. Run `rippy hotkey show` for valid keys.",
                cfg.hotkey.key
            );
            return;
        }
    };

    let modifier_mask = config::modifiers_mask(&cfg.hotkey.modifiers);
    let event_mask: CGEventMask = 1 << K_CG_EVENT_KEY_DOWN;

    unsafe {
        let run_loop = CFRunLoopGetCurrent();

        // Create the context with a null tap initially — we update it after
        // CGEventTapCreate succeeds so the callback can re-enable the tap.
        let ctx = Box::new(HotkeyContext {
            keycode,
            modifier_mask,
            terminal_app: cfg.terminal.app.clone(),
            tap: std::ptr::null_mut(),
        });
        let ctx_ptr = Box::into_raw(ctx);

        let tap = CGEventTapCreate(
            K_CG_SESSION_EVENT_TAP,
            K_CG_HEAD_INSERT_EVENT_TAP,
            K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
            event_mask,
            hotkey_callback,
            ctx_ptr as *mut c_void,
        );

        if tap.is_null() {
            eprintln!("Failed to create event tap. Requesting Accessibility permission...");
            check_accessibility(true);
            // Clean up
            let _ = Box::from_raw(ctx_ptr);
            return;
        }

        // Now store the tap handle so the callback can re-enable it on timeout.
        (*ctx_ptr).tap = tap;

        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);

        eprintln!(
            "Hotkey {} registered. Listening...",
            config::format_hotkey(&cfg.hotkey)
        );

        // Poll the running flag on a background thread; stop the run loop when signaled.
        let running_clone = running.clone();
        let rl_addr = run_loop as usize;
        std::thread::spawn(move || {
            while running_clone.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            CFRunLoopStop(rl_addr as CFRunLoopRef);
        });

        CFRunLoopRun();

        // Clean up
        let _ = Box::from_raw(ctx_ptr);
    }
}
