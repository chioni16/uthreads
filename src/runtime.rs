use core::arch::asm;
use core::fmt::Debug;

use crate::channel::Channel;
use crate::thread::{Context, Id, State, Thread};
use crate::{BASE_THREAD_ID, DEBUG, RUNTIME};

/// Represents a Runtime.
pub struct Runtime {
    /// All active threads, i.e, which haven't completed.
    /// Can store threads that are not currently running,
    /// but are waiting to be chosen by the runtime or for some other event to occur.
    threads: Vec<Thread>,
    /// Id of thread that is currently running.
    current: Id,
    /// Shows the total number of threads created up until a certain point.
    /// Used to generate unique thread IDs for threads spawned by a runtime.
    count: usize,
}

impl Runtime {
    pub fn new() -> Self {
        let base_thread = Thread::new(BASE_THREAD_ID, State::Running);

        Runtime {
            threads: vec![base_thread],
            current: BASE_THREAD_ID,
            count: 1,
        }
    }

    // Set the global RUNTIME to current Runtime.
    // This is done to avoid having to pass the Runtime struct to every function.
    // Note that the Runtime will have to be initialised before using it.
    // Also, in most cases, we only need to initialise it once and then destroy it when it's no longer needed,
    // i.e, once all the required tasks are completed. TODO
    pub unsafe fn init(&self) {
        unsafe {
            RUNTIME = self as *const _ as *mut _;
        }
    }

    pub fn run(&mut self) {
        if DEBUG {
            println!("started running from thread: {:?}", self.current);
        }
        // This is run on the main thread. It doesn't run any user code.
        // All it does is check if there are any pending threads that can be immediately run
        // and then pass on the control to such a thread, if present. If not, the runtime is closed.
        // As such, we stop the runtime when no immediately runnable threads are found.
        // But we ideally should wait for threads waiting on external events to complete.
        // Or introduce a timeout. TODO
        while self.yield_thread() {}
    }

    // Helper functions to get the position of a given (or current) thread in the vec of threads.
    // This is needed as removing the threads from the vec when they exit means that the vec position can be different from thread ID.
    fn cur_pos(&self) -> usize {
        self.threads
            .iter()
            .position(|t| t.id == self.current)
            .unwrap()
    }

    #[inline]
    fn get_pos(&self, id: Id) -> usize {
        self.threads.iter().position(|t| t.id == id).unwrap()
    }

    // Choose the next thread to be run.
    // Only the threads that are not waiting for some external event to occur and are ready are chosen.
    // Curretly, a rudimentary round robin algorithm is used to select the next thread,
    // but this can be replaced by something that accounts for thread priority, thread wait time etc.
    #[inline]
    fn round_robin(&self, start_pos: usize) -> Option<usize> {
        let mut next_pos = start_pos;
        while self.threads[next_pos].state != State::Ready {
            next_pos += 1;
            if next_pos == self.threads.len() {
                next_pos = 0;
            }
            if next_pos == start_pos {
                return None;
            }
        }

        Some(next_pos)
    }

    // Cleanup activities when a thread completes what it is asked to do.
    // And also, gives control back to another thread.
    #[inline(never)]
    fn done(&mut self) {
        // cleanup runs only for the non-main threads.
        if self.current != BASE_THREAD_ID {
            let cur_pos = self.cur_pos();

            if DEBUG {
                println!("from return: {:?}", self.current);
                println!(
                    "from return - before: {:?}",
                    self.threads.iter().map(|t| t.id).collect::<Vec<_>>()
                );
            }

            let mut cur_thread = self.threads.remove(cur_pos);

            if DEBUG {
                println!(
                    "from return - after: {:?}",
                    self.threads.iter().map(|t| t.id).collect::<Vec<_>>()
                );
            }

            // get the next thread to run.
            let start_pos = if cur_pos == self.threads.len() {
                0
            } else {
                cur_pos
            };
            let next_pos = self.round_robin(start_pos).unwrap();

            // bookkeeping to make sure that the thread states are consistent
            self.threads[next_pos].state = State::Running;
            self.current = self.threads[next_pos].id;

            // store and restore the thread contexts and jump to the target thread.
            unsafe {
                let old: *mut Context = &mut cur_thread.ctx;
                let new: *const Context = &self.threads[next_pos].ctx;

                if DEBUG {
                    println!(
                        "\told thread: {:?} @ {:#x}",
                        self.threads[cur_pos].id, old as usize
                    );
                    println!(
                        "\tnew thread: {:?} @ {:#x}",
                        self.threads[next_pos].id, new as usize
                    );
                }

                #[cfg(target_os = "linux")]
                asm!("call switch", in("rdi") old, in("rsi") new, clobber_abi("C"));
                // symbols in macos need an underscore at the beginning.
                #[cfg(target_os = "macos")]
                asm!("call _switch", in("rdi") old, in("rsi") new, clobber_abi("C"));
            }

            // We would like to avoid compiler optimising this out and actually run all the code up until this point
            std::hint::black_box(())
        }
    }

