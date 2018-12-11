// numa_threadpool.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[cfg(not(feature = "numa"))]
pub use self::simple::*;

mod simple {
    use threadpool;
    use std::sync::Arc;
    use std::marker::Send;

    pub struct ThreadPool<Context> {
        context: Arc<Context>,
        pool: threadpool::ThreadPool
    }

    impl<Context> ThreadPool<Context>
        where Context: Send + Sync + 'static
    {
        pub fn new(context_ctor: impl Fn(u32)->Context) -> Self {
            let context = context_ctor(0);

            ThreadPool { context: Arc::new(context), pool: threadpool::ThreadPool::default() }
        }

        pub fn execute<Task>(&mut self, task: Task)
            where Task: Fn(&Context)->() + Send + 'static
        {
            let context = self.context.clone();
            self.pool.execute(move|| task(&context));
        }

        pub fn join(self) -> Vec<Context> {
            self.pool.join();
            let context = Arc::try_unwrap(self.context).unwrap_or_else(move|_|
                panic!("Some threads didn't exit somehow")
            );

            return vec![context];
        }

    }
}

#[cfg(feature = "numa")]
pub use self::numa::*;

#[cfg(feature = "numa")]
mod numa {
    extern crate nix;

    use std::ffi::CString;
    use std::sync::{Arc, Mutex, Condvar};
    use std::thread::{JoinHandle, self};
    use std::collections::VecDeque;
    use std::marker::Send;
    use std::os::raw::{c_ulong, c_uint, c_int, c_char};

    #[repr(C)]
    struct bitmask { 
        size: c_ulong, // in bits
        words: *mut c_ulong
    }

    #[link(name="numa")]
    extern {
        fn numa_available() -> i32;

        fn numa_allocate_cpumask() -> *mut bitmask;
        fn numa_bitmask_free(bitmask: *mut bitmask);
        fn numa_allocate_nodemask() -> *mut bitmask;

        fn numa_bitmask_clearall(bitmask: *mut bitmask);
        fn numa_sched_setaffinity(tid: c_uint, bitmask: *mut bitmask) -> c_int;

        fn numa_bitmask_isbitset(bitmask: *const bitmask, n: c_uint) -> bool;
        fn numa_bitmask_setbit(bitmask: *mut bitmask, n: c_uint);
        fn numa_bitmask_clearbit(bitmask: *mut bitmask, n: c_uint);
        fn numa_bitmask_nbytes(bitmask: *mut bitmask) -> c_uint;

        fn numa_node_to_cpus(node: c_uint, bitmask: *mut bitmask) -> c_int;
        fn numa_bitmask_weight(bitmask: *const bitmask) -> c_uint;

        fn numa_num_possible_nodes() -> c_uint;

        fn numa_error(desc: *const c_char);

        static numa_all_nodes_ptr: *const bitmask;
        static numa_all_cpus_ptr: *const bitmask;
    }

    struct Join {}
    type Task<Context> = Box<Fn(&Context)->() + Send + 'static>;

    struct NodeInfo<Context> {
        node_id: c_uint,
        cpuset: Vec<c_uint>,
        context: Context
    }

    struct WorkQueue<Context> {
        queue: Mutex<VecDeque<Result<Task<Context>, Join>>>,
        cvar : Condvar
    }

    impl<Context: 'static> WorkQueue<Context> {
        fn new() -> Self {
            WorkQueue { queue: Mutex::new(VecDeque::new()), cvar: Condvar::new() }
        }

        fn poll(&self) -> Result<Task<Context>, Join> {
            let mut guard = self.queue.lock().unwrap();

            while guard.is_empty() {
                guard = self.cvar.wait(guard).unwrap();
            }

            let head = guard.pop_front().unwrap();
            if let Err(_) = &head {
                guard.push_front(Err(Join{}));
            }
            head
        }

