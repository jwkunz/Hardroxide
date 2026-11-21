//! The file system against images made and checked by the host's dosfstools (`mkfs.fat`, `fsck.fat`).
//! Skipped (with a message) if those tools are not installed.

use hardroxide_fat::{BlockDevice, DirEntry, FormatOptions, OpenMode, Timestamp, Volume};
use hardroxide_mmio::block::SliceDisk;
use std::path::PathBuf;
use std::process::Command;

fn tools() -> bool {
    let ok = |t: &str| Command::new(t).arg("--help").output().is_ok();
    if !(ok("mkfs.fat") && ok("fsck.fat")) {
        eprintln!("skipping: mkfs.fat and fsck.fat (dosfstools) are not installed");
        return false;
    }
    true
}

fn tmp(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "hrx-fat-{}-{}-{name}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Run `fsck.fat -n` (read only) on `bytes`; panics with its output if it finds anything wrong.
fn fsck(bytes: &[u8], what: &str) {
    let p = tmp(what);
    std::fs::write(&p, bytes).unwrap();
    let out = Command::new("fsck.fat")
        .arg("-n")
        .arg("-v")
        .arg(&p)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_file(&p);
    assert!(
        out.status.success(),
        "fsck.fat reports problems with {what}:\n{text}"
    );
}

/// A bare 64 MiB FAT32 volume made by `mkfs.fat`.
fn mkfs_image(kib: u32) -> Vec<u8> {
    let p = tmp("mkfs");
    let out = Command::new("mkfs.fat")
        .args(["-F", "32", "-n", "TESTVOL", "-C"])
        .arg(&p)
        .arg(kib.to_string())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let b = std::fs::read(&p).unwrap();
    let _ = std::fs::remove_file(&p);
    b
}

/// Deterministic pseudo-random bytes.
fn data(seed: u32, n: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 8) as u8
        })
        .collect()
}

fn read_all<D: BlockDevice>(v: &mut Volume<D>, path: &str) -> Vec<u8>
where
    D::Error: std::fmt::Debug,
{
    let mut f = v.open(path, OpenMode::Read).unwrap();
    let mut out = vec![0u8; f.len() as usize];
    let mut done = 0;
    while done < out.len() {
        let n = f.read(v, &mut out[done..]).unwrap();
        assert!(n > 0, "short read of {path}");
        done += n;
    }
    assert_eq!(f.read(v, &mut [0u8; 4]).unwrap(), 0);
    out
}

fn write_file<D: BlockDevice>(v: &mut Volume<D>, path: &str, bytes: &[u8]) {
    let mut f = v.open(path, OpenMode::Write).unwrap();
    // In uneven pieces, to exercise partial sectors and cluster boundaries.
    let mut at = 0;
    let mut step = 1;
    while at < bytes.len() {
        let n = step.min(bytes.len() - at);
        assert_eq!(f.write(v, &bytes[at..at + n]).unwrap(), n);
        at += n;
        step = step * 3 + 1;
        if step > 40_000 {
            step = 7;
        }
    }
    f.close(v).unwrap();
}

fn list<D: BlockDevice>(v: &mut Volume<D>, path: &str) -> Vec<(String, bool, u32)>
where
    D::Error: std::fmt::Debug,
{
    let mut out: Vec<DirEntry> = Vec::new();
    v.read_dir(path, |e| out.push(*e)).unwrap();
    let mut r: Vec<_> = out
        .iter()
        .map(|e| (e.name.to_string(), e.is_dir, e.size))
        .collect();
    r.sort();
    r
}

#[test]
fn an_image_from_mkfs_mounts_and_is_empty() {
    if !tools() {
        return;
    }
    let mut img = mkfs_image(65_536);
    let mut v = Volume::mount(SliceDisk::new(&mut img)).unwrap();
    assert!(list(&mut v, "/").is_empty());
    assert!(v.free_bytes().unwrap() > 60 * 1024 * 1024);
    assert!(v.stat("/nothing").is_err());
}

