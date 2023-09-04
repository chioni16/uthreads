// no shared memory for communication
// message passing
// not references but owned types
// not sync or send - using raw pointers will ensure this.
// make channel copy

use std::alloc::{alloc_zeroed, Layout};
use std::cmp::Ordering;
use std::mem;

use crate::Id;

const BLOCK_QUEUE_SIZE: usize = 10;

// #[derive(Clone, Copy)]
pub struct Channel<T> {
    pub buffer: CircularBuffer<T>,
    pub sendq: CircularBuffer<(Id, T)>,
    pub recvq: CircularBuffer<Id>,
}

impl<T> Channel<T> {
    pub fn new(size: usize) -> Self {
        let buffer = CircularBuffer::<T>::new(size);
        let sendq = CircularBuffer::<(Id, T)>::new(BLOCK_QUEUE_SIZE);
        let recvq = CircularBuffer::<Id>::new(BLOCK_QUEUE_SIZE);

        Channel {
            buffer,
            sendq,
            recvq,
        }
    }
}

// #[derive(Clone, Copy)]
pub struct CircularBuffer<T> {
    inner: *mut T,
    write: usize,
    read: usize,
    size: usize,
    full: bool,
}

impl<T> CircularBuffer<T> {
    fn new(size: usize) -> Self {
        let align = mem::align_of::<T>();
        let ts = mem::size_of::<T>();
        let buf_size = ts.checked_mul(size).unwrap();

        let layout = Layout::from_size_align(buf_size, align).unwrap();
        let ptr = unsafe { alloc_zeroed(layout) };

        CircularBuffer {
            inner: ptr.cast(),
            write: 0,
            read: 0,
            size,
            full: size == 0,
        }
    }

    fn len(&self) -> usize {
        if self.full {
            return self.size;
        }

        match self.write.cmp(&self.read) {
            Ordering::Equal => 0,
            Ordering::Greater => self.write - self.read,
            Ordering::Less => self.size + self.write - self.read,
        }
    }

    fn inc_write(&mut self) {
        self.write = self.write.checked_add(1).unwrap() % self.size;
        self.full = self.write == self.read;
    }

    fn inc_read(&mut self) {
        self.read = self.read.checked_add(1).unwrap() % self.size;
        if self.full {
            self.full = false;
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn is_full(&self) -> bool {
        self.full
    }

    pub fn read(&mut self) -> Result<T, ()> {
        if self.is_empty() {
            return Err(());
        }

        let ret = unsafe {
            let ptr = self.inner.add(self.read);
            ptr.read()
        };
        self.inc_read();

        Ok(ret)
    }

    pub fn write(&mut self, val: T) -> Result<(), T> {
        if self.is_full() {
            return Err(val);
        }

        unsafe {
            let ptr = self.inner.add(self.write);
            ptr.write(val)
        }

        self.inc_write();

        Ok(())
    }
}

impl<T> Drop for CircularBuffer<T> {
    fn drop(&mut self) {
        let _ = unsafe { Vec::from_raw_parts(self.inner, 0, self.size) };
    }
}
