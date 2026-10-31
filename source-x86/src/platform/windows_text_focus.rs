use std::{
    sync::{atomic::{AtomicUsize, AtomicU8, Ordering}, Arc},
    thread,
    time::Duration,
};
use windows::Win32::{
    System::{Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED}, Variant::VariantToBoolean},
    UI::Accessibility::{CUIAutomation8, IUIAutomation2, IUIAutomationTextPattern, IUIAutomationValuePattern, UIA_EditControlTypeId, UIA_IsReadOnlyAttributeId, UIA_TextPatternId, UIA_ValuePatternId},
};

/// UIA providers can block. Keep them off the connection task and never read a field's value.
pub struct TextFocusMonitor {
    worker: Arc<FocusWorker>,
}

struct FocusWorker { subscribers: AtomicUsize, state: AtomicU8 }

lazy_static::lazy_static! {
    // UIA's cross-bitness provider lifetime is tied to its creating COM apartment.
    // Keep that apartment alive between sessions; do not query without subscribers.
    static ref FOCUS_WORKER: Arc<FocusWorker> = FocusWorker::start();
}

#[derive(Default)]
pub struct TextFocusReporter {
    requested: bool,
    monitor: Option<TextFocusMonitor>,
    previous: Option<bool>,
}

impl TextFocusReporter {
    pub fn configure(&mut self, option: i32) {
        if option == 1 || option == 2 { self.requested = option == 2; }
    }
    pub fn sample(&mut self, allowed: bool) -> Option<bool> {
        let active = if self.requested && allowed {
            self.monitor.get_or_insert_with(TextFocusMonitor::new).active()?
        } else {
            self.monitor = None;
            if self.previous == Some(true) { false } else { return None; }
        };
        if self.previous == Some(active) { return None; }
        self.previous = Some(active);
        Some(active)
    }
}

impl TextFocusMonitor {
    pub fn new() -> Self {
        let worker = FOCUS_WORKER.clone();
        if worker.subscribers.fetch_add(1, Ordering::AcqRel) == 0 { worker.state.store(0, Ordering::Release); }
        Self { worker }
    }

    pub fn active(&self) -> Option<bool> {
        match self.worker.state.load(Ordering::Acquire) { 1 => Some(false), 2 => Some(true), _ => None }
    }
}

impl FocusWorker {
    fn start() -> Arc<Self> {
        let worker = Arc::new(Self { subscribers: AtomicUsize::new(0), state: AtomicU8::new(0) });
        let background = worker.clone();
        thread::spawn(move || unsafe {
            // Queried in the existing user-session connection manager, never the SYSTEM server.
            crate::platform::windows::try_change_desktop();
            let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let mut automation = if initialized { create_automation() } else { None };
            hbb_common::log::debug!("Text focus worker: COM {initialized}; UIA {}; system {}", automation.is_some(), crate::platform::windows::is_root());
            loop {
                if background.subscribers.load(Ordering::Acquire) > 0 {
                    if crate::platform::windows::try_change_desktop() && initialized { automation = create_automation(); }
                    background.state.store(if editable(automation.as_ref()) { 2 } else { 1 }, Ordering::Release);
                }
                thread::sleep(Duration::from_millis(250));
            }
        });
        worker
    }
}

unsafe fn create_automation() -> Option<IUIAutomation2> {
    let automation: IUIAutomation2 = match CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) {
        Ok(value) => value,
        Err(error) => { hbb_common::log::debug!("Text focus UIA initialization: {error}"); return None; }
    };
    if let Err(error) = automation.SetConnectionTimeout(200) { hbb_common::log::trace!("Text focus connection timeout: {error}"); }
    if let Err(error) = automation.SetTransactionTimeout(200) { hbb_common::log::trace!("Text focus transaction timeout: {error}"); }
    Some(automation)
}

impl Drop for TextFocusMonitor {
    fn drop(&mut self) { self.worker.subscribers.fetch_sub(1, Ordering::AcqRel); }
}

unsafe fn editable(automation: Option<&IUIAutomation2>) -> bool {
    if let Some(automation) = automation {
        if let Ok(element) = automation.GetFocusedElement() {
            if !element.CurrentHasKeyboardFocus().map(|v| v.as_bool()).unwrap_or(false)
                || !element.CurrentIsEnabled().map(|v| v.as_bool()).unwrap_or(false) { return false; }
            if let Ok(value) = element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                return value.CurrentIsReadOnly().map(|v| !v.as_bool()).unwrap_or(false);
            }
            if let Ok(text) = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
                if let Ok(range) = text.DocumentRange() {
                    if let Ok(attribute) = range.GetAttributeValue(UIA_IsReadOnlyAttributeId) {
                        if let Ok(readonly) = VariantToBoolean(&attribute) { return !readonly.as_bool(); }
                    }
                }
            }
            if element.CurrentControlType().ok() == Some(UIA_EditControlTypeId)
                && element.CurrentIsPassword().map(|v| v.as_bool()).unwrap_or(false) { return true; }
        }
    }
    let mut info: winapi::um::winuser::GUITHREADINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of_val(&info) as u32;
    winapi::um::winuser::GetGUIThreadInfo(0, &mut info) != 0
        && !info.hwndCaret.is_null() && info.hwndCaret == info.hwndFocus
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_clients_and_unauthorized_connections_get_no_focus_data() {
        let mut reporter = TextFocusReporter::default();
        assert_eq!(reporter.sample(true), None);
        reporter.configure(2);
        assert_eq!(reporter.sample(false), None);
        assert!(reporter.monitor.is_none());
    }
    #[test]
    fn revoking_input_or_opting_out_hides_keyboard_once() {
        let mut reporter = TextFocusReporter::default();
        reporter.configure(2);
        let worker = Arc::new(FocusWorker { subscribers: AtomicUsize::new(1), state: AtomicU8::new(2) });
        reporter.monitor = Some(TextFocusMonitor { worker: worker.clone() });
        reporter.previous = Some(true);
        assert_eq!(reporter.sample(false), Some(false));
        assert_eq!(worker.subscribers.load(Ordering::Acquire), 0);
        assert_eq!(reporter.sample(false), None);
        reporter.previous = Some(true);
        reporter.configure(1);
        assert_eq!(reporter.sample(true), Some(false));
    }
}