#[test]
fn files_and_directories_written_here_pass_fsck() {
    if !tools() {
        return;
    }
    let mut img = mkfs_image(65_536);
    {
        let mut v = Volume::mount(SliceDisk::new(&mut img)).unwrap();
        v.set_time(Timestamp {
            year: 2026,
            month: 11,
            day: 21,
            hour: 12,
            minute: 30,
            second: 0,
        });
        let cb = v.cluster_bytes() as usize;
        let sizes = [
            0,
            1,
            511,
            512,
            513,
            cb - 1,
            cb,
            cb + 1,
            3 * cb + 17,
            1_000_000,
        ];
        v.mkdir("/data").unwrap();
        v.mkdir("/data/deep").unwrap();
        for (i, n) in sizes.iter().enumerate() {
            write_file(&mut v, &format!("/data/f{i}.bin"), &data(i as u32, *n));
        }
        write_file(&mut v, "/hello.txt", b"hello, FAT32\n");
        write_file(
            &mut v,
            "/data/deep/A long file name with spaces.txt",
            b"long name",
        );
        write_file(&mut v, "/data/deep/Another long file name.txt", b"second");
        write_file(&mut v, "/data/deep/Another long file name 2.txt", b"third");
        write_file(&mut v, "/Mixed Case.TXT", b"mixed");
        write_file(&mut v, "/ünïcode-名前.txt", "unicode".as_bytes());
        v.sync().unwrap();

        for (i, n) in sizes.iter().enumerate() {
            assert_eq!(
                read_all(&mut v, &format!("/data/f{i}.bin")),
                data(i as u32, *n),
                "file {i} ({n} bytes)"
            );
        }
        assert_eq!(read_all(&mut v, "/hello.txt"), b"hello, FAT32\n");
        assert_eq!(
            read_all(&mut v, "/DATA/DEEP/a long file name with spaces.TXT"),
            b"long name"
        );
        assert_eq!(read_all(&mut v, "/mixed case.txt"), b"mixed");
        assert_eq!(read_all(&mut v, "/ünïcode-名前.txt"), b"unicode");
        let root = list(&mut v, "/");
        assert!(root.iter().any(|e| e.0 == "Mixed Case.TXT"), "{root:?}");
        assert!(
            root.iter().any(|e| e.0 == "hello.txt" && e.2 == 13),
            "{root:?}"
        );
        let e = v.stat("/hello.txt").unwrap();
        assert_eq!(
            (e.modified.year, e.modified.month, e.modified.day),
            (2026, 11, 21)
        );
    }
    fsck(&img, "written");
}

#[test]
fn appending_overwriting_truncating_and_deleting() {
    if !tools() {
        return;
    }
    let mut img = mkfs_image(65_536);
    {
        let mut v = Volume::mount(SliceDisk::new(&mut img)).unwrap();
        let free0 = v.free_bytes().unwrap();
        write_file(&mut v, "/log.txt", &data(1, 700));
        // Append across a sector and cluster boundary.
        let mut f = v.open("/log.txt", OpenMode::Append).unwrap();
        assert_eq!(f.pos(), 700);
        f.write(&mut v, &data(2, 5000)).unwrap();
        f.close(&mut v).unwrap();
        let mut want = data(1, 700);
        want.extend(data(2, 5000));
        assert_eq!(read_all(&mut v, "/log.txt"), want);
        // Overwrite the middle in place.
        let mut f = v.open("/log.txt", OpenMode::ReadWrite).unwrap();
        f.seek(&mut v, 600).unwrap();
        f.write(&mut v, &[0xAB; 300]).unwrap();
        f.close(&mut v).unwrap();
        want[600..900].fill(0xAB);
        assert_eq!(read_all(&mut v, "/log.txt"), want);
        // Truncate in the middle of the file.
        let mut f = v.open("/log.txt", OpenMode::ReadWrite).unwrap();
        f.seek(&mut v, 1000).unwrap();
        f.truncate(&mut v).unwrap();
        f.close(&mut v).unwrap();
        assert_eq!(read_all(&mut v, "/log.txt"), &want[..1000]);
        // Re-open for writing empties it.
        write_file(&mut v, "/log.txt", b"new");
        assert_eq!(read_all(&mut v, "/log.txt"), b"new");
        // Delete, and the space comes back.
        write_file(&mut v, "/big.bin", &data(3, 2_000_000));
        assert!(v.free_bytes().unwrap() < free0 - 1_900_000);
        v.remove("/big.bin").unwrap();
        v.remove("/log.txt").unwrap();
        assert_eq!(v.free_bytes().unwrap(), free0);
        assert!(v.stat("/big.bin").is_err());
        // A directory with something in it will not go; an empty one will.
        v.mkdir("/d").unwrap();
        write_file(&mut v, "/d/x", b"x");
        assert!(v.remove("/d").is_err());
        v.remove("/d/x").unwrap();
        v.remove("/d").unwrap();
        v.sync().unwrap();
    }
    fsck(&img, "modified");
}

#[test]
fn a_directory_with_many_entries_grows_across_clusters() {
    if !tools() {
        return;
    }
    // 64 MiB with 512-byte clusters (the smallest FAT32): 16 entries per cluster.
    let mut img = mkfs_image(65_536);
    {
        let mut v = Volume::mount(SliceDisk::new(&mut img)).unwrap();
        v.mkdir("/many").unwrap();
        for i in 0..150 {
            write_file(
                &mut v,
                &format!("/many/file number {i:03}.txt"),
                format!("content {i}").as_bytes(),
            );
        }
        let names = list(&mut v, "/many");
        assert_eq!(names.len(), 150);
        assert_eq!(read_all(&mut v, "/many/file number 077.txt"), b"content 77");
        // Remove every other one, add more: freed entries are reused.
        for i in (0..150).step_by(2) {
            v.remove(&format!("/many/file number {i:03}.txt")).unwrap();
        }
        for i in 0..40 {
            write_file(&mut v, &format!("/many/second batch {i}.txt"), b"y");
        }
        assert_eq!(list(&mut v, "/many").len(), 75 + 40);
        v.sync().unwrap();
    }
    fsck(&img, "many");
}

