use hakille_core::{HakilleError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

pub const SYS_READ: usize = 3;
pub const SYS_WRITE: usize = 4;
pub const SYS_OPEN: usize = 5;
pub const SYS_CLOSE: usize = 6;
pub const SYS_FPATH: usize = 9;
pub const SYS_DUP: usize = 10;
pub const SYS_FEVENT: usize = 14;
pub const SYS_FSTAT: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct RedoxPacket {
    pub id: u64,
    pub op: usize,
    pub a: usize,
    pub b: usize,
    pub c: usize,
    pub d: usize,
}

impl RedoxPacket {
    pub fn new(id: u64, op: usize, a: usize, b: usize, c: usize, d: usize) -> Self {
        Self { id, op, a, b, c, d }
    }
}

pub trait SchemeHandler: Send + Sync {
    fn open(&self, path: &str, flags: usize) -> Result<usize>;
    fn read(&self, handle: usize, buf: &mut [u8]) -> Result<usize>;
    fn write(&self, handle: usize, buf: &[u8]) -> Result<usize>;
    fn close(&self, handle: usize) -> Result<()>;
    fn fpath(&self, handle: usize) -> Result<String>;
}

/// Sovereign inter-agent zero-copy communication channel scheme `chan:`
pub struct ChanScheme {
    channels: Mutex<HashMap<String, Vec<Vec<u8>>>>,
    handles: Mutex<HashMap<usize, String>>,
    next_handle: Mutex<usize>,
}

impl ChanScheme {
    pub fn new() -> Self {
        Self {
            channels: Mutex::new(HashMap::new()),
            handles: Mutex::new(HashMap::new()),
            next_handle: Mutex::new(100),
        }
    }
}

impl Default for ChanScheme {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemeHandler for ChanScheme {
    fn open(&self, path: &str, _flags: usize) -> Result<usize> {
        let mut next = self.next_handle.lock().unwrap();
        let handle = *next;
        *next += 1;

        let mut chs = self.channels.lock().unwrap();
        chs.entry(path.to_string()).or_default();

        let mut hdls = self.handles.lock().unwrap();
        hdls.insert(handle, path.to_string());
        Ok(handle)
    }

    fn read(&self, handle: usize, buf: &mut [u8]) -> Result<usize> {
        let hdls = self.handles.lock().unwrap();
        let path = hdls
            .get(&handle)
            .ok_or_else(|| HakilleError::SubstrateError("Invalid handle".to_string()))?;
        let mut chs = self.channels.lock().unwrap();
        if let Some(queue) = chs.get_mut(path)
            && !queue.is_empty()
        {
            let msg = queue.remove(0);
            let len = msg.len().min(buf.len());
            buf[..len].copy_from_slice(&msg[..len]);
            return Ok(len);
        }
        Ok(0)
    }

    fn write(&self, handle: usize, buf: &[u8]) -> Result<usize> {
        let hdls = self.handles.lock().unwrap();
        let path = hdls
            .get(&handle)
            .ok_or_else(|| HakilleError::SubstrateError("Invalid handle".to_string()))?;
        let mut chs = self.channels.lock().unwrap();
        let queue = chs.entry(path.clone()).or_default();
        queue.push(buf.to_vec());
        Ok(buf.len())
    }

    fn close(&self, handle: usize) -> Result<()> {
        let mut hdls = self.handles.lock().unwrap();
        hdls.remove(&handle);
        Ok(())
    }

    fn fpath(&self, handle: usize) -> Result<String> {
        let hdls = self.handles.lock().unwrap();
        hdls.get(&handle)
            .map(|p| format!("chan:{}", p))
            .ok_or_else(|| HakilleError::SubstrateError("Invalid handle".to_string()))
    }
}

/// Surface framebuffer : (largeur, hauteur, pixels BGRA).
type OrbitalSurface = (u32, u32, Vec<u8>);

/// Redox Orbital window manager scheme `orbital:`
pub struct OrbitalScheme {
    windows: Mutex<HashMap<usize, OrbitalSurface>>, // handle -> surface
    next_handle: Mutex<usize>,
}

impl OrbitalScheme {
    pub fn new() -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            next_handle: Mutex::new(200),
        }
    }

    pub fn get_surface(&self, handle: usize) -> Option<(u32, u32, Vec<u8>)> {
        self.windows.lock().unwrap().get(&handle).cloned()
    }
}

impl Default for OrbitalScheme {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemeHandler for OrbitalScheme {
    fn open(&self, _path: &str, _flags: usize) -> Result<usize> {
        let mut next = self.next_handle.lock().unwrap();
        let handle = *next;
        *next += 1;

        // Default surface dimensions 1920x1080
        let (w, h) = (1920, 1080);
        let fb = vec![0u8; (w * h * 4) as usize];

        let mut wins = self.windows.lock().unwrap();
        wins.insert(handle, (w, h, fb));
        Ok(handle)
    }

