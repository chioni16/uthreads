#![feature(naked_functions)]

use core::arch::asm;

mod channel;

const DEFAULT_STACK_SIZE: usize = 1024 * 1024 * 2;
const BASE_THREAD_ID: Id = Id(0);
static mut RUNTIME: usize = 0;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(transparent)]
pub struct Id(usize);

#[derive(PartialEq, Eq, Debug)]
enum State {
    Running,
    Ready,
}

#[derive(Debug, Default)]
#[repr(C)]
struct ThreadContext {
    rsp: u64,
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
}

#[derive(Debug)]
struct Thread {
    id: Id,
    stack: Box<[u8]>,
    ctx: ThreadContext,
    state: State,
}

pub struct Runtime {
    threads: Vec<Thread>,
    current: Id,
    count: usize,
}

impl Runtime {
    pub fn new() -> Self {
        let base_thread = Thread {
            id: BASE_THREAD_ID,
            stack: vec![0_u8; DEFAULT_STACK_SIZE].into_boxed_slice(),
            ctx: ThreadContext::default(),
            state: State::Running,
        };

        Runtime {
            threads: vec![base_thread],
            current: BASE_THREAD_ID,
            count: 1,
        }
    }

    pub fn init(&self) {
        unsafe {
            let r_ptr: *const Runtime = self;
            RUNTIME = r_ptr as usize;
        }
    }

    pub fn run(&mut self) -> ! {
        while self.t_yield() {}
        std::process::exit(0);
    }

    #[inline]
    fn cur_pos(&self) -> usize {
        println!("from cur_pos: {:?}", self.current);
        self.threads
            .iter()
            .position(|t| t.id == self.current)
            .unwrap()
    }

    #[inline]
    fn round_robin(&self, start_pos: usize) -> Option<usize> {
        println!("from round_robin: start");

        let mut next_pos = start_pos;
        while self.threads[next_pos].state != State::Ready {
            next_pos += 1;
            if next_pos == self.threads.len() {
                next_pos = 0;
            }
            if next_pos == start_pos {
                println!("from round_robin stop: {:?}", self.current);
                return None;
            }
        }

        Some(next_pos)
    }

    #[inline(never)]
    fn t_return(&mut self) {
        if self.current != BASE_THREAD_ID {
            let cur_pos = self.cur_pos();
            
            println!("from return: {:?}", self.current);
            println!(
                "from return - before: {:?}",
                self.threads.iter().map(|t| t.id).collect::<Vec<_>>()
            );
            let mut cur_thread = self.threads.remove(cur_pos);
            println!(
                "from return - after: {:?}",
                self.threads.iter().map(|t| t.id).collect::<Vec<_>>()
            );
            
            let start_pos = if cur_pos == self.threads.len() { 0 } else { cur_pos };
            let next_pos = self.round_robin(start_pos).unwrap();
            self.threads[next_pos].state = State::Running;
            self.current = self.threads[next_pos].id;

            unsafe {
                let old: *mut ThreadContext = &mut cur_thread.ctx;
                let new: *const ThreadContext = &self.threads[next_pos].ctx;
                #[cfg(target_os = "linux")]
                asm!("call switch", in("rdi") old, in("rsi") new, clobber_abi("C"));
                #[cfg(target_os = "macos")]
                asm!("call _switch", in("rdi") old, in("rsi") new, clobber_abi("C"));
            }

            std::hint::black_box(())
        }
    }

    #[inline(never)]
    fn t_yield(&mut self) -> bool {
        let cur_pos = self.cur_pos();
        let Some(next_pos) = self.round_robin(cur_pos) else { return false };

        self.threads[cur_pos].state = State::Ready;
        self.threads[next_pos].state = State::Running;

        self.current = self.threads[next_pos].id;

        unsafe {
            let old: *mut ThreadContext = &mut self.threads[cur_pos].ctx;
            let new: *const ThreadContext = &self.threads[next_pos].ctx;
            #[cfg(target_os = "linux")]
            asm!("call switch", in("rdi") old, in("rsi") new, clobber_abi("C"));
            #[cfg(target_os = "macos")]
            asm!("call _switch", in("rdi") old, in("rsi") new, clobber_abi("C"));
        }

        std::hint::black_box(true)
    }

    pub fn spawn(&mut self, f: fn()) {
        let mut thread = Thread {
            id: Id(self.count),
            stack: vec![0_u8; DEFAULT_STACK_SIZE].into_boxed_slice(),
            ctx: ThreadContext::default(),
            state: State::Ready,
        };

        let size = thread.stack.len();
        unsafe {
            let s_ptr = thread.stack.as_mut_ptr().add(size);
            let s_ptr = (s_ptr as usize & !15) as *mut u8;
            std::ptr::write(s_ptr.offset(-16) as *mut usize, guard as usize);
            std::ptr::write(s_ptr.offset(-24) as *mut usize, skip as usize);
            std::ptr::write(s_ptr.offset(-32) as *mut usize, f as usize);
            thread.ctx.rsp = s_ptr.offset(-32) as u64;
        }

        self.threads.push(thread);
        self.count += 1;
    }
}


fn guard() {
    unsafe {
        let rt_ptr = RUNTIME as *mut Runtime;
        (*rt_ptr).t_return();
    };
}

#[naked]
unsafe extern "C" fn skip() {
    asm!("ret", options(noreturn))
}

pub fn yield_thread() {
    println!("from yield_thread");
    unsafe {
        let rt_ptr = RUNTIME as *mut Runtime;
        println!("from yield_thread 2: {}", rt_ptr as usize);
        (*rt_ptr).t_yield();
    };
}

pub fn spawn_thread(f: fn()) {
    println!("from spawn_thread");
    unsafe {
        let rt_ptr = RUNTIME as *mut Runtime;
        println!("from spawn_thread 2: {}", rt_ptr as usize);
        (*rt_ptr).spawn(f);
    };
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

fn main() {
    let mut runtime = Runtime::new();
    runtime.init();
    spawn_thread(|| {
        println!("THREAD 1 STARTING");
        let id = 1;
        for i in 0..10 {
            println!("thread: {} counter: {}", id, i);
            yield_thread();
        }
        println!("THREAD 1 FINISHED");
        
        spawn_thread(|| {
            println!("THREAD 3 STARTING");
            let jd = 3;
            for j in 0..20 {
                println!("thread: {} counter: {}", jd, j);
                yield_thread();
            }
            println!("THREAD 3 FINISHED");
        });
        
    });
    spawn_thread(|| {
        println!("THREAD 2 STARTING");
        let id = 2;
        for i in 0..15 {
            println!("thread: {} counter: {}", id, i);
            yield_thread();
        }
        println!("THREAD 2 FINISHED");
    });
    runtime.run();
}
