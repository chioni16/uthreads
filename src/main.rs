#![feature(naked_functions)]

mod channel;
mod runtime;
mod thread;

use channel::Channel;
use runtime::{chan_recv, chan_send, create_thread, Runtime};
use thread::Id;

const DEFAULT_STACK_SIZE: usize = 1024 * 5;
const BASE_THREAD_ID: Id = Id(0);
const DEBUG: bool = true;

// We make use of global variables in order to avoid having to pass the Runtime / Channel to every function called.
// This is not a problem with Runtime, as there is always supposed to have a maximum of one Runtime at any point in time.
// But, there are legit reason for an application to make use of more than one channel at a time, which is not ergonomic at the moment.
// But this works just fine as a toy runtime and does what it's designed to do.
static mut RUNTIME: *mut Runtime = std::ptr::null_mut();
static mut CHAN: *mut Channel<usize> = std::ptr::null_mut();

fn main() {
    // Initialise global variables: Runtime and Channel before using them.
    let mut runtime = Runtime::new();
    let chan = Box::from(Channel::new(1));
    unsafe {
        runtime.init();
        CHAN = Box::into_raw(chan);
    }

    // Create tasks to be run.
    // New tasks can be created from within another task.
    create_thread(move || {
        println!("THREAD 1 STARTING");
        let id = 1;
        for i in 0..10 {
            println!("thread: {} counter: {}", id, i);
            // yield_thread()
            println!("Thread {:?} received: {:?}", id, unsafe { chan_recv(CHAN) });
        }
        println!("THREAD 1 FINISHED");

        create_thread(|| {
            println!("THREAD 3 STARTING");
            let id = 3;
            for i in 0..20 {
                println!("thread: {} counter: {}", id, i);
                // yield_thread();
                println!("Thread {:?} received: {:?}", id, unsafe { chan_recv(CHAN) });
            }
            println!("THREAD 3 FINISHED");
        });
    });
    create_thread(|| {
        println!("THREAD 2 STARTING");
        let id = 2;
        for i in 0..15 {
            println!("thread: {} counter: {}", id, i);
            // yield_thread();
            unsafe {
                chan_send(CHAN, i + 1);
            }
        }
        println!("THREAD 2 FINISHED");
    });

    // Run the tasks created.
    runtime.run();

    // Destroy the global variables created at the beginning.
    unsafe {
        let _ = Box::from_raw(CHAN);
    }
}
