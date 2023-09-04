use crate::DEFAULT_STACK_SIZE;

/// Uniquely identifies a thread.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(transparent)]
pub struct Id(pub usize);

/// Possible states that a thread can be in during its lifetime.
#[derive(PartialEq, Eq, Debug)]
pub enum State {
    /// Thread is making progress.
    Running,
    /// Thread is ready to be run and is not waiting on any external event.
    Ready,
    /// Thread is unable to send a value to a channel and is hence blocked until the channel frees up.
    ChannelBlockSend,
    /// Thread is waiting to receive a value from the channel.
    ChannelBlockRecv,
}

/// Stores information about a thread that we want preserved between thread switches.
/// Currently, we only store the callee saved registers.
#[derive(Debug, Default)]
#[repr(C)]
pub struct Context {
    pub rsp: u64,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
}

/// Represents a thread in our runtime.
#[derive(Debug)]
pub struct Thread {
    /// Uniquely identifies a thread.
    pub id: Id,
    /// Stack used by the thread to run the function passed.
    pub stack: Box<[u8]>,
    /// Stores the thread context between successive runs.
    pub ctx: Context,
    /// Represents the current state of the thread.
    pub state: State,
    /// Stores the value sent by the channel, if any.
    pub chan_val: Option<usize>,
}

impl Thread {
    pub fn new(id: Id, state: State) -> Self {
        Thread {
            id,
            stack: vec![0_u8; DEFAULT_STACK_SIZE].into_boxed_slice(),
            ctx: Context::default(),
            state,
            chan_val: None,
        }
    }
}
