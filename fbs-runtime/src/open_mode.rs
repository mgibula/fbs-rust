
#[derive(Debug, Default, Clone, Copy)]
pub struct OpenMode {
    flags: i32,
    mode: u32,
}

impl OpenMode {
    pub fn new() -> Self {
        OpenMode { flags: libc::O_RDWR, mode: 0 }
    }

    pub fn read_only(&mut self) -> &mut Self {
        self.flags ^= !(libc::O_RDWR | libc::O_RDONLY | libc::O_WRONLY);
        self.flags |= libc::O_RDONLY;

        self
    }

    pub fn write_only(&mut self) -> &mut Self {
        self.flags ^= !(libc::O_RDWR | libc::O_RDONLY | libc::O_WRONLY);
        self.flags |= libc::O_WRONLY;

        self
    }

    pub fn read_write(&mut self) -> &mut Self {
        self.flags ^= !(libc::O_RDWR | libc::O_RDONLY | libc::O_WRONLY);
        self.flags |= libc::O_RDWR;

        self
    }

    pub fn append(&mut self, value: bool) -> &mut Self {
        if value {
            self.flags |= libc::O_CREAT;
        } else {
            self.flags ^= !libc::O_CREAT;
        }

        self
    }

    pub fn close_on_exec(&mut self, value: bool) -> &mut Self {
        if value {
            self.flags |= libc::O_CLOEXEC;
        } else {
            self.flags ^= !libc::O_CLOEXEC;
        }

        self
    }

    pub fn create(&mut self, value: bool, mode: u32) -> &mut Self {
        if value {
            self.flags |= libc::O_CREAT;
            self.mode = mode;
        } else {
            self.flags ^= !libc::O_CREAT;
        }

        self
    }

    pub fn exists(&mut self, value: bool) -> &mut Self {
        if value {
            self.flags |= libc::O_EXCL;
        } else {
            self.flags ^= !libc::O_EXCL;
        }

        self
    }

    pub fn direct(&mut self, value: bool) -> &mut Self {
        if value {
            self.flags |= libc::O_DIRECT;
        } else {
            self.flags ^= !libc::O_DIRECT;
        }

        self
    }

    pub fn non_blocking(&mut self, value: bool) -> &mut Self {
        if value {
            self.flags |= libc::O_NONBLOCK;
        } else {
            self.flags ^= !libc::O_NONBLOCK;
        }

        self
    }

    pub fn truncate(&mut self, value: bool) -> &mut Self {
        if value {
            self.flags |= libc::O_TRUNC;
        } else {
            self.flags ^= !libc::O_TRUNC;
        }

        self
    }

    pub fn set_flags(&mut self, flags: i32) -> &mut Self {
        self.flags = flags;
        self
    }

    pub fn flags(&self) -> i32 {
        self.flags
    }

    pub fn set_mode(&mut self, mode: u32) -> &mut Self {
        self.mode = mode;
        self
    }

    pub fn mode(&self) -> u32 {
        self.mode
    }

}
