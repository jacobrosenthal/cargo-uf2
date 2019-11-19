use cargo_project;
use colored::*;
// use crc::{self, crc16, Hasher16};
use goblin::elf::program_header::*;
use hidapi::{HidApi, HidDevice};

use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    time::Instant,
};
use structopt::StructOpt;
use uf2::*;

fn main() {
    // Initialize the logging backend.
    pretty_env_logger::init();

    // Get commandline options.
    // Skip the first arg which is the calling application name.
    let opt = Opt::from_iter(std::env::args().skip(1));

    // Try and get the cargo project information.
    let project = cargo_project::Project::query(".").unwrap();

    // Decide what artifact to use.
    let artifact = if let Some(bin) = &opt.bin {
        cargo_project::Artifact::Bin(bin)
    } else if let Some(example) = &opt.example {
        cargo_project::Artifact::Example(example)
    } else {
        cargo_project::Artifact::Bin(project.name())
    };

    // Decide what profile to use.
    let profile = if opt.release {
        cargo_project::Profile::Release
    } else {
        cargo_project::Profile::Dev
    };

    // Try and get the artifact path.
    let path = project
        .path(
            artifact,
            profile,
            opt.target.as_ref().map(|t| &**t),
            "x86_64-unknown-linux-gnu",
        )
        .unwrap();

    // Remove first two args which is the calling application name and the `uf2` command from cargo.
    let mut args: Vec<_> = std::env::args().skip(2).collect();

    // todo, keep as iter. difficult because we want to filter map remove two items at once.
    // Remove our args as cargo build does not understand them.
    let flags = ["--pid", "--vid"].iter();
    for flag in flags {
        if let Some(index) = args.iter().position(|x| x == flag) {
            args.remove(index);
            args.remove(index);
        }
    }

    let status = Command::new("cargo")
        .arg("build")
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    if !status.success() {
        use std::os::unix::process::ExitStatusExt;
        let status = status
            .code()
            .or_else(|| if cfg!(unix) { status.signal() } else { None })
            .unwrap_or(1);
        std::process::exit(status);
    }

    let api = HidApi::new().expect("Couldn't find system usb");

    let d = if let (Some(v), Some(p)) = (opt.vid, opt.pid) {
        api.open(v, p)
            .expect("Are you sure device is plugged in and in uf2 mode?")
    } else {
        println!("no vid/pid provided..");

        let mut device: Option<HidDevice> = None;

        for device_info in api.devices() {
            println!(
                "trying {:?} {:?}",
                device_info
                    .manufacturer_string
                    .as_ref()
                    .unwrap_or(&"".to_string()),
                device_info
                    .product_string
                    .as_ref()
                    .unwrap_or(&"".to_string())
            );

            if let Ok(d) = device_info.open_device(&api) {
                let info = Info {}.send(&d);
                if info.is_ok() {
                    device = Some(d);
                    break;
                }
            }
        }
        device.expect("Are you sure device is plugged in and in uf2 mode?")
    };

    println!("    {} {:?}", "Flashing".green().bold(), path);

    // Start timer.
    let instant = Instant::now();

    flash_elf(path, &d).unwrap();

    // Stop timer.
    let elapsed = instant.elapsed();
    println!(
        "    {} in {}s",
        "Finished".green().bold(),
        elapsed.as_millis() as f32 / 1000.0
    );
}

pub trait MemoryRange {
    fn contains_range(&self, range: &std::ops::Range<u32>) -> bool;
    fn intersects_range(&self, range: &std::ops::Range<u32>) -> bool;
}

impl MemoryRange for core::ops::Range<u32> {
    fn contains_range(&self, range: &std::ops::Range<u32>) -> bool {
        self.contains(&range.start) && self.contains(&(range.end - 1))
    }

    fn intersects_range(&self, range: &std::ops::Range<u32>) -> bool {
        self.contains(&range.start) && !self.contains(&(range.end - 1))
            || !self.contains(&range.start) && self.contains(&(range.end - 1))
    }
}