    fn read(&self, handle: usize, buf: &mut [u8]) -> Result<usize> {
        let wins = self.windows.lock().unwrap();
        if let Some((_, _, fb)) = wins.get(&handle) {
            let len = fb.len().min(buf.len());
            buf[..len].copy_from_slice(&fb[..len]);
            return Ok(len);
        }
        Ok(0)
    }

    fn write(&self, handle: usize, buf: &[u8]) -> Result<usize> {
        let mut wins = self.windows.lock().unwrap();
        if let Some((_, _, fb)) = wins.get_mut(&handle) {
            let len = fb.len().min(buf.len());
            fb[..len].copy_from_slice(&buf[..len]);
            return Ok(len);
        }
        Ok(0)
    }

    fn close(&self, handle: usize) -> Result<()> {
        let mut wins = self.windows.lock().unwrap();
        wins.remove(&handle);
        Ok(())
    }

    fn fpath(&self, handle: usize) -> Result<String> {
        Ok(format!("orbital:window-{}", handle))
    }
}

/// Redox system telemetry scheme `sys:`
pub struct SysScheme;

impl SchemeHandler for SysScheme {
    fn open(&self, path: &str, _flags: usize) -> Result<usize> {
        match path {
            "memory" => Ok(301),
            "cpu" => Ok(302),
            "power" => Ok(303),
            "context" => Ok(304),
            _ => Err(HakilleError::SubstrateError(format!(
                "Unknown sys: path {}",
                path
            ))),
        }
    }

    fn read(&self, handle: usize, buf: &mut [u8]) -> Result<usize> {
        let content = match handle {
            301 => "MemTotal: 67108864 kB\nMemFree: 50331648 kB\n",
            302 => "Cores: 16\nArch: x86_64-sovereign\n",
            303 => "ac: 1\nbat_pct: 100\ncharging: 1\n",
            304 => "HakilleKernel: running\nOrbital: active\n",
            _ => "",
        };
        let bytes = content.as_bytes();
        let len = bytes.len().min(buf.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }

    fn write(&self, _handle: usize, _buf: &[u8]) -> Result<usize> {
        Ok(0) // Read-only system scheme
    }

    fn close(&self, _handle: usize) -> Result<()> {
        Ok(())
    }

    fn fpath(&self, handle: usize) -> Result<String> {
        let name = match handle {
            301 => "memory",
            302 => "cpu",
            303 => "power",
            304 => "context",
            _ => "unknown",
        };
        Ok(format!("sys:{}", name))
    }
}

/// Redox Sound streaming scheme `audio:`
pub struct AudioScheme {
    output_buffer: Mutex<Vec<u8>>,
}

impl AudioScheme {
    pub fn new() -> Self {
        Self {
            output_buffer: Mutex::new(Vec::new()),
        }
    }
}

impl Default for AudioScheme {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemeHandler for AudioScheme {
    fn open(&self, _path: &str, _flags: usize) -> Result<usize> {
        Ok(401)
    }

    fn read(&self, _handle: usize, _buf: &mut [u8]) -> Result<usize> {
        Ok(0)
    }

    fn write(&self, _handle: usize, buf: &[u8]) -> Result<usize> {
        let mut out = self.output_buffer.lock().unwrap();
        out.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn close(&self, _handle: usize) -> Result<()> {
        Ok(())
    }

    fn fpath(&self, _handle: usize) -> Result<String> {
        Ok("audio:output".to_string())
    }
}

/// RedoxFS Block and file access scheme `disk:`
pub struct DiskScheme {
    storage: Mutex<HashMap<String, Vec<u8>>>,
    handles: Mutex<HashMap<usize, String>>,
    next_handle: Mutex<usize>,
}

impl DiskScheme {
    pub fn new() -> Self {
        Self {
            storage: Mutex::new(HashMap::new()),
            handles: Mutex::new(HashMap::new()),
            next_handle: Mutex::new(500),
        }
    }

    /// Remove a stored object by path. Returns `true` if an entry existed.
    pub fn remove(&self, path: &str) -> bool {
        self.storage.lock().unwrap().remove(path).is_some()
    }

    /// List immediate child names under `prefix` ("" = root of the disk scheme).
    pub fn list_keys(&self, prefix: &str) -> Vec<String> {
        let norm = prefix.trim_matches('/');
        let store = self.storage.lock().unwrap();
        let mut out = Vec::new();
        for key in store.keys() {
            let rest = if norm.is_empty() {
                key.as_str()
            } else {
                match key.strip_prefix(norm) {
                    Some(r) if r.starts_with('/') => &r[1..],
                    _ => continue,
                }
            };
            if rest.is_empty() {
                continue;
            }
            let child = rest.split('/').next().unwrap_or(rest).to_string();
            if !child.is_empty() && !out.contains(&child) {
                out.push(child);
            }
        }
        out.sort();
        out
    }
}

impl Default for DiskScheme {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemeHandler for DiskScheme {
    fn open(&self, path: &str, _flags: usize) -> Result<usize> {
        let mut next = self.next_handle.lock().unwrap();
        let handle = *next;
        *next += 1;

        let mut hdls = self.handles.lock().unwrap();
        hdls.insert(handle, path.to_string());
        Ok(handle)
    }