        fn push(&self, task: Task<Context>) {
            let mut guard = self.queue.lock().unwrap();

            if let Some(Err(_)) = guard.front() {
                panic!("Tried to add to the queue while joining")
            }

            guard.push_back(Ok(task));
            self.cvar.notify_one();
        }

        fn kill(&self) {
            let mut guard = self.queue.lock().unwrap();
            guard.push_back(Err(Join{}));
            self.cvar.notify_all();
        }
    }

    pub struct ThreadPool<Context: 'static + Send> {
        nodes: Vec<Arc<NodeInfo<Context>>>,
        queue: Arc<WorkQueue<Context>>,
        threads: Vec<JoinHandle<()>>
    }

    fn worker<Context: 'static>(node: &NodeInfo<Context>, cpu_id: c_uint, queue: &WorkQueue<Context>) {
        use self::nix::unistd::gettid;
        use crate::numa_threadpool::numa::nix::libc::pid_t;

        let tid = pid_t::from(gettid());
        unsafe {
            let bitmask = numa_allocate_cpumask();
            numa_bitmask_clearall(bitmask);
            numa_bitmask_setbit(bitmask, cpu_id);
            let rv = numa_sched_setaffinity(tid as c_uint, bitmask);
            if 0 != rv {
                let mesg = CString::new("setaffinity").unwrap();
                numa_error(mesg.as_ptr());
            }
            numa_bitmask_free(bitmask);
            if 0 != rv {
                panic!();
            }
        }

        loop {
            match queue.poll() {
                Err(_) => return,
                Ok(task) => task(&node.context)
            }
        }
    }

    fn build_nodes<Context>(context_ctor: impl Fn(u32)->Context) -> Vec<Arc<NodeInfo<Context>>> {
        let mut nodes = Vec::new();

        let max = unsafe { numa_num_possible_nodes() };
        for i in 0..max {
            if unsafe { numa_bitmask_isbitset(numa_all_nodes_ptr, i) } {
                let mut info = NodeInfo { node_id: (i as c_uint), cpuset: Vec::new(), context: context_ctor(i) };

                let bitmask = unsafe { numa_allocate_cpumask() };
                if unsafe { numa_node_to_cpus(i, bitmask) } != 0 {
                    panic!("numa_node_to_cpus");
                }
                let size = unsafe { numa_bitmask_nbytes(bitmask) } * 8;

                for cpu in 0..size {
                    if unsafe {numa_bitmask_isbitset(bitmask, cpu)} {
                        info.cpuset.push(cpu as c_uint);
                    }
                }

                unsafe {numa_bitmask_free(bitmask)};

                nodes.push(Arc::new(info));
            }
        }

        nodes
    }

    impl<Context: 'static + Send + Sync> ThreadPool<Context> {
        pub fn new(context_ctor: impl Fn(u32)->Context) -> Self {
            if unsafe { numa_available() } == -1 {
                panic!("NUMA library unavailable");
            }

            let nodes = build_nodes(context_ctor);
            let queue = Arc::new(WorkQueue::new());
            let mut threads = Vec::new();

            for node in nodes.iter() {
                for cpu in node.cpuset.iter() {
                    let node = node.clone();
                    let cpu2 = cpu.clone();
                    let queue = queue.clone();

                    threads.push(thread::spawn(move ||
                        worker(&node, cpu2, &queue)
                    ));
                }
            }

            ThreadPool { nodes, queue, threads }
        }

        pub fn execute(&self, task: impl Fn(&Context)->() + Send + 'static) {
            self.queue.push(Box::new(task));
        }

        pub fn join(self) -> Vec<(u32, Context)> {
            self.queue.kill();
            for thread in self.threads {
                thread.join().unwrap();
            }

            return self.nodes.into_iter().map(|arc| {
                let info = Arc::try_unwrap(arc).unwrap_or_else(
                    move|_| panic!("Threads didn't exit somehow")
                );

                (info.node_id, info.context)
            }).collect();
        }

    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn test() {
        ThreadPool::new(|_| ());
    }
}