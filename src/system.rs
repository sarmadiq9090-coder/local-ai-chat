//! Hardware summary (GPUs + VRAM, RAM, CPU) for model suggestions.
//!
//! Probed once (blocking: spawns nvidia-smi / PowerShell) and cached in `AppState`.
//! - NVIDIA VRAM: `nvidia-smi` (exact).
//! - Other adapters: names from WMI `Win32_VideoController`; VRAM from the display
//!   driver's registry value `HardwareInformation.qwMemorySize` when present
//!   (WMI `AdapterRAM` is a 32-bit field capped at 4 GB, so it is not used).
//! - RAM: `GlobalMemoryStatusEx`. CPU: registry `ProcessorNameString`.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Gpu {
    pub name: String,
    pub vram_gb: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SystemInfo {
    pub gpus: Vec<Gpu>,
    pub ram_gb: f64,
    pub cpu: Option<String>,
}

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// One decimal place, e.g. 8.0 / 63.9.
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Probe the machine. Blocking (spawns processes): call via `spawn_blocking`.
pub fn probe() -> SystemInfo {
    let nvidia = run(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ],
    )
    .map(|out| parse_nvidia_smi(&out))
    .unwrap_or_default();
    let others = run(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", PS_GPUS],
    )
    .map(|out| parse_ps_gpus(&out))
    .unwrap_or_default();
    SystemInfo {
        gpus: merge_gpus(nvidia, others),
        ram_gb: total_ram_gb(),
        cpu: cpu_name(),
    }
}

/// WMI adapter names, each joined with its registry VRAM in bytes: `name|bytes`.
const PS_GPUS: &str = "$m=@{}; Get-ItemProperty 'HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\0*' -EA 0 | % { $q=$_.'HardwareInformation.qwMemorySize'; if($q){$m[$_.DriverDesc]=$q} }; Get-CimInstance Win32_VideoController | % { \"$($_.Name)|$($m[$_.Name])\" }";

/// Run a command without flashing a console window; stdout on success.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `NVIDIA GeForce RTX 2070, 8192` (MiB) per line.
fn parse_nvidia_smi(out: &str) -> Vec<Gpu> {
    out.lines()
        .filter_map(|line| {
            let (name, mib) = line.rsplit_once(',')?;
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            let vram_gb = mib.trim().parse::<f64>().ok().map(|m| round1(m / 1024.0));
            Some(Gpu {
                name: name.to_string(),
                vram_gb,
            })
        })
        .collect()
}

/// `name|bytes` per line; bytes may be empty.
fn parse_ps_gpus(out: &str) -> Vec<Gpu> {
    out.lines()
        .filter_map(|line| {
            let (name, bytes) = line.trim().rsplit_once('|').unwrap_or((line.trim(), ""));
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            let vram_gb = bytes
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|b| *b > 0.0)
                .map(|b| round1(b / GIB));
            Some(Gpu {
                name: name.to_string(),
                vram_gb,
            })
        })
        .collect()
}

/// nvidia-smi entries win; WMI adds the adapters it doesn't already cover.
fn merge_gpus(mut nvidia: Vec<Gpu>, others: Vec<Gpu>) -> Vec<Gpu> {
    for g in others {
        if !nvidia.iter().any(|n| n.name.eq_ignore_ascii_case(&g.name)) {
            nvidia.push(g);
        }
    }
    nvidia
}

#[cfg(windows)]
fn total_ram_gb() -> f64 {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: zeroed POD struct with dwLength set, as the API requires.
    unsafe {
        let mut m: MEMORYSTATUSEX = std::mem::zeroed();
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut m) == 0 {
            return 0.0;
        }
        round1(m.ullTotalPhys as f64 / GIB)
    }
}

#[cfg(not(windows))]
fn total_ram_gb() -> f64 {
    0.0
}

#[cfg(windows)]
fn cpu_name() -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let key = wide(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0");
    let value = wide("ProcessorNameString");
    let mut buf = [0u16; 256];
    let mut len = std::mem::size_of_val(&buf) as u32;
    // SAFETY: buffers are valid for the sizes passed; the API NUL-terminates REG_SZ.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    let from_registry = (rc == 0)
        .then(|| {
            String::from_utf16_lossy(&buf[..(len as usize / 2)])
                .trim_end_matches('\0')
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty());
    from_registry.or_else(|| std::env::var("PROCESSOR_IDENTIFIER").ok())
}

#[cfg(not(windows))]
fn cpu_name() -> Option<String> {
    std::env::var("PROCESSOR_IDENTIFIER").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nvidia_smi() {
        let g = parse_nvidia_smi("NVIDIA GeForce RTX 2070, 8192\r\nNVIDIA RTX A6000, 49140\n\n");
        assert_eq!(
            g,
            vec![
                Gpu {
                    name: "NVIDIA GeForce RTX 2070".into(),
                    vram_gb: Some(8.0)
                },
                Gpu {
                    name: "NVIDIA RTX A6000".into(),
                    vram_gb: Some(48.0)
                },
            ]
        );
    }

    #[test]
    fn parses_powershell_and_merges() {
        let ps =
            parse_ps_gpus("Intel(R) UHD Graphics 630|\r\nNVIDIA GeForce RTX 2070|8589934592\r\n");
        assert_eq!(
            ps[0],
            Gpu {
                name: "Intel(R) UHD Graphics 630".into(),
                vram_gb: None
            }
        );
        assert_eq!(ps[1].vram_gb, Some(8.0));
        let nv = parse_nvidia_smi("NVIDIA GeForce RTX 2070, 8192");
        let all = merge_gpus(nv, ps);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "NVIDIA GeForce RTX 2070");
        assert_eq!(all[1].name, "Intel(R) UHD Graphics 630");
    }

    #[test]
    fn fallback_without_nvidia_smi() {
        let all = merge_gpus(vec![], parse_ps_gpus("AMD Radeon RX 7800 XT|17163091968"));
        assert_eq!(
            all,
            vec![Gpu {
                name: "AMD Radeon RX 7800 XT".into(),
                vram_gb: Some(16.0)
            }]
        );
    }

    #[test]
    fn probe_runs() {
        let s = probe();
        assert!(s.ram_gb > 0.0);
        eprintln!("{s:?}");
    }
}
