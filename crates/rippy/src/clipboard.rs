use objc2_app_kit::NSPasteboard;
use objc2_app_kit::NSPasteboardTypeString;
use objc2_foundation::NSString;

pub trait Clipboard {
    fn read(&self) -> (Option<String>, i64);
    fn write(&self, content: &str);
}

pub struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn read(&self) -> (Option<String>, i64) {
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
        unsafe {
            let pb = NSPasteboard::generalPasteboard();
            pb.clearContents();
            let ns_string = NSString::from_str(content);
            pb.setString_forType(&ns_string, NSPasteboardTypeString);
        }
    }
}

pub fn get_clipboard() -> (Option<String>, i64) {
    SystemClipboard.read()
}

pub fn set_clipboard(content: &str) {
    SystemClipboard.write(content);
}