    fn read(&self, handle: usize, buf: &mut [u8]) -> Result<usize> {
        let hdls = self.handles.lock().unwrap();
        let path = hdls
            .get(&handle)
            .ok_or_else(|| HakilleError::SubstrateError("Invalid handle".to_string()))?;
        let store = self.storage.lock().unwrap();
        if let Some(data) = store.get(path) {
            let len = data.len().min(buf.len());
            buf[..len].copy_from_slice(&data[..len]);
            return Ok(len);
        }
        Ok(0)
    }

    fn write(&self, handle: usize, buf: &[u8]) -> Result<usize> {
        let hdls = self.handles.lock().unwrap();
        let path = hdls
            .get(&handle)
            .ok_or_else(|| HakilleError::SubstrateError("Invalid handle".to_string()))?;
        let mut store = self.storage.lock().unwrap();
        store.insert(path.clone(), buf.to_vec());
        Ok(buf.len())
    }

    fn close(&self, handle: usize) -> Result<()> {
        let mut hdls = self.handles.lock().unwrap();
        hdls.remove(&handle);
        Ok(())
    }

    fn fpath(&self, handle: usize) -> Result<String> {
        let hdls = self.handles.lock().unwrap();
        hdls.get(&handle)
            .map(|p| format!("disk:{}", p))
            .ok_or_else(|| HakilleError::SubstrateError("Invalid handle".to_string()))
    }
}

/// Redox Scheme Router multiplexing packets to registered scheme handlers
pub struct RedoxSchemeHandler {
    chan: ChanScheme,
    orbital: OrbitalScheme,
    sys: SysScheme,
    audio: AudioScheme,
    disk: DiskScheme,
}

impl RedoxSchemeHandler {
    pub fn new() -> Self {
        Self {
            chan: ChanScheme::new(),
            orbital: OrbitalScheme::new(),
            sys: SysScheme,
            audio: AudioScheme::new(),
            disk: DiskScheme::new(),
        }
    }

    pub fn is_scheme_supported(&self, scheme: &str) -> bool {
        matches!(
            scheme,
            "chan:" | "orbital:" | "disk:" | "sys:" | "audio:" | "hakille:" | "memory:" | "agent:"
        )
    }

    pub fn dispatch_open(&self, url: &str, flags: usize) -> Result<usize> {
        if let Some(path) = url.strip_prefix("chan:") {
            self.chan.open(path, flags)
        } else if let Some(path) = url.strip_prefix("orbital:") {
            self.orbital.open(path, flags)
        } else if let Some(path) = url.strip_prefix("sys:") {
            self.sys.open(path, flags)
        } else if let Some(path) = url.strip_prefix("audio:") {
            self.audio.open(path, flags)
        } else if let Some(path) = url.strip_prefix("disk:") {
            self.disk.open(path, flags)
        } else if let Some(path) = url.strip_prefix("hakille:") {
            self.chan.open(path, flags)
        } else if let Some(path) = url.strip_prefix("memory:") {
            self.disk.open(path, flags)
        } else if let Some(path) = url.strip_prefix("agent:") {
            self.chan.open(path, flags)
        } else {
            Err(HakilleError::SubstrateError(format!(
                "Unsupported scheme in URL: {}",
                url
            )))
        }
    }

    pub fn dispatch_read(&self, handle: usize, buf: &mut [u8]) -> Result<usize> {
        if (100..200).contains(&handle) {
            self.chan.read(handle, buf)
        } else if (200..300).contains(&handle) {
            self.orbital.read(handle, buf)
        } else if (300..400).contains(&handle) {
            self.sys.read(handle, buf)
        } else if (400..500).contains(&handle) {
            self.audio.read(handle, buf)
        } else if handle >= 500 {
            self.disk.read(handle, buf)
        } else {
            Err(HakilleError::SubstrateError(format!(
                "Invalid Redox handle {}",
                handle
            )))
        }
    }

    pub fn dispatch_write(&self, handle: usize, buf: &[u8]) -> Result<usize> {
        if (100..200).contains(&handle) {
            self.chan.write(handle, buf)
        } else if (200..300).contains(&handle) {
            self.orbital.write(handle, buf)
        } else if (300..400).contains(&handle) {
            self.sys.write(handle, buf)
        } else if (400..500).contains(&handle) {
            self.audio.write(handle, buf)
        } else if handle >= 500 {
            self.disk.write(handle, buf)
        } else {
            Err(HakilleError::SubstrateError(format!(
                "Invalid Redox handle {}",
                handle
            )))
        }
    }

