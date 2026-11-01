//! Known machine variants and their ROM dumps.
//!
//! The repository ships the dumps from the MAME 0.201 set in `roms/` (see
//! `roms/README.md` for source and attribution). Any directory works: files
//! are recognised by SHA-1 first and by file name second. Checksums are
//! those catalogued by MAME.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Lc80,
    Poly880,
}

#[derive(Debug)]
pub struct RomFile {
    pub name: &'static str,
    /// Load address in the CPU address space.
    pub offset: u16,
    pub size: usize,
    pub sha1: &'static str,
}

#[derive(Debug)]
pub struct Variant {
    pub id: &'static str,
    pub name: &'static str,
    pub family: Family,
    pub clock_hz: u32,
    pub ram_size: usize,
    pub files: &'static [RomFile],
}

pub static VARIANTS: &[Variant] = &[
    Variant {
        id: "lc80",
        name: "LC-80 (2x U505, 1 KB RAM)",
        family: Family::Lc80,
        clock_hz: 900_000,
        ram_size: 1024,
        files: &[
            RomFile {
                name: "bm075.d202",
                offset: 0x0000,
                size: 0x400,
                sha1: "044440b13e62addbc3f6a77369cfd16f99b39752",
            },
            RomFile {
                name: "bm076.d203",
                offset: 0x0800,
                size: 0x400,
                sha1: "3a6cbd0c57c38eadb7055dca4b396c348567d1d5",
            },
        ],
    },
    Variant {
        id: "lc80a",
        name: "LC-80 (2716 EPROM, 1 KB RAM)",
        family: Family::Lc80,
        clock_hz: 900_000,
        ram_size: 1024,
        files: &[RomFile {
            name: "lc80_2716.bin",
            offset: 0x0000,
            size: 0x800,
            sha1: "6fff953f0f1eee829fd774366313ab7e8053468c",
        }],
    },
    Variant {
        id: "lc80e",
        name: "LC-80 export (SC-80 chess, 4 KB RAM)",
        family: Family::Lc80,
        clock_hz: 1_650_000,
        ram_size: 4096,
        files: &[
            RomFile {
                name: "lc80e-0000-schach.rom",
                offset: 0x0000,
                size: 0x1000,
                sha1: "f2be3f2a9d3780d59657e49b3abeffb0fc13db89",
            },
            RomFile {
                name: "lc80e-1000-schach.rom",
                offset: 0x1000,
                size: 0x1000,
                sha1: "0ea019b0944736ae5b842bf9aa3537300f259b98",
            },
            RomFile {
                name: "lc80e-c000-schach.rom",
                offset: 0x4000,
                size: 0x1000,
                sha1: "2f7b3fd046c965185606253f6cd9372da289ca6f",
            },
        ],
    },
    Variant {
        id: "lc80_2",
        name: "LC-80.2 (hack by E. Buschendorf, 4 KB RAM)",
        family: Family::Lc80,
        clock_hz: 900_000,
        ram_size: 4096,
        files: &[RomFile {
            name: "lc80_2.bin",
            offset: 0x0000,
            size: 0x1000,
            sha1: "d9cddaf847831e4ab21854c0f895348b7fda20b8",
        }],
    },
    Variant {
        id: "poly880",
        name: "Poly-Computer 880",
        family: Family::Poly880,
        clock_hz: 921_600,
        ram_size: 1024,
        files: &[
            RomFile {
                name: "bm039.i5",
                offset: 0x0000,
                size: 0x400,
                sha1: "85bfe53d39d6690e79999a1e1240789497e72db0",
            },
            RomFile {
                name: "bm040.i6",
                offset: 0x1000,
                size: 0x400,
                sha1: "6ffa2f80b2c6f8ec9e22834f739c82f9754272b8",
            },
        ],
    },
    Variant {
        id: "poly880s",
        name: "Poly-Computer 880 (SC1 chess hack)",
        family: Family::Poly880,
        clock_hz: 921_600,
        ram_size: 1024,
        files: &[RomFile {
            name: "sc1.rom",
            offset: 0x0000,
            size: 0x1000,
            sha1: "01568911446eda9f05ec136df53da147b7c6f2bf",
        }],
    },
];

pub fn variant(id: &str) -> Option<&'static Variant> {
    VARIANTS.iter().find(|v| v.id.eq_ignore_ascii_case(id))
}

/// A 64 KB image of the ROM contents, placed at their load addresses.
#[derive(Clone)]
pub struct RomImage {
    pub data: Vec<u8>,
    /// Non-fatal findings, e.g. checksum mismatches.
    pub warnings: Vec<String>,
}

/// A bag of candidate ROM files.
#[derive(Default, Clone)]
pub struct RomImages {
    by_name: HashMap<String, Vec<u8>>,
    by_sha1: HashMap<String, Vec<u8>>,
}

pub fn sha1_hex(data: &[u8]) -> String {
    sha1_smol::Sha1::from(data).digest().to_string()
}

impl RomImages {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        self.by_sha1.insert(sha1_hex(&data), data.clone());
        self.by_name.insert(name.to_ascii_lowercase(), data);
    }

    /// Load every regular file (up to 64 KB) from a directory, recursively.
    pub fn from_dir(path: &std::path::Path) -> std::io::Result<Self> {
        let mut roms = RomImages::new();
        roms.scan(path, 0)?;
        Ok(roms)
    }

    fn scan(&mut self, path: &std::path::Path, depth: usize) -> std::io::Result<()> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            let meta = entry.metadata()?;
            if meta.is_dir() && depth < 3 {
                self.scan(&p, depth + 1)?;
            } else if meta.is_file()
                && meta.len() <= 0x10000
                && let Some(name) = p.file_name().and_then(|n| n.to_str())
            {
                self.insert(name, std::fs::read(&p)?);
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Which variants could be built from the files present.
    pub fn available_variants(&self) -> Vec<&'static Variant> {
        VARIANTS
            .iter()
            .filter(|v| v.files.iter().all(|f| self.find(f).is_some()))
            .collect()
    }

    fn find(&self, f: &RomFile) -> Option<(&Vec<u8>, bool)> {
        if let Some(d) = self.by_sha1.get(f.sha1) {
            return Some((d, true));
        }
        self.by_name.get(f.name).map(|d| (d, false))
    }

    pub fn build_image(&self, v: &Variant) -> Result<RomImage, String> {
        let mut data = vec![0xFF; 0x10000];
        let mut warnings = Vec::new();
        for f in v.files {
            let (bytes, verified) = self.find(f).ok_or_else(|| {
                format!("{}: ROM '{}' (SHA-1 {}) not found", v.id, f.name, f.sha1)
            })?;
            if !verified {
                warnings.push(format!("{}: SHA-1 mismatch, using it anyway", f.name));
            }
            if bytes.len() != f.size {
                warnings.push(format!(
                    "{}: expected {} bytes, got {}",
                    f.name,
                    f.size,
                    bytes.len()
                ));
            }
            let n = bytes.len().min(f.size);
            let o = f.offset as usize;
            data[o..o + n].copy_from_slice(&bytes[..n]);
        }
        Ok(RomImage { data, warnings })
    }
}
