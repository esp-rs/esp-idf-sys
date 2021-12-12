use crate::*;

// NOTE: ESP-IDF-specific
const SPINLOCK_FREE_INIT: u32 = 0xB33FFFFF;
pub type EspCriticalSection = portMUX_TYPE;

impl EspCriticalSection {
    pub fn new() -> Self {
        Self {
            owner: SPINLOCK_FREE_INIT,
            count: 0,
        }
    }

    pub fn enter_critical(&mut self) {
        unsafe { vPortEnterCritical(self) }
    }

    pub fn exit_critical(&mut self) {
        unsafe { vPortExitCritical(self) }
    }

    pub fn scoped(&mut self) -> ScopedCriticalSection {
        self.enter_critical();
        ScopedCriticalSection::new(self)
    }
}

pub struct ScopedCriticalSection<'a>(&'a mut EspCriticalSection);

impl<'a> ScopedCriticalSection<'a> {
    fn new(ecm: &'a mut EspCriticalSection) -> Self {
        Self(ecm)
    }
}

impl<'a> Drop for ScopedCriticalSection<'a> {
    fn drop(&mut self) {
        self.0.exit_critical();
    }
}
