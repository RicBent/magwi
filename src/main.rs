mod exheader;
mod hook;
mod job_env;
mod jobs;
mod worker_pool;

use binrw::{BinReaderExt, BinWriterExt};
use exheader::Exheader;

use job_env::JobEnv;
use jobs::{find_jobs, Job, JobKind};
use object::read::*;
use worker_pool::{TaskResult, WorkerPool};

use hook::{HookExtraPos, HookInfo, HookKind, HookLocation, HookWriter};

use std::collections::HashMap;
use std::io::prelude::*;
use std::{io::Write, path::PathBuf, process::Command, vec};

use enum_map::enum_map;

const APP_NAME: &'static str = env!("CARGO_PKG_NAME");
const APP_VERSION: &'static str = env!("CARGO_PKG_VERSION");

fn print_step(step: usize, name: &str) {
    const NUM_STEPS: usize = 4;
    println!(
        "{} {}",
        console::style(format!("[{step}/{NUM_STEPS}]")).bold(),
        console::style(name).cyan().bold(),
    );
}

fn fatal_error(msg: impl AsRef<str>) -> ! {
    println!("{}", console::style(msg.as_ref()).bold().red());
    std::process::exit(1)
}

macro_rules! fatal_error {
    ($($arg:tt)*) => {
        fatal_error(format!($($arg)*))
    }
}

fn hook_error(location: impl AsRef<HookLocation>, msg: impl AsRef<str>) -> ! {
    let location = location.as_ref();

    println!(
        "{}: {} {}",
        console::style(format!("{location}")).bold(),
        console::style("error:").bold().red(),
        msg.as_ref(),
    );

    if let Ok(file) = std::fs::File::open(&location.file) {
        if let Some(Ok(line)) = std::io::BufReader::new(file)
            .lines()
            .nth(location.line as usize - 1)
        {
            println!("    {} | {}", location.line, line);
        }
    }

    std::process::exit(1)
}

macro_rules! hook_error {
    ($location:expr, $($arg:tt)*) => {
        hook_error($location, format!($($arg)*))
    }
}

fn calc_loader_address(eh: &Exheader) -> u32 {
    eh.info.sci.text_section.address + eh.info.sci.text_section.size
}

fn calc_loader_max_size(eh: &Exheader) -> u32 {
    eh.info.sci.text_section.num_pages * exheader::PAGE_SIZE - eh.info.sci.text_section.size
}

fn calc_custom_text_address(eh: &Exheader) -> u32 {
    eh.info.sci.data_section.address
        + eh.info.sci.data_section.num_pages * exheader::PAGE_SIZE
        + eh.info.sci.bss_size
}

