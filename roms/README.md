# ROMs

## Source

The dumps in this directory come from the MAME 0.201 merged ROM set on the
Internet Archive, https://archive.org/download/MAME201_Merged (`lc80.zip`
and `poly880.zip`), unpacked unchanged. The POLY880 SC1 chess ROM
(`poly880/poly880s/sc1.rom`) was added to MAME later; it comes from the MAME
0.279 merged set, https://archive.org/download/mame-roms-0.279-merged
(`poly880.zip`). It is byte-identical to the `roms/poly880/sc1.rom` shipped
with Torsten Paul's KCemu (https://github.com/t-paul/kcemu); MAME added the
set as "Poly-Computer 880 (SC1)" (`poly880s`) in 0.232. The SHA-1 checksums match those
catalogued in MAME's `ddr/lc80.cpp` and `ddr/poly880.cpp` drivers.

The firmware was written in the GDR by VEB Mikroelektronik "Karl Marx"
Erfurt (LC-80) and VEB Polytechnik Karl-Marx-Stadt (POLY880). The LC-80.2
ROM is a later modification by Eckart Buschendorf. These files are included
for preservation and educational use and are **not** covered by this
repository's license.

| Directory       | Contents                                          |
|-----------------|---------------------------------------------------|
| `lc80/`         | LC-80 set 1 (`lc80.d202`/`.d203`), set 2 (2716)   |
| `lc80/sc80/`    | LC-80 export version with the SC-80 chess program |
| `lc80/lc80_2/`  | LC-80.2                                           |
| `poly880/`      | Poly-Computer 880 (`poly880.i5`/`.i6`)            |
| `poly880/poly880s/` | SC1 chess conversion of the POLY880 (`sc1.rom`) |

## Lookup

Files are matched by SHA-1 first, so the file names don't matter;
sub-directories are searched too. `--roms DIR` / `EMULATRON_ROMS` point the
emulator elsewhere, and `emulatron --list` shows what was found.

| Variant    | File                    | Load  | Size  | SHA-1                                      |
|------------|-------------------------|-------|-------|--------------------------------------------|
| `lc80`     | `bm075.d202`            | 0000h | 1 KB  | `044440b13e62addbc3f6a77369cfd16f99b39752` |
|            | `bm076.d203`            | 0800h | 1 KB  | `3a6cbd0c57c38eadb7055dca4b396c348567d1d5` |
| `lc80a`    | `lc80_2716.bin`         | 0000h | 2 KB  | `6fff953f0f1eee829fd774366313ab7e8053468c` |
| `lc80e`    | `lc80e-0000-schach.rom` | 0000h | 4 KB  | `f2be3f2a9d3780d59657e49b3abeffb0fc13db89` |
|            | `lc80e-1000-schach.rom` | 1000h | 4 KB  | `0ea019b0944736ae5b842bf9aa3537300f259b98` |
|            | `lc80e-c000-schach.rom` | C000h | 4 KB  | `2f7b3fd046c965185606253f6cd9372da289ca6f` |
| `lc80_2`   | `lc80_2.bin`            | 0000h | 4 KB  | `d9cddaf847831e4ab21854c0f895348b7fda20b8` |
| `poly880`  | `bm039.i5`              | 0000h | 1 KB  | `85bfe53d39d6690e79999a1e1240789497e72db0` |
|            | `bm040.i6`              | 1000h | 1 KB  | `6ffa2f80b2c6f8ec9e22834f739c82f9754272b8` |
| `poly880s` | `sc1.rom`               | 0000h | 4 KB  | `01568911446eda9f05ec136df53da147b7c6f2bf` |

File names in the table are MAME's current ones; the files here carry the
older 0.201 names (e.g. `lc80.d202`).
