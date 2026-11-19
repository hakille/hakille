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

/// Structure ABI Linux `struct sock_fprog`, liée à la durée de vie `'a` de son
/// tampon d'instructions pour empêcher tout pointeur pendant.
#[derive(Debug)]
#[repr(C)]
pub struct SockFprog<'a> {
    pub len: u16,
    pub filter: *const SockFilter,
    _marker: std::marker::PhantomData<&'a [SockFilter]>,
}

impl<'a> SockFprog<'a> {
    pub fn try_new(filters: &'a [SockFilter]) -> Result<Self, String> {
        let len = u16::try_from(filters.len())
            .map_err(|_| format!("programme BPF trop grand ({} > 65535)", filters.len()))?;
        Ok(Self {
            len,
            filter: filters.as_ptr(),
            _marker: std::marker::PhantomData,
        })
    }
}

/// Filtre Seccomp-BPF compilé propriétaire de ses instructions.
#[derive(Debug, Clone)]
pub struct CompiledSeccompFilter {
    instructions: Vec<SockFilter>,
}

impl CompiledSeccompFilter {
    pub fn instructions(&self) -> &[SockFilter] {
        &self.instructions
    }

    pub fn as_sock_fprog(&self) -> Result<SockFprog<'_>, String> {
        SockFprog::try_new(&self.instructions)
    }
}

impl BpfInstruction {
    pub fn stmt(code: u16, k: u32) -> Self {
        Self {
            code,
            jt: 0,
            jf: 0,
            k,
        }
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
        // Sur aarch64 (asm-generic), `poll` et `epoll_wait` n'existent pas :
        // on autorise `ppoll` (x86_64=271, aarch64=73) et `epoll_pwait` (281, 22).
        profile.allow_syscall("ppoll", 271, 73);
        profile.allow_syscall("epoll_create1", 291, 20);
        profile.allow_syscall("epoll_ctl", 233, 21);
        profile.allow_syscall("epoll_pwait", 281, 22);
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

    /// Compile le profil en bytecode BPF sans troncature à 255 règles.
    /// Refuse explicitement si le programme dépasse la limite noyau `BPF_MAXINSNS` (4096).
    pub fn try_compile_bpf(&self, arch: SyscallArch) -> Result<Vec<BpfInstruction>, String> {
        let total_insns = 4usize
            .checked_add(self.rules.len().saturating_mul(2))
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| "débordement du nombre d'instructions BPF".to_string())?;
        if total_insns > 4096 {
            return Err(format!(
                "profil seccomp trop volumineux ({total_insns} instructions > 4096 BPF_MAXINSNS)"
            ));
        }

        let mut instructions = Vec::with_capacity(total_insns);
        // 1. Vérification de l'architecture
        instructions.push(BpfInstruction::stmt(
            BPF_LD | BPF_W | BPF_ABS,
            SECCOMP_DATA_ARCH_OFFSET,
        ));
        instructions.push(BpfInstruction::jump(
            BPF_JMP | BPF_JEQ | BPF_K,
            arch.audit_arch(),
            1,
            0,
        ));
        instructions.push(BpfInstruction::stmt(
            BPF_RET | BPF_K,
            SECCOMP_RET_KILL_PROCESS,
        ));
        // 2. Chargement du numéro d'appel système
        instructions.push(BpfInstruction::stmt(
            BPF_LD | BPF_W | BPF_ABS,
            SECCOMP_DATA_NR_OFFSET,
        ));

        // 3. Paires (JEQ nr, jt=0, jf=1) + (RET action) : aucune limite à 255 règles.
        for rule in &self.rules {
            let nr = rule.nr_for(arch);
            let action_ret = Self::action_code(&rule.action);
            instructions.push(BpfInstruction::jump(BPF_JMP | BPF_JEQ | BPF_K, nr, 0, 1));
            instructions.push(BpfInstruction::stmt(BPF_RET | BPF_K, action_ret));
        }

        // 4. Action par défaut si aucune règle ne correspond
        let default_ret = Self::action_code(&self.default_action);
        instructions.push(BpfInstruction::stmt(BPF_RET | BPF_K, default_ret));

        Ok(instructions)
    }

    /// Compiles this SeccompProfile into executable BPF bytecode instructions.
    /// En cas de profil trop volumineux (> 4096 instructions), bascule fail-closed
    /// sur un programme `SECCOMP_RET_KILL_PROCESS` unique.
    pub fn compile_bpf(&self, arch: SyscallArch) -> Vec<BpfInstruction> {
        self.try_compile_bpf(arch).unwrap_or_else(|_| {
            vec![BpfInstruction::stmt(
                BPF_RET | BPF_K,
                SECCOMP_RET_KILL_PROCESS,
            )]
        })
    }

    /// Compile le profil dans un conteneur propriétaire dont le `SockFprog<'_>`
    /// reste garanti valide tant que le conteneur vit.
    pub fn compile_filter(&self, arch: SyscallArch) -> Result<CompiledSeccompFilter, String> {
        let instructions = self.try_compile_bpf(arch)?;
        Ok(CompiledSeccompFilter { instructions })
    }

    /// Simulates classic BPF bytecode execution against a system call and architecture
    pub fn simulate_bpf(
        instructions: &[BpfInstruction],
        arch: SyscallArch,
        syscall_nr: u32,
    ) -> u32 {
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