fn main() {
    println!("{} v{}", APP_NAME, APP_VERSION);

    let project_path = std::env::args().nth(1);

    let project_path = match project_path {
        Some(path) => PathBuf::from(path),
        None => std::env::current_dir().expect("Failed to get current directory"),
    };
    std::env::set_current_dir(&project_path).expect("Failed to set current directory");

    let mut writer = HookWriter::new(0x100000, std::fs::read("original/code.bin").unwrap());

    let job_env = std::sync::Arc::from(JobEnv {
        cwd: project_path.clone(),
        compiler: enum_map! {
            JobKind::C   => "arm-none-eabi-gcc",
            JobKind::CPP => "arm-none-eabi-g++",
            JobKind::ASM => "arm-none-eabi-gcc",
        },
        flags: enum_map! {
            JobKind::C   => vec![
                "-iquote", "include", "-isystem", "include/sys", "-isystem", "include/sys/clib",
                "-march=armv6k+fp", "-mtune=mpcore", "-mfloat-abi=hard", "-mtp=soft",
                "-fdiagnostics-color", "-Wall", "-O3", "-mword-relocations", "-fshort-wchar", "-fomit-frame-pointer", "-ffunction-sections", "-nostdinc"
            ],
            JobKind::CPP => vec![
                "-iquote", "include", "-isystem", "include/sys", "-isystem", "include/sys/clib",
                "-march=armv6k+fp", "-mtune=mpcore", "-mfloat-abi=hard", "-mtp=soft",
                "-fdiagnostics-color", "-Wall", "-O3", "-mword-relocations", "-fshort-wchar", "-fomit-frame-pointer", "-ffunction-sections", "-nostdinc",
                "-fno-exceptions", "-fno-rtti"
            ],
            JobKind::ASM => vec![
                "-iquote", "include", "-isystem", "include/sys", "-isystem", "include/sys/clib",
                "-march=armv6k+fp", "-mtune=mpcore", "-mfloat-abi=hard", "-mtp=soft",
                "-fdiagnostics-color", "-x", "assembler-with-cpp"
            ],
        },
    });

    let mut exheader: Exheader = std::fs::File::open("original/exheader.bin")
        .expect("Opening exheader failed")
        .read_ne()
        .expect("Reading exheader failed");

    let loader_address = calc_loader_address(&exheader);
    let loader_max_size = calc_loader_max_size(&exheader);
    let custom_text_address = calc_custom_text_address(&exheader);

    let Ok(mut jobs) = find_jobs("source", "build/obj", "build/dep", true) else {
        println!("Failed to find jobs: io error");
        return;
    };

    jobs.iter_mut().for_each(|job| {
        job.update_build_reason();
    });

    let todo_jobs: Vec<&Job> = jobs.iter().filter(|job| job.build_required()).collect();

    print_step(1, "Compiling...");

    let pb_root = indicatif::MultiProgress::new();

    let pb = indicatif::ProgressBar::new(todo_jobs.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
        )
        .expect("Progress style template should be valid")
        .progress_chars("=>."),
    );
    pb_root.add(pb.clone());

    pb.inc(0);

    let spinner_style = indicatif::style::ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .expect("Progress style template should be valid");

    let num_workers = num_cpus::get();
    let spinners = (0..num_workers)
        .map(|_| {
            let pb = pb_root.add(indicatif::ProgressBar::new_spinner());
            pb.set_style(spinner_style.clone());
            pb.set_message(format!("waiting..."));
            pb
        })
        .collect::<Vec<_>>();

    let mut pool = WorkerPool::new(num_workers);

    for job in todo_jobs {
        // a bit wasteful to clone these, but oh well
        let pb = pb.clone();
        let spinners = spinners.clone();
        let job = job.clone();
        let job_env = job_env.clone();

        pool.submit_task(move |thread_idx| {
            let spinner = &spinners[thread_idx];
            spinner.enable_steady_tick(std::time::Duration::from_millis(100));
            spinner.set_message(job.src_path.display().to_string());

            match job_env.execute_job(&job) {
                Ok(_) => {
                    pb.inc(1);
                    TaskResult::Ok
                }
                Err(e) => {
                    pb.println(e.to_string());
                    TaskResult::Terminate
                }
            }
        });
    }

    if pool.wait() == TaskResult::Terminate {
        fatal_error("Compilation failed");
    }

    pb.finish_and_clear();
    for spinner in spinners {
        spinner.finish_and_clear();
    }
    pb_root.clear().ok();

    print_step(2, "Section hooks...");

    let mut linker_file = std::fs::File::create("build/linker.ld").unwrap();
    linker_file
        .write("SECTIONS\n{\n    /* Hook Generated Sections */\n".as_bytes())
        .unwrap();

    let mut obj_paths = Vec::new();

    for job in &jobs {
        obj_paths.push(&job.obj_path);

        let elf_data = std::fs::read(&job.obj_path).unwrap();
        let elf_file = object::File::parse(elf_data.as_slice()).unwrap();

        for section in elf_file.sections() {
            let Ok(name) = section.name() else {
                continue;
            };

            match HookInfo::from_section_str(name) {
                Ok(hi) => {
                    match hi.kind {
                        HookKind::Replace(repl_addr) => {
                            linker_file
                                .write(
                                    format!("    {name} 0x{repl_addr:x} : {{ *({name}); }}\n")
                                        .as_bytes(),
                                )
                                .unwrap();
                        }
                        // Invalid kinds are discarded
                        _ => {
                            hook_error!(hi.location, "Invalid hook kind for section hook");
                        }
                    }
                }
                Err(hook::Error::InvalidPrefix) => {}
                Err(hook::Error::ParsingError(e, loc)) => {
                    hook_error!(loc, "{}", e);
                }

                Err(e) => {
                    fatal_error!("Parsing section hook \"{}\" failed: {:?}", name, e);
                }
            }
        }
    }

    linker_file.write(format!(
        "\n    .mw_loader_text 0x{loader_address:x} : {{ *(.mw_loader_text); *(.mw_loader_text.*); }}\n",
    ).as_bytes()).unwrap();

    linker_file
        .write(format!("    .text 0x{custom_text_address:x} :\n",).as_bytes())
        .unwrap();
    linker_file
        .write(LINKER_SCRIPT_SECTIONS.as_bytes())
        .unwrap();

    linker_file.write("}\n".as_bytes()).unwrap();
    drop(linker_file);

    print_step(3, "Linking...");

    let output = Command::new("arm-none-eabi-g++")
        .current_dir(&project_path)
        .args(vec![
            "-nodefaultlibs",
            "-nostartfiles",
            "-march=armv6k+fp",
            "-mtune=mpcore",
            "-mfloat-abi=hard",
            "-mtp=soft",
            "-T",
            "symbols.ld",
            "-T",
            "build/linker.ld",
            "-Wl,-Map=build/out.map",
            "-fdiagnostics-color",
        ])
        .args(obj_paths)
        .arg("-o")
        .arg("build/out.elf")
        .output();

    match output {
        Ok(output) => {
            let err = String::from_utf8_lossy(&output.stderr);
            if !err.is_empty() {
                println!("{}", err);
            }
            if !output.status.success() {
                fatal_error("Linking failed");
            }
        }
        Err(e) => {
            fatal_error!("Running linker failed: {e}");
        }
    }

    let elf_data = std::fs::read("build/out.elf").unwrap();
    let elf_file = object::File::parse(elf_data.as_slice()).unwrap();

    let mut loader_text_section = None;
    let mut custom_text_section = None;

    for section in elf_file.sections() {
        let Ok(name) = section.name() else {
            continue;
        };

        if name == ".mw_loader_text" {
            writer.set_loader_extra_address(section.address() as u32 + section.size() as u32);
            loader_text_section = Some(section);
            continue;
        }

        if name == ".text" {
            custom_text_section = Some(section);
            continue;
        }

        // No need for a full parse here. Emitting the section is only possible if the hook is valid.
        if !name.starts_with(HookInfo::SECTION_PREFIX) {
            continue;
        }

        let address = section.address() as u32;
        let data = section
            .data()
            .expect("Failed to read section data for hook section");

        writer.write(address, data).unwrap();
    }

    print_step(4, "Symbol hooks...");

    #[derive(Debug)]
    struct PrePostEntry {
        extra_pos: HookExtraPos,
        pre: Vec<(u32, HookLocation)>,
        post: Vec<(u32, HookLocation)>,
    }

    let mut pre_post_entries: HashMap<u32, PrePostEntry> = HashMap::new();
    let mut text_end_symbol = None;

    let symtab = elf_file.symbol_table().unwrap();
    let mut symtab_index: HashMap<String, u32> = HashMap::new();

    for sym in symtab.symbols() {
        let Ok(name) = sym.name() else {
            continue;
        };

        let address = sym.address() as u32;

        symtab_index.insert(name.into(), address);
        if let Ok(demangled_sym) = cpp_demangle::Symbol::new(name) {
            symtab_index.insert(demangled_sym.to_string(), address);
        }

        match HookInfo::from_symbol_str(name) {
            Ok(hi) => match hi.kind {
                HookKind::Branch(branch) => {
                    let to_addr = address;
                    let data = branch
                        .to_u32(to_addr)
                        .unwrap_or_else(|| {
                            hook_error!(
                                hi.location,
                                "Branch destination 0x{:x} is out of range from 0x{:x}",
                                branch.from_addr,
                                to_addr,
                            );
                        })
                        .to_le_bytes();
                    writer.write(branch.from_addr, data).unwrap();
                }
                HookKind::Pre(from_addr) | HookKind::Post(from_addr) => {
                    let extra_pos = if from_addr < custom_text_address {
                        HookExtraPos::Loader
                    } else {
                        HookExtraPos::Tail
                    };

                    let entry = pre_post_entries
                        .entry(from_addr)
                        .or_insert_with(|| PrePostEntry {
                            pre: Vec::new(),
                            post: Vec::new(),
                            extra_pos: extra_pos,
                        });

                    if extra_pos != entry.extra_pos {
                        hook_error!(
                            hi.location,
                            "Pre/post hooks for 0x{:x} are in different sections",
                            from_addr,
                        );
                    }

                    let a = (address, hi.location);

                    match hi.kind {
                        HookKind::Pre(_) => entry.pre.push(a),
                        HookKind::Post(_) => entry.post.push(a),
                        _ => unreachable!(),
                    }
                }
                HookKind::Symptr(patch_addr) => {
                    writer.write(patch_addr, address.to_le_bytes()).unwrap()
                }
                _ => {
                    hook_error!(hi.location, "Invalid hook kind for symbol hook");
                }
            },
            Err(hook::Error::InvalidPrefix) => {
                if name == "__mw_text_end" {
                    text_end_symbol = Some(sym);
                }
            }
            Err(hook::Error::ParsingError(e, loc)) => {
                hook_error!(loc, "{}", e);
            }
            Err(e) => {
                fatal_error!("Parsing symbol hook \"{}\" failed: {}", name, e);
            }
        }
    }

    for e in std::fs::read_dir("hooks").unwrap() {
        let Ok(e) = e else {
            continue;
        };

        let Ok(ft) = e.file_type() else {
            continue;
        };

        if !ft.is_file() {
            continue;
        }

        if e.path().extension() != Some(std::ffi::OsStr::new("hks")) {
            continue;
        }

        for h in hook::hks::open_file(e.path()).unwrap() {
            let Ok(mut h) = h else {
                fatal_error!("Failed to parse hook file");
            };

            macro_rules! hks_hook_error {
                ($($arg:tt)*) => {
                    hook_error!(HookLocation { file: e.path(), line: h.line() as u32 }, $($arg)*)
                }
            }

            let address = h.get_address("addr").unwrap();

            match h.get("type").unwrap().as_str() {
                "branch" => {
                    let link = h.get_bool("link").unwrap();

                    let to_address = if h.has("func") {
                        let sym = h.get("func").unwrap();
                        *symtab_index.get(sym.as_str()).unwrap_or_else(|| {
                            hks_hook_error!("Symbol \"{}\" not found", sym);
                        })
                    } else {
                        h.get_address("dest").unwrap()
                    };

                    writer
                        .write(
                            address,
                            hook::arm::make_branch_u32(
                                link,
                                address,
                                to_address,
                                hook::arm::ArmCondition::AL,
                            )
                            .unwrap()
                            .to_le_bytes(),
                        )
                        .unwrap();
                }
                "softbranch" | "soft_branch" => {
                    let opcode_pos = h.get("opcode").unwrap();

                    let to_address = if h.has("func") {
                        let sym = h.get("func").unwrap();
                        *symtab_index.get(sym.as_str()).unwrap_or_else(|| {
                            hks_hook_error!("Symbol \"{}\" not found", sym);
                        })
                    } else {
                        h.get_address("dest").unwrap()
                    };

                    let extra_pos = if to_address < custom_text_address {
                        HookExtraPos::Loader
                    } else {
                        HookExtraPos::Tail
                    };

                    let entry = pre_post_entries
                        .entry(address)
                        .or_insert_with(|| PrePostEntry {
                            pre: Vec::new(),
                            post: Vec::new(),
                            extra_pos: extra_pos,
                        });

                    if extra_pos != entry.extra_pos {
                        hks_hook_error!(
                            "Pre/post hooks for 0x{:x} are in different sections",
                            address,
                        );
                    }

                    let a = (
                        to_address,
                        HookLocation {
                            file: e.path(),
                            line: h.line() as u32,
                        },
                    );

                    match opcode_pos.as_str() {
                        "pre" => entry.post.push(a),
                        "post" => entry.pre.push(a),
                        _ => {
                            hks_hook_error!("Invalid opcode position \"{}\"", opcode_pos);
                        }
                    }
                }
                "patch" => {
                    let data_str = h.get("data").unwrap().replace(" ", "");

                    let data_chars = data_str.chars().collect::<Vec<_>>();

                    if data_chars.len() % 2 != 0 {
                        hks_hook_error!(
                            "Invalid patch data \"{}\": Must be multiple of 2 hex character",
                            data_str
                        );
                    }

                    for (i, c) in data_chars.iter().enumerate() {
                        if !c.is_ascii_hexdigit() {
                            hks_hook_error!(
                                "Invalid patch data \"{}\": Invalid hex character at index {}",
                                data_str,
                                i
                            );
                        }
                    }

                    let data = data_chars
                        .chunks_exact(2)
                        .map(|c| u8::from_str_radix(&c.iter().collect::<String>(), 16).unwrap())
                        .collect::<Vec<_>>();

                    writer.write(address, data).unwrap();
                }
                "symbol" | "symptr" | "sym_ptr" => {
                    let sym = h.get("sym").unwrap();
                    let sym_addr = symtab_index.get(sym.as_str()).unwrap_or_else(|| {
                        hks_hook_error!("Symbol \"{}\" not found", sym);
                    });

                    writer.write(address, sym_addr.to_le_bytes()).unwrap();
                }
                t => {
                    hks_hook_error!("Invalid hook type \"{}\"", t)
                }
            }

            if !h.is_done() {
                hks_hook_error!(
                    "Unused keys: \"{}\"",
                    h.remaining_keys().collect::<Vec<_>>().join("\", \"")
                );
            }
        }
    }

    match loader_text_section {
        Some(section) => {
            let used_loader_size = section.size() as u32;

            println!("{}", console::style("Loader:").bold());
            println!("  address: 0x{:08x}", loader_address);
            println!(" max size: 0x{:08x}", loader_max_size);
            println!(
                "     size: 0x{:08x} ({:.2}%)",
                used_loader_size,
                used_loader_size as f32 / loader_max_size as f32 * 100.0
            );

            if used_loader_size > loader_max_size {
                fatal_error!("Loader size exceeds maximum size");
            }

            let data = section
                .data()
                .expect("Failed to read loader text section data");
            writer.write(loader_address, data).unwrap();
        }
        None => {
            fatal_error!("Loader text section not found");
        }
    }

    match custom_text_section {
        Some(section) => {
            let used_text_size = section.size() as u32;

            println!("{}", console::style("Custom text:").bold());
            println!("  address: 0x{:08x}", custom_text_address);
            println!("     size: 0x{:08x}", used_text_size);

            let data = section
                .data()
                .expect("Failed to read custom text section data");

            let end_address = (custom_text_address + used_text_size + 0xFFF) & !0xFFF;

            writer.resize_until(end_address).unwrap();
            writer.write(custom_text_address, data).unwrap();

            if let Some(_text_end_symbol) = text_end_symbol {
                // TODO: This sym needs to be fixed, otherwise extra data will not be reprotected by the loader properly
                // set to writer.end_address()
            }
        }
        None => {
            fatal_error!("Custom text section not found");
        }
    }

    for (from_address, entry) in &pre_post_entries {
        writer
            .write_extra(entry.extra_pos, |writer, extra_writer| {
                let original_instruction = u32::from_le_bytes(writer.read(*from_address).unwrap());

                // Write jump to extra block
                writer
                    .write(
                        *from_address,
                        hook::arm::make_branch_u32(
                            false,
                            *from_address,
                            extra_writer.base_address(),
                            hook::arm::ArmCondition::AL,
                        )
                        .unwrap()
                        .to_le_bytes(),
                    )
                    .unwrap();

                // Write pre hooks
                for (dest_addr, _) in &entry.pre {
                    // push {r0-r12, lr}
                    extra_writer
                        .write_end(
                            hook::arm::make_push_u32(0x5FFF, hook::arm::ArmCondition::AL)
                                .to_le_bytes(),
                        )
                        .unwrap();

                    extra_writer
                        .write_end(
                            hook::arm::make_branch_u32(
                                true,
                                extra_writer.end_address(),
                                *dest_addr,
                                hook::arm::ArmCondition::AL,
                            )
                            .unwrap()
                            .to_le_bytes(),
                        )
                        .unwrap();

                    // pop {r0-r12, lr}
                    extra_writer
                        .write_end(
                            hook::arm::make_pop_u32(0x5FFF, hook::arm::ArmCondition::AL)
                                .to_le_bytes(),
                        )
                        .unwrap();
                }

                // Write original instruction
                let relocated_instruction = hook::arm::relocate_u32(
                    original_instruction,
                    *from_address,
                    extra_writer.end_address(),
                )
                .unwrap_or_else(|| fatal_error!("Relocating original instruction failed"));
                extra_writer
                    .write_end(relocated_instruction.to_le_bytes())
                    .unwrap();

                // Write post hooks
                for (dest_addr, _) in &entry.post {
                    // push {r0-r12, lr}
                    extra_writer
                        .write_end(
                            hook::arm::make_push_u32(0x5FFF, hook::arm::ArmCondition::AL)
                                .to_le_bytes(),
                        )
                        .unwrap();

                    extra_writer
                        .write_end(
                            hook::arm::make_branch_u32(
                                true,
                                extra_writer.end_address(),
                                *dest_addr,
                                hook::arm::ArmCondition::AL,
                            )
                            .unwrap()
                            .to_le_bytes(),
                        )
                        .unwrap();

                    // pop {r0-r12, lr}
                    extra_writer
                        .write_end(
                            hook::arm::make_pop_u32(0x5FFF, hook::arm::ArmCondition::AL)
                                .to_le_bytes(),
                        )
                        .unwrap();
                }

                // Write jump back to original code
                extra_writer
                    .write_end(
                        hook::arm::make_branch_u32(
                            false,
                            extra_writer.end_address(),
                            *from_address + 4,
                            hook::arm::ArmCondition::AL,
                        )
                        .unwrap()
                        .to_le_bytes(),
                    )
                    .unwrap();
            })
            .unwrap();
    }

    std::fs::write("build/code.bin", writer.data()).unwrap();

    exheader.info.sci.text_section.size =
        exheader.info.sci.text_section.num_pages * exheader::PAGE_SIZE;
    exheader.info.sci.data_section.size =
        writer.end_address() - exheader.info.sci.data_section.address;
    exheader.info.sci.data_section.num_pages =
        exheader::page_count(exheader.info.sci.data_section.size);
    exheader.info.sci.bss_size = 0;

    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open("build/exheader.bin")
        .unwrap()
        .write_ne(&exheader)
        .unwrap();

    println!("{}", console::style("Done!").green().bold());
}

const LINKER_SCRIPT_SECTIONS: &str = r#"    {
        __mw_text_start = .;
        *(.text);
        *(.text.*);
        *(.rodata);
        *(.rodata.*);
        __init_array_start = .;
        *(.init_array);
        *(.init_array.*);
        __init_array_end = .;
        __fini_array_start = .;
        *(.fini_array);
        *(.fini_array.*);
        __fini_array_end = .;
        *(.data);
        *(.data.*);
        *(.bss);
        *(.bss.*);
        __mw_text_end = .;
    }
"#;