#[test]
fn a_volume_formatted_here_passes_fsck_and_works() {
    if !tools() {
        return;
    }
    // A bare volume (no partition table) so fsck can look at it directly.
    let mut img = vec![0u8; 100 * 1024 * 1024];
    {
        let mut disk = SliceDisk::new(&mut img);
        Volume::format(
            &mut disk,
            &FormatOptions {
                label: "mycard",
                partition_table: false,
                volume_id: 0x1234_ABCD,
            },
        )
        .unwrap();
    }
    fsck(&img, "formatted");
    {
        let mut v = Volume::mount(SliceDisk::new(&mut img)).unwrap();
        write_file(&mut v, "/a.bin", &data(9, 123_456));
        v.mkdir("/sub").unwrap();
        write_file(&mut v, "/sub/b.txt", b"b");
        v.sync().unwrap();
        assert_eq!(read_all(&mut v, "/a.bin"), data(9, 123_456));
    }
    fsck(&img, "formatted-and-written");
}

#[test]
fn a_partitioned_card_formatted_here_mounts_and_its_volume_passes_fsck() {
    if !tools() {
        return;
    }
    let mut img = vec![0u8; 100 * 1024 * 1024];
    {
        let mut disk = SliceDisk::new(&mut img);
        Volume::format(&mut disk, &FormatOptions::default()).unwrap();
    }
    // The MBR: one FAT32 LBA partition at 1 MiB.
    assert_eq!(&img[510..512], &[0x55, 0xAA]);
    assert_eq!(img[446 + 4], 0x0C);
    assert_eq!(
        u32::from_le_bytes(img[446 + 8..446 + 12].try_into().unwrap()),
        2048
    );
    {
        let mut v = Volume::mount(SliceDisk::new(&mut img)).unwrap();
        write_file(&mut v, "/card.txt", b"on a partitioned card");
        v.sync().unwrap();
    }
    // The partition on its own, for fsck.
    fsck(&img[2048 * 512..], "partition");
    let mut v = Volume::mount(SliceDisk::new(&mut img)).unwrap();
    assert_eq!(read_all(&mut v, "/card.txt"), b"on a partitioned card");
}

#[test]
fn an_8_gigabyte_card_gets_4_kib_clusters_and_a_sound_layout() {
    // Only the layout is computed here (a sparse device that remembers just the blocks written).
    use hardroxide_mmio::block::BLOCK_SIZE;
    use std::collections::HashMap;
    struct Sparse(u64, HashMap<u64, [u8; BLOCK_SIZE]>);
    impl BlockDevice for Sparse {
        type Error = ();
        fn block_count(&self) -> u64 {
            self.0
        }
        fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), ()> {
            for (i, c) in buf.chunks_mut(BLOCK_SIZE).enumerate() {
                c.copy_from_slice(
                    &self
                        .1
                        .get(&(lba + i as u64))
                        .copied()
                        .unwrap_or([0; BLOCK_SIZE]),
                );
            }
            Ok(())
        }
        fn write_blocks(&mut self, lba: u64, buf: &[u8]) -> Result<(), ()> {
            for (i, c) in buf.chunks(BLOCK_SIZE).enumerate() {
                if c.iter().any(|&b| b != 0) {
                    self.1.insert(lba + i as u64, c.try_into().unwrap());
                } else {
                    self.1.remove(&(lba + i as u64));
                }
            }
            Ok(())
        }
    }
    let mut dev = Sparse(15_523_840, HashMap::new()); // about 7.4 GiB, a typical "8 GB" card
    Volume::format(&mut dev, &FormatOptions::default()).unwrap();
    let mut v = Volume::mount(dev).unwrap();
    assert_eq!(v.cluster_bytes(), 4096);
    assert!(v.total_bytes() > 7_000_000_000);
    assert!(v.free_bytes().unwrap() > v.total_bytes() - 8192);
    write_file(&mut v, "/x.bin", &data(5, 100_000));
    assert_eq!(read_all(&mut v, "/x.bin"), data(5, 100_000));
}

#[test]
fn the_checker_in_these_tests_does_notice_damage() {
    if !tools() {
        return;
    }
    let mut img = mkfs_image(65_536);
    {
        let mut v = Volume::mount(SliceDisk::new(&mut img)).unwrap();
        write_file(&mut v, "/victim.bin", &data(4, 100_000));
        v.sync().unwrap();
    }
    fsck(&img, "before-damage");
    // Break the file's chain: point the second FAT entry of the file's chain at a free cluster's neighbour.
    let fat0 = 32 * 512; // a bare mkfs volume: 32 reserved sectors
    let first = u32::from_le_bytes(img[fat0 + 3 * 4..fat0 + 3 * 4 + 4].try_into().unwrap());
    img[fat0 + 3 * 4..fat0 + 3 * 4 + 4].copy_from_slice(&(first ^ 0x0000_0F00).to_le_bytes());
    let p = tmp("damaged");
    std::fs::write(&p, &img).unwrap();
    let out = Command::new("fsck.fat").arg("-n").arg(&p).output().unwrap();
    let _ = std::fs::remove_file(&p);
    assert!(!out.status.success(), "fsck.fat should have complained");
}