    pub fn dispatch_close(&self, handle: usize) -> Result<()> {
        if (100..200).contains(&handle) {
            self.chan.close(handle)
        } else if (200..300).contains(&handle) {
            self.orbital.close(handle)
        } else if (300..400).contains(&handle) {
            self.sys.close(handle)
        } else if (400..500).contains(&handle) {
            self.audio.close(handle)
        } else if handle >= 500 {
            self.disk.close(handle)
        } else {
            Err(HakilleError::SubstrateError(format!(
                "Invalid Redox handle {}",
                handle
            )))
        }
    }

    pub fn dispatch_fpath(&self, handle: usize) -> Result<String> {
        if (100..200).contains(&handle) {
            self.chan.fpath(handle)
        } else if (200..300).contains(&handle) {
            self.orbital.fpath(handle)
        } else if (300..400).contains(&handle) {
            self.sys.fpath(handle)
        } else if (400..500).contains(&handle) {
            self.audio.fpath(handle)
        } else if handle >= 500 {
            self.disk.fpath(handle)
        } else {
            Err(HakilleError::SubstrateError(format!(
                "Invalid Redox handle {}",
                handle
            )))
        }
    }

    /// Handles a native Redox microkernel Packet IPC request
    pub fn handle_packet(&self, packet: &mut RedoxPacket, buffer_pool: &mut [u8]) -> Result<usize> {
        match packet.op {
            SYS_OPEN => {
                let path_start = packet.b;
                let path_len = packet.c;
                if path_start + path_len > buffer_pool.len() {
                    return Err(HakilleError::SubstrateError(
                        "Buffer bounds exceeded for SYS_OPEN".to_string(),
                    ));
                }
                let path_bytes = &buffer_pool[path_start..path_start + path_len];
                let path_str = std::str::from_utf8(path_bytes).map_err(|e| {
                    HakilleError::SubstrateError(format!("Invalid UTF-8 path: {}", e))
                })?;
                let handle = self.dispatch_open(path_str, packet.d)?;
                packet.a = handle;
                Ok(handle)
            }
            SYS_READ => {
                let handle = packet.a;
                let buf_start = packet.b;
                let buf_len = packet.c;
                if buf_start + buf_len > buffer_pool.len() {
                    return Err(HakilleError::SubstrateError(
                        "Buffer bounds exceeded for SYS_READ".to_string(),
                    ));
                }
                let buf = &mut buffer_pool[buf_start..buf_start + buf_len];
                let bytes_read = self.dispatch_read(handle, buf)?;
                packet.a = bytes_read;
                Ok(bytes_read)
            }
            SYS_WRITE => {
                let handle = packet.a;
                let buf_start = packet.b;
                let buf_len = packet.c;
                if buf_start + buf_len > buffer_pool.len() {
                    return Err(HakilleError::SubstrateError(
                        "Buffer bounds exceeded for SYS_WRITE".to_string(),
                    ));
                }
                let buf = &buffer_pool[buf_start..buf_start + buf_len];
                let bytes_written = self.dispatch_write(handle, buf)?;
                packet.a = bytes_written;
                Ok(bytes_written)
            }
            SYS_CLOSE => {
                let handle = packet.a;
                self.dispatch_close(handle)?;
                packet.a = 0;
                Ok(0)
            }
            SYS_FPATH => {
                let handle = packet.a;
                let path = self.dispatch_fpath(handle)?;
                let path_bytes = path.as_bytes();
                let buf_start = packet.b;
                let max_len = packet.c;
                let copy_len = path_bytes.len().min(max_len);
                if buf_start + copy_len > buffer_pool.len() {
                    return Err(HakilleError::SubstrateError(
                        "Buffer bounds exceeded for SYS_FPATH".to_string(),
                    ));
                }
                buffer_pool[buf_start..buf_start + copy_len]
                    .copy_from_slice(&path_bytes[..copy_len]);
                packet.a = copy_len;
                Ok(copy_len)
            }
            _ => Err(HakilleError::SubstrateError(format!(
                "Unsupported Redox syscall op: {}",
                packet.op
            ))),
        }
    }

    pub fn orbital(&self) -> &OrbitalScheme {
        &self.orbital
    }

    pub fn chan(&self) -> &ChanScheme {
        &self.chan
    }

    pub fn sys(&self) -> &SysScheme {
        &self.sys
    }

    pub fn audio(&self) -> &AudioScheme {
        &self.audio
    }

    pub fn disk(&self) -> &DiskScheme {
        &self.disk
    }
}

impl Default for RedoxSchemeHandler {
    fn default() -> Self {
        Self::new()
    }
}