    // give control to another thread.
    #[inline(never)]
    fn yield_thread(&mut self) -> bool {
        if DEBUG {
            println!("called yield from: {:?}", self.current);
        }

        // get the next thread to run.
        let cur_pos = self.cur_pos();
        let Some(next_pos) = self.round_robin(cur_pos) else {
            // return false when no other runnable thread is found.
            return false;
        };

        if DEBUG {
            println!("\tswitching to {:?}...", self.threads[next_pos].id);
        }

        // bookkeeping to make sure that the thread states are consistent

        if self.threads[cur_pos].state == State::Running {
            self.threads[cur_pos].state = State::Ready;
        }

        self.threads[next_pos].state = State::Running;
        self.current = self.threads[next_pos].id;

        // store and restore the thread contexts and jump to the target thread.
        unsafe {
            let old: *mut Context = &mut self.threads[cur_pos].ctx;
            let new: *const Context = &self.threads[next_pos].ctx;

            if DEBUG {
                println!(
                    "\told thread: {:?} @ {:#x}",
                    self.threads[cur_pos].id, old as usize
                );
                println!(
                    "\tnew thread: {:?} @ {:#x}",
                    self.threads[next_pos].id, new as usize
                );
            }

            #[cfg(target_os = "linux")]
            asm!("call switch", in("rdi") old, in("rsi") new, clobber_abi("C"));
            // symbols in macos need an underscore at the beginning.
            #[cfg(target_os = "macos")]
            asm!("call _switch", in("rdi") old, in("rsi") new, clobber_abi("C"));
        }

        // we would like to avoid compiler optimising this out and actually run all the code up until this point
        std::hint::black_box(true)
    }

    pub fn create_thread(&mut self, f: fn()) {
        let mut thread = Thread::new(Id(self.count), State::Ready);

        // prepare the thread
        unsafe {
            let s_ptr = thread.stack.as_mut_ptr().add(thread.stack.len());
            let s_ptr = (s_ptr as usize & !15) as *mut u8;
            // add cleanup functions that are run when the user function returns
            std::ptr::write(s_ptr.offset(-16) as *mut usize, done as usize);
            // aligns stack to a 16 byte boundary
            std::ptr::write(s_ptr.offset(-24) as *mut usize, do_nothing as usize);
            // user function
            std::ptr::write(s_ptr.offset(-32) as *mut usize, f as usize);
            // bookkeeping
            thread.ctx.rsp = s_ptr.offset(-32) as u64;
        }

        if DEBUG {
            println!("spawned new thread: {:?}", thread.id);
        }

        self.threads.push(thread);
        self.count += 1;
    }

    fn change_thread_state(&mut self, id: Id, state: State) {
        let index = self.get_pos(id);
        let thread = &mut self.threads[index];

        if DEBUG {
            println!(
                "Changed thread {:?} from {:?} to {:?}",
                thread.id, thread.state, state
            );
        }

        thread.state = state;
    }

    fn add_val_to_chan<T: Debug>(&mut self, id: Id, val: T) {
        assert_ne!(self.current, id);

        let index = self.get_pos(id);
        let thread = &mut self.threads[index];

        assert!(thread.chan_val.is_none());

        if DEBUG {
            println!(
                "Thread {:?} wrote value {:?} to thread {:?}",
                self.current, val, id
            );
        }

        let boxed_val = Box::new(val);
        let ptr = Box::into_raw(boxed_val);

        thread.chan_val = Some(ptr as usize);
    }

    fn get_val_from_chan<T>(&mut self) -> Option<T> {
        let index = self.get_pos(self.current);
        let thread = &mut self.threads[index];
        thread
            .chan_val
            .take()
            .map(|ptr| *unsafe { Box::from_raw(ptr as *mut T) })
    }
}

