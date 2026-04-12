pub trait Clipboard {
    fn read(&self) -> (Option<String>, i64);
    fn write(&self, content: &str);
}

pub struct SystemClipboard;

#[cfg(target_os = "macos")]
impl Clipboard for SystemClipboard {
    fn read(&self) -> (Option<String>, i64) {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};

        unsafe {
            let pb = NSPasteboard::generalPasteboard();
            let count = pb.changeCount() as i64;
            let content = pb
                .stringForType(NSPasteboardTypeString)
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            (content, count)
        }
    }

    fn write(&self, content: &str) {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::NSString;

        unsafe {
            let pb = NSPasteboard::generalPasteboard();
            pb.clearContents();
            let ns_string = NSString::from_str(content);
            pb.setString_forType(&ns_string, NSPasteboardTypeString);
        }
    }
}

#[cfg(target_os = "windows")]
impl Clipboard for SystemClipboard {
    fn read(&self) -> (Option<String>, i64) {
        let count = clipboard_win::seq_num()
            .map(|n| n.get() as i64)
            .unwrap_or(0);
        let content = clipboard_win::get_clipboard_string().ok().filter(|s| !s.is_empty());
        (content, count)
    }

    fn write(&self, content: &str) {
        clipboard_win::set_clipboard_string(content).ok();
    }
}

pub fn get_clipboard() -> (Option<String>, i64) {
    SystemClipboard.read()
}

pub fn set_clipboard(content: &str) {
    SystemClipboard.write(content);
}
