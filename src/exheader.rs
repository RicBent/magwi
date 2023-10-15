use binrw::binrw;

#[binrw]
pub struct CodeSection {
    pub address: u32,
    pub num_pages: u32,
    pub size: u32,
}

#[binrw]
pub struct SCI {
    pub name: [u8; 8],
    pub flags: [u8; 6],
    pub remaster_version: u16,
    pub text_section: CodeSection,
    pub stack_size: u32,
    pub rodata_section: CodeSection,
    pub _reserved1: [u8; 4],
    pub data_section: CodeSection,
    pub bss_size: u32,
    pub dependencies: [u64; 48],
    pub save_data_size: u64,
    pub jump_id: u64,
    pub _reserved2: [u8; 0x30],
}

#[binrw]
pub struct ACI {
    pub data: [u8; 0x200],
}

#[binrw]
pub struct Info {
    pub sci: SCI,
    pub aci: ACI,
}

#[binrw]
pub struct ACIExt {
    pub rsa: [u8; 0x100],
    pub ncch_header_rsa: [u8; 0x100],
    pub aci: ACI,
}

#[binrw]
#[brw(little)]
pub struct Exheader {
    pub info: Info,
    pub aci_ext: ACIExt,
}

pub const PAGE_SIZE: u32 = 0x1000;

pub fn round_to_page(v: u32) -> u32 {
    (v + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

pub fn page_count(v: u32) -> u32 {
    round_to_page(v) / PAGE_SIZE
}