// function which does nothing but just return
// takes care of the stack alignment rules for x86
#[naked]
unsafe extern "C" fn do_nothing() {
    asm!("ret", options(noreturn))
}

fn done() {
    unsafe {
        (*RUNTIME).done();
    };
}

fn get_current_thread() -> Id {
    unsafe { (*RUNTIME).current }
}

pub fn yield_thread() {
    unsafe {
        (*RUNTIME).yield_thread();
    }
}

pub fn create_thread(f: fn()) {
    unsafe {
        (*RUNTIME).create_thread(f);
    }
}

fn change_thread_state(id: Id, state: State) {
    unsafe {
        (*RUNTIME).change_thread_state(id, state);
    }
}

fn add_val_to_chan<T: Debug>(id: Id, val: T) {
    unsafe {
        (*RUNTIME).add_val_to_chan(id, val);
    }
}

fn get_val_from_chan<T>() -> Option<T> {
    unsafe { (*RUNTIME).get_val_from_chan() }
}

pub fn chan_send<T: Debug>(chan: *mut Channel<T>, val: T) {
    if DEBUG {
        println!("Called send on thread {:?}", get_current_thread());
    }

    let chan: &mut Channel<T> = unsafe { &mut *chan };

    // if there's a thread waiting to receive a value, 
    // directly give the value to the waiting thread.
    // And change the state of the receiving thread to Ready
    if let Ok(receiver) = chan.recvq.read() {
        add_val_to_chan(receiver, val);
        change_thread_state(receiver, State::Ready);
    }
    // try adding the value to the channel buffer
    else if let Err(val) = chan.buffer.write(val) {
        // In case the buffer is full, add the sender to the waiting list
        let curr_id = get_current_thread();
        chan.sendq
            .write((curr_id, val))
            .expect("No more space in sendq");
        // change the state of the sending thread to blocked
        change_thread_state(curr_id, State::ChannelBlockSend);
        // yield control to another thread
        yield_thread();
    }
}

pub fn chan_recv<T: Debug>(chan: *mut Channel<T>) -> T {
    if DEBUG {
        println!("Called receive on thread {:?}", get_current_thread());
    }

    let chan: &mut Channel<T> = unsafe { &mut *chan };

    // if there's a sender blocked on sending, get its value
    if let Ok((sender, val)) = chan.sendq.read() {
        if DEBUG {
            println!(
                "Found a ready to send thread {:?}, value = {:?}",
                sender, val
            );
        }
        // change the state of the blocked sender to ready
        change_thread_state(sender, State::Ready);
        return val;
    } else {
        // fetch value from channel buffer
        match chan.buffer.read() {
            Ok(val) => {
                if DEBUG {
                    println!(
                        "Thread {:?} found a value in the buffer: {:?}",
                        get_current_thread(),
                        val
                    );
                }
                return val;
            }
            // if no value present in the buffer, block
            Err(()) => {
                let curr_id = get_current_thread();
                // add the current thread to waiting list
                chan.recvq.write(curr_id).expect("No more space in recvq");
                change_thread_state(curr_id, State::ChannelBlockRecv);
                println!("Added thread {:?} to the recvq", get_current_thread());

                // yield control to another thread
                yield_thread();

                // here the control is given back to this thread
                // and a value is given from the chan it was blocked on
                get_val_from_chan()
                    .or_else(|| chan.buffer.read().ok())
                    .unwrap()
            }
        }
    }
}

#[naked]
#[no_mangle]
unsafe extern "C" fn switch() {
    asm!(
        "mov [rdi + 0x00], rsp",
        "mov [rdi + 0x08], r15",
        "mov [rdi + 0x10], r14",
        "mov [rdi + 0x18], r13",
        "mov [rdi + 0x20], r12",
        "mov [rdi + 0x28], rbx",
        "mov [rdi + 0x30], rbp",
        "mov rsp, [rsi + 0x00]",
        "mov r15, [rsi + 0x08]",
        "mov r14, [rsi + 0x10]",
        "mov r13, [rsi + 0x18]",
        "mov r12, [rsi + 0x20]",
        "mov rbx, [rsi + 0x28]",
        "mov rbp, [rsi + 0x30]",
        "ret",
        options(noreturn)
    );
}
