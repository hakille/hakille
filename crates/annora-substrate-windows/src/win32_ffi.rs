//! Native Win32 FFI bindings for Windows 10/11 substrate operations.
//! Zero external runtime dependencies; connects directly to system DLLs.

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::mem::size_of;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SYSTEM_POWER_STATUS {
    pub ACLineStatus: u8,
    pub BatteryFlag: u8,
    pub BatteryLifePercent: u8,
    pub SystemStatusFlag: u8,
    pub BatteryLifeTime: u32,
    pub BatteryFullLifeTime: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MEMORYSTATUSEX {
    pub dwLength: u32,
    pub dwMemoryLoad: u32,
    pub ullTotalPhys: u64,
    pub ullAvailPhys: u64,
    pub ullTotalPageFile: u64,
    pub ullAvailPageFile: u64,
    pub ullTotalVirtual: u64,
    pub ullAvailVirtual: u64,
    pub ullAvailExtendedVirtual: u64,
}

impl Default for MEMORYSTATUSEX {
    fn default() -> Self {
        Self {
            dwLength: size_of::<Self>() as u32,
            dwMemoryLoad: 0,
            ullTotalPhys: 0,
            ullAvailPhys: 0,
            ullTotalPageFile: 0,
            ullAvailPageFile: 0,
            ullTotalVirtual: 0,
            ullAvailVirtual: 0,
            ullAvailExtendedVirtual: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct BITMAPINFOHEADER {
    pub biSize: u32,
    pub biWidth: i32,
    pub biHeight: i32,
    pub biPlanes: u16,
    pub biBitCount: u16,
    pub biCompression: u32,
    pub biSizeImage: u32,
    pub biXPelsPerMeter: i32,
    pub biYPelsPerMeter: i32,
    pub biClrUsed: u32,
    pub biClrImportant: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RGBQUAD {
    pub rgbBlue: u8,
    pub rgbGreen: u8,
    pub rgbRed: u8,
    pub rgbReserved: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct BITMAPINFO {
    pub bmiHeader: BITMAPINFOHEADER,
    pub bmiColors: [RGBQUAD; 1],
}

pub const SM_CXSCREEN: i32 = 0;
pub const SM_CYSCREEN: i32 = 1;
pub const SRCCOPY: u32 = 0x00CC0020;
pub const DIB_RGB_COLORS: u32 = 0;
pub const BI_RGB: u32 = 0;

pub const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
pub const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
pub const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
pub const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
pub const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
pub const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;
pub const MOUSEEVENTF_WHEEL: u32 = 0x0800;

pub const KEYEVENTF_KEYUP: u32 = 0x0002;

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn GetSystemPowerStatus(lpSystemPowerStatus: *mut SYSTEM_POWER_STATUS) -> i32;
    pub fn GlobalMemoryStatusEx(lpBuffer: *mut MEMORYSTATUSEX) -> i32;
    pub fn GetLastError() -> u32;
    pub fn GetComputerNameW(lpBuffer: *mut u16, nSize: *mut u32) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    pub fn GetSystemMetrics(nIndex: i32) -> i32;
    pub fn GetDC(hWnd: isize) -> isize;
    pub fn ReleaseDC(hWnd: isize, hDC: isize) -> i32;
    pub fn SetCursorPos(X: i32, Y: i32) -> i32;
    pub fn mouse_event(dwFlags: u32, dx: u32, dy: u32, dwData: u32, dwExtraInfo: usize);
    pub fn keybd_event(bVk: u8, bScan: u8, dwFlags: u32, dwExtraInfo: usize);
}

#[link(name = "gdi32")]
unsafe extern "system" {
    pub fn CreateCompatibleDC(hDC: isize) -> isize;
    pub fn DeleteDC(hDC: isize) -> i32;
    pub fn CreateCompatibleBitmap(hDC: isize, nWidth: i32, nHeight: i32) -> isize;
    pub fn SelectObject(hDC: isize, hgdiobj: isize) -> isize;
    pub fn DeleteObject(hObject: isize) -> i32;
    pub fn BitBlt(
        hdcDest: isize,
        nXDest: i32,
        nYDest: i32,
        nWidth: i32,
        nHeight: i32,
        hdcSrc: isize,
        nXSrc: i32,
        nYSrc: i32,
        dwRop: u32,
    ) -> i32;
    pub fn GetDIBits(
        hdc: isize,
        hbm: isize,
        start: u32,
        cLines: u32,
        lpvBits: *mut u8,
        lpbmi: *mut BITMAPINFO,
        usage: u32,
    ) -> i32;
}
