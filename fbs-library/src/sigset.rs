use std::mem::MaybeUninit;
use super::system_error::SystemError;

#[repr(i32)]
#[non_exhaustive]
#[derive(Debug)]
pub enum Signal {
    SIGHUP          = libc::SIGHUP,
    SIGINT          = libc::SIGINT,
    SIGQUIT         = libc::SIGQUIT,
    SIGILL          = libc::SIGILL,
    SIGTRAP         = libc::SIGTRAP,
    SIGABRT         = libc::SIGABRT,
    SIGBUS          = libc::SIGBUS,
    SIGFPE          = libc::SIGFPE,
    SIGKILL         = libc::SIGKILL,
    SIGUSR1         = libc::SIGUSR1,
    SIGSEGV         = libc::SIGSEGV,
    SIGSTKFLT       = libc::SIGSTKFLT,
    SIGTTIN         = libc::SIGTTIN,
    SIGVTALRM       = libc::SIGVTALRM,
    SIGUSR2         = libc::SIGUSR2,
    SIGPIPE         = libc::SIGPIPE,
    SIGCHLD         = libc::SIGCHLD,
    SIGCONT         = libc::SIGCONT,
    SIGTTOU         = libc::SIGTTOU,
    SIGURG          = libc::SIGURG,
    SIGPROF         = libc::SIGPROF,
    SIGWINCH        = libc::SIGWINCH,
    SIGALRM         = libc::SIGALRM,
    SIGSTOP         = libc::SIGSTOP,
    SIGXCPU         = libc::SIGXCPU,
    SIGIO           = libc::SIGIO,
    SIGTERM         = libc::SIGTERM,
    SIGTSTP         = libc::SIGTSTP,
    SIGXFSZ         = libc::SIGXFSZ,
    SIGPWR          = libc::SIGPWR,
    SIGSYS          = libc::SIGSYS,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct SignalSet {
    sigset: libc::sigset_t,
}

impl SignalSet {
    pub fn empty() -> Self {
        unsafe {
            let mut result = MaybeUninit::<SignalSet>::zeroed().assume_init();
            libc::sigemptyset(&mut result.sigset);
            result
        }
    }

    pub fn full() -> Self {
        unsafe {
            let mut result = MaybeUninit::<SignalSet>::zeroed().assume_init();
            libc::sigfillset(&mut result.sigset);
            result
        }
    }

    pub fn add(&mut self, signal: Signal) {
        unsafe {
            libc::sigaddset(&mut self.sigset, signal as i32);
        }
    }

    pub fn remove(&mut self, signal: Signal) {
        unsafe {
            libc::sigdelset(&mut self.sigset, signal as i32);
        }
    }

    pub fn clear(&mut self) {
        unsafe {
            libc::sigemptyset(&mut self.sigset);
        }
    }

    pub fn fill(&mut self) {
        unsafe {
            libc::sigfillset(&mut self.sigset);
        }
    }

    pub fn is_set(&self, signal: Signal) -> bool {
        unsafe {
            match libc::sigismember(&self.sigset, signal as i32) {
                1 => true,
                0 => false,
                error => panic!("sigismember error: {}", error),
            }
        }
    }

    pub fn as_ptr(&self) -> *const libc::sigset_t {
        &self.sigset
    }
}

pub enum SignalMask {
    Block,
    Unblock,
    Set,
}

pub fn set_process_signal_mask(what: SignalMask, mask: SignalSet) -> Result<(), SystemError> {
    let what = match what  {
        SignalMask::Block => libc::SIG_BLOCK,
        SignalMask::Unblock => libc::SIG_UNBLOCK,
        SignalMask::Set => libc::SIG_SETMASK,
    };

    unsafe {
        let error = libc::sigprocmask(what, mask.as_ptr(), std::ptr::null_mut());
        match error {
            0 => Ok(()),
            _ => Err(SystemError::new_from_errno())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigset_full() {
        let sigset = SignalSet::full();

        assert_eq!(sigset.is_set(Signal::SIGHUP), true);
        assert_eq!(sigset.is_set(Signal::SIGINT), true);
        assert_eq!(sigset.is_set(Signal::SIGSYS), true);
    }

    #[test]
    fn sigset_empty() {
        let sigset = SignalSet::empty();

        assert_eq!(sigset.is_set(Signal::SIGHUP), false);
        assert_eq!(sigset.is_set(Signal::SIGINT), false);
        assert_eq!(sigset.is_set(Signal::SIGSYS), false);
    }

    #[test]
    fn sigset_add() {
        let mut sigset = SignalSet::empty();
        sigset.add(Signal::SIGINT);

        assert_eq!(sigset.is_set(Signal::SIGHUP), false);
        assert_eq!(sigset.is_set(Signal::SIGINT), true);
        assert_eq!(sigset.is_set(Signal::SIGSYS), false);
    }

    #[test]
    fn sigset_remove() {
        let mut sigset = SignalSet::full();
        sigset.remove(Signal::SIGINT);

        assert_eq!(sigset.is_set(Signal::SIGHUP), true);
        assert_eq!(sigset.is_set(Signal::SIGINT), false);
        assert_eq!(sigset.is_set(Signal::SIGSYS), true);
    }

    #[test]
    fn sigset_clear() {
        let mut sigset = SignalSet::full();
        sigset.clear();

        assert_eq!(sigset.is_set(Signal::SIGHUP), false);
        assert_eq!(sigset.is_set(Signal::SIGINT), false);
        assert_eq!(sigset.is_set(Signal::SIGSYS), false);
    }

    #[test]
    fn sigset_fill() {
        let mut sigset = SignalSet::empty();
        sigset.fill();

        assert_eq!(sigset.is_set(Signal::SIGHUP), true);
        assert_eq!(sigset.is_set(Signal::SIGINT), true);
        assert_eq!(sigset.is_set(Signal::SIGSYS), true);
    }
}
