use serde::{Deserialize, Serialize};

pub const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
pub const AUDIT_ARCH_AARCH64: u32 = 0xc000_00b7;

pub const BPF_LD: u16 = 0x00;
pub const BPF_W: u16 = 0x00;
pub const BPF_ABS: u16 = 0x20;
pub const BPF_JMP: u16 = 0x05;
pub const BPF_JEQ: u16 = 0x10;
pub const BPF_K: u16 = 0x00;
pub const BPF_RET: u16 = 0x06;

pub const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
pub const SECCOMP_RET_KILL_THREAD: u32 = 0x0000_0000;
pub const SECCOMP_RET_TRAP: u32 = 0x0003_0000;
pub const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
pub const SECCOMP_RET_LOG: u32 = 0x7ffc_0000;
pub const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;

pub const SECCOMP_DATA_NR_OFFSET: u32 = 0;
pub const SECCOMP_DATA_ARCH_OFFSET: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyscallArch {
    X86_64,
    Aarch64,
}

impl SyscallArch {
    pub fn audit_arch(&self) -> u32 {
        match self {
            Self::X86_64 => AUDIT_ARCH_X86_64,
            Self::Aarch64 => AUDIT_ARCH_AARCH64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct BpfInstruction {
    pub code: u16,
    pub jt: u8,
    pub jf: u8,
    pub k: u32,
}

pub type SockFilter = BpfInstruction;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SockFprog {
    pub len: u16,
    pub filter: *const SockFilter,
}

impl SockFprog {
    pub fn new(filters: &[SockFilter]) -> Self {
        Self {
            len: filters.len() as u16,
            filter: filters.as_ptr(),
        }
    }
}

impl BpfInstruction {
    pub fn stmt(code: u16, k: u32) -> Self {
        Self { code, jt: 0, jf: 0, k }
    }

    pub fn jump(code: u16, k: u32, jt: u8, jf: u8) -> Self {
        Self { code, jt, jf, k }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallRule {
    pub name: String,
    pub x86_64_nr: u32,
    pub aarch64_nr: u32,
    pub action: String, // "allow", "errno", "kill", "trap"
}

impl SyscallRule {
    pub fn new(name: &str, x86_64_nr: u32, aarch64_nr: u32, action: &str) -> Self {
        Self {
            name: name.to_string(),
            x86_64_nr,
            aarch64_nr,
            action: action.to_string(),
        }
    }

    pub fn nr_for(&self, arch: SyscallArch) -> u32 {
        match arch {
            SyscallArch::X86_64 => self.x86_64_nr,
            SyscallArch::Aarch64 => self.aarch64_nr,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompProfile {
    pub default_action: String, // "kill", "trap", "errno", "allow"
    pub allowed_syscalls: Vec<String>,
    pub isolated_namespaces: Vec<String>,
    pub rules: Vec<SyscallRule>,
}

impl SeccompProfile {
    pub fn new(default_action: &str) -> Self {
        Self {
            default_action: default_action.to_string(),
            allowed_syscalls: Vec::new(),
            isolated_namespaces: vec![
                "pid".to_string(),
                "net".to_string(),
                "ipc".to_string(),
                "uts".to_string(),
                "mount".to_string(),
            ],
            rules: Vec::new(),
        }
    }

    pub fn allow_syscall(&mut self, name: &str, x86_64: u32, aarch64: u32) {
        self.allowed_syscalls.push(name.to_string());
        self.rules.push(SyscallRule {
            name: name.to_string(),
            x86_64_nr: x86_64,
            aarch64_nr: aarch64,
            action: "allow".to_string(),
        });
    }

    pub fn wasm_sandbox() -> Self {
        let mut profile = Self::new("errno");
        // Memory & allocations
        profile.allow_syscall("mmap", 9, 222);
        profile.allow_syscall("munmap", 11, 215);
        profile.allow_syscall("mprotect", 10, 226);
        profile.allow_syscall("brk", 12, 214);
        // Concurrency & futex
        profile.allow_syscall("futex", 202, 98);
        profile.allow_syscall("sched_yield", 24, 124);
        // Lifecycle & clocks
        profile.allow_syscall("exit_group", 231, 94);
        profile.allow_syscall("clock_gettime", 228, 113);
        // Basic I/O for stdout/stderr pipes
        profile.allow_syscall("read", 0, 63);
        profile.allow_syscall("write", 1, 64);
        profile.allow_syscall("close", 3, 57);
        profile
    }

    pub fn agent_minimal() -> Self {
        let mut profile = Self::new("kill");
        profile.allow_syscall("read", 0, 63);
        profile.allow_syscall("write", 1, 64);
        profile.allow_syscall("close", 3, 57);
        profile.allow_syscall("futex", 202, 98);
        profile.allow_syscall("exit_group", 231, 94);
        profile.allow_syscall("clock_gettime", 228, 113);
        profile
    }

    pub fn agent_network() -> Self {
        let mut profile = Self::wasm_sandbox();
        profile.allow_syscall("socket", 41, 198);
        profile.allow_syscall("connect", 42, 203);
        profile.allow_syscall("sendto", 44, 206);
        profile.allow_syscall("recvfrom", 45, 207);
        profile.allow_syscall("poll", 7, 73);
        profile.allow_syscall("epoll_create1", 291, 20);
        profile.allow_syscall("epoll_ctl", 233, 21);
        profile.allow_syscall("epoll_wait", 232, 22);
        profile
    }

    pub fn strict_lockdown() -> Self {
        let mut profile = Self::new("kill");
        profile.allow_syscall("exit_group", 231, 94);
        profile.allow_syscall("futex", 202, 98);
        profile
    }

    pub fn action_code(action_str: &str) -> u32 {
        match action_str.to_ascii_lowercase().as_str() {
            "allow" => SECCOMP_RET_ALLOW,
            "errno" => SECCOMP_RET_ERRNO | 1, // EPERM
            "trap" => SECCOMP_RET_TRAP,
            "kill" | "kill_process" => SECCOMP_RET_KILL_PROCESS,
            "log" => SECCOMP_RET_LOG,
            _ => SECCOMP_RET_KILL_PROCESS,
        }
    }

    /// Compiles this SeccompProfile into executable BPF bytecode instructions
    pub fn compile_bpf(&self, arch: SyscallArch) -> Vec<BpfInstruction> {
        let mut instructions = Vec::new();

        // 1. Check architecture: [BPF_LD | BPF_W | BPF_ABS, arch_offset]
        instructions.push(BpfInstruction::stmt(BPF_LD | BPF_W | BPF_ABS, SECCOMP_DATA_ARCH_OFFSET));
        // If arch == expected, jump 1 forward (skip kill), else jump 0 (execute kill)
        instructions.push(BpfInstruction::jump(BPF_JMP | BPF_JEQ | BPF_K, arch.audit_arch(), 1, 0));
        // Architecture mismatch -> kill immediately
        instructions.push(BpfInstruction::stmt(BPF_RET | BPF_K, SECCOMP_RET_KILL_PROCESS));

        // 2. Load system call number: [BPF_LD | BPF_W | BPF_ABS, nr_offset]
        instructions.push(BpfInstruction::stmt(BPF_LD | BPF_W | BPF_ABS, SECCOMP_DATA_NR_OFFSET));

        // 3. For each rule: if nr == rule_nr, jump to rule action
        let num_rules = self.rules.len().min(255);
        for (i, rule) in self.rules.iter().take(num_rules).enumerate() {
            let nr = rule.nr_for(arch);
            let remaining_checks = (num_rules - 1 - i) as u8;
            let jt = remaining_checks + 1 + (i as u8); // jumps to the corresponding return instruction
            let jf = 0; // fall through to next check
            instructions.push(BpfInstruction::jump(BPF_JMP | BPF_JEQ | BPF_K, nr, jt, jf));
        }

        // 4. Default action if no rule matched
        let default_ret = Self::action_code(&self.default_action);
        instructions.push(BpfInstruction::stmt(BPF_RET | BPF_K, default_ret));

        // 5. Actions for each matched rule
        for rule in self.rules.iter().take(num_rules) {
            let action_ret = Self::action_code(&rule.action);
            instructions.push(BpfInstruction::stmt(BPF_RET | BPF_K, action_ret));
        }

        instructions
    }

    /// Compiles this SeccompProfile into a native SockFprog structure for seccomp(2) / prctl
    pub fn compile_sock_fprog(&self, arch: SyscallArch) -> (Vec<SockFilter>, SockFprog) {
        let instructions = self.compile_bpf(arch);
        let fprog = SockFprog::new(&instructions);
        (instructions, fprog)
    }

    /// Simulates classic BPF bytecode execution against a system call and architecture
    pub fn simulate_bpf(instructions: &[BpfInstruction], arch: SyscallArch, syscall_nr: u32) -> u32 {
        let mut pc = 0;
        let mut accumulator: u32 = 0;

        while pc < instructions.len() {
            let inst = &instructions[pc];
            match inst.code {
                c if c == (BPF_LD | BPF_W | BPF_ABS) => {
                    accumulator = match inst.k {
                        SECCOMP_DATA_ARCH_OFFSET => arch.audit_arch(),
                        SECCOMP_DATA_NR_OFFSET => syscall_nr,
                        _ => 0,
                    };
                    pc += 1;
                }
                c if c == (BPF_JMP | BPF_JEQ | BPF_K) => {
                    if accumulator == inst.k {
                        pc += 1 + (inst.jt as usize);
                    } else {
                        pc += 1 + (inst.jf as usize);
                    }
                }
                c if c == (BPF_RET | BPF_K) => {
                    return inst.k;
                }
                _ => {
                    return SECCOMP_RET_KILL_PROCESS;
                }
            }
        }
        SECCOMP_RET_KILL_PROCESS
    }
}

impl Default for SeccompProfile {
    fn default() -> Self {
        let mut profile = Self::new("errno");
        profile.allow_syscall("read", 0, 63);
        profile.allow_syscall("write", 1, 64);
        profile.allow_syscall("close", 3, 57);
        profile.allow_syscall("fstat", 5, 80);
        profile.allow_syscall("mmap", 9, 222);
        profile.allow_syscall("munmap", 11, 215);
        profile.allow_syscall("brk", 12, 214);
        profile.allow_syscall("exit_group", 231, 94);
        profile.allow_syscall("futex", 202, 98);
        profile.allow_syscall("clock_gettime", 228, 113);
        profile
    }
}
