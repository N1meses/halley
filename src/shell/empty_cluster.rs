//! A non-modal, output-owned hint. Only a released, freshly pressed keyboard
//! shortcut can confirm; hiding it always forgets the pending confirmation.
use std::time::Duration;

use halley_core::cluster::ClusterId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub id: ClusterId,
    pub output: String,
    pub name: String,
    pub shortcut: String,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub name: String,
    pub shortcut: String,
    pub armed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Press {
    Ignored,
    Armed,
    Delete(ClusterId),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Prompt {
    target: Option<Target>,
    visible: bool,
    armed: bool,
    held: Option<u32>,
    quiet_until: Duration,
}

impl Prompt {
    pub fn sync(&mut self, target: Option<Target>, blocked: bool, now: Duration) -> bool {
        let changed_target = self.target != target;
        if changed_target {
            self.target = target;
            self.armed = false;
        }
        let visible = self.target.is_some() && !blocked && now >= self.quiet_until;
        let changed = changed_target || self.visible != visible || (!visible && self.armed);
        self.visible = visible;
        if !visible {
            self.armed = false;
        }
        changed
    }

    pub fn snapshot(&self, output: &str) -> Option<Snapshot> {
        self.target
            .as_ref()
            .filter(|target| self.visible && target.output == output)
            .map(|target| Snapshot {
                name: target.name.clone(),
                shortcut: target.shortcut.clone(),
                armed: self.armed,
            })
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn owns_output(&self, output: &str) -> bool {
        self.target
            .as_ref()
            .is_some_and(|target| target.output == output)
    }

    pub fn cancel(&mut self) -> bool {
        std::mem::take(&mut self.armed)
    }

    pub fn suspend(&mut self, now: Duration) {
        self.armed = false;
        self.visible = false;
        // Cover the asynchronous gap before a spawned launcher maps or takes
        // focus. Failed launches recover automatically without arming deletion.
        self.quiet_until = now + Duration::from_millis(750);
    }

    pub fn release(&mut self, keycode: u32) {
        if self.held == Some(keycode) {
            self.held = None;
        }
    }

    pub fn press(&mut self, keycode: u32) -> Press {
        if !self.visible || self.held.is_some() {
            return Press::Ignored;
        }
        self.held = Some(keycode);
        if self.armed {
            self.armed = false;
            self.visible = false;
            Press::Delete(self.target.as_ref().expect("visible target").id)
        } else {
            self.armed = true;
            Press::Armed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target(id: u64, output: &str) -> Target {
        Target {
            id: ClusterId::new(id),
            output: output.into(),
            name: "Work".into(),
            shortcut: "Super+Q".into(),
        }
    }
    #[test]
    fn requires_release_and_two_presses_and_is_output_owned() {
        let mut prompt = Prompt::default();
        prompt.sync(Some(target(1, "DP-1")), false, Duration::ZERO);
        assert!(prompt.snapshot("DP-2").is_none());
        assert_eq!(prompt.press(24), Press::Armed);
        assert_eq!(prompt.press(24), Press::Ignored);
        prompt.release(25);
        assert_eq!(prompt.press(24), Press::Ignored);
        prompt.release(24);
        assert_eq!(prompt.press(24), Press::Delete(ClusterId::new(1)));
    }
    #[test]
    fn overlay_and_launcher_cancel_confirmation_then_restore_unarmed_hint() {
        let mut prompt = Prompt::default();
        let t = Some(target(1, "DP-1"));
        prompt.sync(t.clone(), false, Duration::ZERO);
        prompt.press(24);
        prompt.release(24);
        prompt.sync(t.clone(), true, Duration::ZERO);
        assert!(!prompt.visible());
        prompt.sync(t.clone(), false, Duration::ZERO);
        assert!(!prompt.snapshot("DP-1").unwrap().armed);
        prompt.press(24);
        prompt.release(24);
        prompt.suspend(Duration::ZERO);
        prompt.sync(t.clone(), false, Duration::from_millis(500));
        assert!(!prompt.visible());
        prompt.sync(t, false, Duration::from_secs(1));
        assert!(!prompt.snapshot("DP-1").unwrap().armed);
    }
    #[test]
    fn new_window_leaving_switching_and_escape_forget_confirmation() {
        let mut prompt = Prompt::default();
        prompt.sync(Some(target(1, "DP-1")), false, Duration::ZERO);
        prompt.press(24);
        prompt.release(24);
        assert!(prompt.cancel());
        assert_eq!(prompt.press(24), Press::Armed);
        prompt.release(24);
        prompt.sync(None, false, Duration::ZERO);
        assert!(!prompt.visible());
        prompt.sync(Some(target(2, "DP-2")), false, Duration::ZERO);
        assert_eq!(prompt.press(24), Press::Armed);
    }
}
