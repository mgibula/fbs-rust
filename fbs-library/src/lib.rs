use std::cell::Cell;

pub mod ip_address;
pub mod open_mode;
pub mod socket;
pub mod socket_address;
pub mod system_error;
pub mod sigset;
pub mod signalfd;
pub mod channel;
pub mod indexed_list;
pub mod poll;
pub mod pipe;
pub mod eventfd;

#[inline]
pub fn update_cell<T: Default, F: FnOnce(T) -> T>(cell: &Cell<T>, f: F) {
    let tmp = cell.take();
    cell.set(f(tmp));
}

#[inline]
pub fn with_cell<U, T: Default, F: FnOnce(&mut T) -> U>(cell: &Cell<T>, f: F) -> U {
    let mut tmp = cell.take();
    let result = f(&mut tmp);
    cell.set(tmp);

    result
}