/// Starts the download of a elf file.
fn flash_elf(path: PathBuf, d: &HidDevice) -> Result<(), Error> {
    let mut file = File::open(path)?;
    let mut buffer = vec![];
    file.read_to_end(&mut buffer)?;

    if let Ok(binary) = goblin::elf::Elf::parse(&buffer.as_slice()) {
        for ph in &binary.program_headers {
            if ph.p_type == PT_LOAD && ph.p_filesz > 0 {
                let address = ph.p_paddr as u32;
                let data = &buffer[(ph.p_offset as usize)..][..ph.p_filesz as usize];

                flash(data, address, &d)?;
            }
        }
    }
    Ok(())
}

fn flash(binary: &[u8], address: u32, d: &HidDevice) -> Result<(), uf2::Error> {
    let bininfo: BinInfoResponse = BinInfo {}.send(&d)?;

    if bininfo.mode != BinInfoMode::Bootloader {
        let _ = StartFlash {}.send(&d)?;
    }

    //pad zeros to page size
    // let padded_num_pages = (binary.len() as f64 / f64::from(bininfo.flash_page_size)).ceil() as u32;
    // let padded_size = padded_num_pages * bininfo.flash_page_size;

    // get checksums of existing pages
    // let top_address = address + padded_size as u32;
    // let max_pages = bininfo.max_message_size / 2 - 2;
    // let steps = max_pages * bininfo.flash_page_size;
    // let mut device_checksums = vec![];

    // for target_address in (address..top_address).step_by(steps as usize) {
    //     let pages_left = (top_address - target_address) / bininfo.flash_page_size;

    //     let num_pages = if pages_left < max_pages {
    //         pages_left
    //     } else {
    //         max_pages
    //     };
    //     let chk: ChksumPagesResponse = ChksumPages {
    //         target_address,
    //         num_pages,
    //     }
    //     .send(&d)?;
    //     device_checksums.extend_from_slice(&chk.chksums[..]);
    // }

    // only write changed contents
    for (page_index, page) in binary.chunks(bininfo.flash_page_size as usize).enumerate() {
        // let mut digest1 = crc16::Digest::new_custom(crc16::X25, 0u16, 0u16, crc::CalcType::Normal);

        //pad with zeros in case its last page and under size
        if (page.len() as u32) < bininfo.flash_page_size {
            let mut padded = page.to_vec();
            padded.resize(bininfo.flash_page_size as usize, 0);
            // digest1.write(&padded);
        }
        // else {
        //     digest1.write(&page);
        // }

        // if digest1.sum16() != device_checksums[page_index] {
        let target_address = address + bininfo.flash_page_size * page_index as u32;
        let _ = WriteFlashPage {
            target_address,
            data: page.to_vec(),
        }
        .send(&d)?;
        // }
    }

    let _ = ResetIntoApp {}.send(&d)?;
    Ok(())
}

fn parse_hex_16(input: &str) -> Result<u16, std::num::ParseIntError> {
    if input.starts_with("0x") {
        u16::from_str_radix(&input[2..], 16)
    } else {
        input.parse::<u16>()
    }
}

#[derive(Debug, StructOpt)]
struct Opt {
    // `cargo build` arguments
    #[structopt(name = "binary", long = "bin")]
    bin: Option<String>,
    #[structopt(name = "example", long = "example")]
    example: Option<String>,
    #[structopt(name = "package", short = "p", long = "package")]
    package: Option<String>,
    #[structopt(name = "release", long = "release")]
    release: bool,
    #[structopt(name = "target", long = "target")]
    target: Option<String>,
    #[structopt(name = "PATH", long = "manifest-path", parse(from_os_str))]
    manifest_path: Option<PathBuf>,
    #[structopt(long)]
    no_default_features: bool,
    #[structopt(long)]
    all_features: bool,
    #[structopt(long)]
    features: Vec<String>,

    #[structopt(name = "pid", long = "pid", parse(try_from_str = parse_hex_16))]
    pid: Option<u16>,
    #[structopt(name = "vid", long = "vid",  parse(try_from_str = parse_hex_16))]
    vid: Option<u16>,
}
