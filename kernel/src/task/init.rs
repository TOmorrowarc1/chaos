//! Process-thread creation-time helpers: ELF loading (was `structs.rs`) and
//! initial user-stack setup (was `abi.rs`). Both happen when a new
//! process-thread is created, so they are merged here.

use crate::memory::{GlobalFrameAlloc, MemoryAttr, MemorySet, Read};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::null;
use core::str;
use log::*;
use rcore_fs::vfs::INode;
use rcore_memory::memory_set::handler::File;
use rcore_memory::{Page, PAGE_SIZE};
use xmas_elf::{
    header,
    program::{Flags, SegmentData, Type},
    ElfFile,
};

// ===========================================================================
// Internal: ELF segment flags → MemoryAttr
// ===========================================================================

trait ToMemoryAttr {
    fn to_attr(&self) -> MemoryAttr;
}

impl ToMemoryAttr for Flags {
    fn to_attr(&self) -> MemoryAttr {
        let mut flags = MemoryAttr::default().user();
        if self.is_execute() {
            flags = flags.execute();
        }
        if !self.is_write() {
            flags = flags.readonly();
        }
        flags
    }
}

// ===========================================================================
// ELF loading
// ===========================================================================

/// Helper functions to process an ELF file.
pub trait ElfExt {
    /// Set up a `MemorySet` according to the ELF file. Returns the program break.
    fn make_memory_set(&self, ms: &mut MemorySet, inode: &Arc<dyn INode>) -> usize;

    /// Get the interpreter (dynamic linker) path, if any.
    fn get_interpreter(&self) -> Result<&str, &str>;

    /// Append this ELF as an interpreter into the given memory set at `bias`.
    fn append_as_interpreter(
        &self,
        inode: &Arc<dyn INode>,
        memory_set: &mut MemorySet,
        bias: usize,
    );

    /// Get the virtual address of the PHDR section, if any.
    fn get_phdr_vaddr(&self) -> Option<u64>;
}

impl ElfExt for ElfFile<'_> {
    fn make_memory_set(&self, ms: &mut MemorySet, inode: &Arc<dyn INode>) -> usize {
        debug!("creating MemorySet from ELF");
        let mut farthest: usize = 0;
        for ph in self.program_iter() {
            if ph.get_type() != Ok(Type::Load) {
                continue;
            }
            ms.push(
                ph.virtual_addr() as usize,
                ph.virtual_addr() as usize + ph.mem_size() as usize,
                ph.flags().to_attr(),
                File {
                    file: INodeForMap(inode.clone()),
                    mem_start: ph.virtual_addr() as usize,
                    file_start: ph.offset() as usize,
                    file_end: ph.offset() as usize + ph.file_size() as usize,
                    allocator: GlobalFrameAlloc,
                },
                "elf",
            );
            if ph.virtual_addr() as usize + ph.mem_size() as usize > farthest {
                farthest = ph.virtual_addr() as usize + ph.mem_size() as usize;
            }
        }
        Page::of_addr(farthest + PAGE_SIZE).start_address()
    }

    fn get_interpreter(&self) -> Result<&str, &str> {
        let header = self
            .program_iter()
            .filter(|ph| ph.get_type() == Ok(Type::Interp))
            .next()
            .ok_or("no interp header")?;
        let mut data = match header.get_data(self)? {
            SegmentData::Undefined(data) => data,
            _ => unreachable!(),
        };
        while let Some(0) = data.last() {
            data = &data[..data.len() - 1];
        }
        let path = str::from_utf8(data).map_err(|_| "failed to convert to utf8")?;
        Ok(path)
    }

    fn append_as_interpreter(&self, inode: &Arc<dyn INode>, ms: &mut MemorySet, bias: usize) {
        debug!("inserting interpreter from ELF");
        for ph in self.program_iter() {
            if ph.get_type() != Ok(Type::Load) {
                continue;
            }
            ms.push(
                ph.virtual_addr() as usize + bias,
                ph.virtual_addr() as usize + ph.mem_size() as usize + bias,
                ph.flags().to_attr(),
                File {
                    file: INodeForMap(inode.clone()),
                    mem_start: ph.virtual_addr() as usize + bias,
                    file_start: ph.offset() as usize,
                    file_end: ph.offset() as usize + ph.file_size() as usize,
                    allocator: GlobalFrameAlloc,
                },
                "elf-interp",
            );
        }
    }

    fn get_phdr_vaddr(&self) -> Option<u64> {
        if let Some(phdr) = self
            .program_iter()
            .find(|ph| ph.get_type() == Ok(Type::Phdr))
        {
            Some(phdr.virtual_addr())
        } else if let Some(elf_addr) = self
            .program_iter()
            .find(|ph| ph.get_type() == Ok(Type::Load) && ph.offset() == 0)
        {
            Some(elf_addr.virtual_addr() + self.header.pt2.ph_offset())
        } else {
            warn!("elf: no phdr found, tls might not work");
            None
        }
    }
}

/// Adapter so an on-disk inode can back a file-mapped memory area (`File` handler).
#[derive(Clone)]
pub struct INodeForMap(pub Arc<dyn INode>);

impl Read for INodeForMap {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        self.0.read_at(offset, buf).unwrap()
    }
}

// ===========================================================================
// Initial user stack setup
// ===========================================================================

struct StackWriter {
    sp: usize,
}

impl StackWriter {
    fn push_slice<T: Copy>(&mut self, vs: &[T]) {
        use core::{
            mem::{align_of, size_of},
            slice,
        };
        self.sp -= vs.len() * size_of::<T>();
        self.sp -= self.sp % align_of::<T>();
        unsafe { slice::from_raw_parts_mut(self.sp as *mut T, vs.len()) }.copy_from_slice(vs);
    }

    fn push_str(&mut self, s: &str) {
        self.push_slice(&[b'\0']);
        self.push_slice(s.as_bytes());
    }
}

/// Information written onto the initial user stack of a new process.
pub struct ProcInitInfo {
    pub args: Vec<String>,
    pub envs: Vec<String>,
    pub auxv: BTreeMap<u8, usize>,
}

impl ProcInitInfo {
    /// Lay out argc/argv/envp/auxv onto the user stack starting at `stack_top`.
    /// Returns the new stack pointer.
    pub unsafe fn push_at(&self, stack_top: usize) -> usize {
        let mut writer = StackWriter { sp: stack_top };
        // from stack_top, downward:
        // program name
        writer.push_str(&self.args[0]);
        // environment strings
        let envs: Vec<_> = self
            .envs
            .iter()
            .map(|arg| {
                writer.push_str(arg.as_str());
                writer.sp
            })
            .collect();
        // argv strings
        let argv: Vec<_> = self
            .args
            .iter()
            .map(|arg| {
                writer.push_str(arg.as_str());
                writer.sp
            })
            .collect();
        // auxv terminator
        writer.push_slice(&[null::<u8>(), null::<u8>()]);
        // auxiliary vector entries
        for (&type_, &value) in self.auxv.iter() {
            writer.push_slice(&[type_ as usize, value]);
        }
        // envp null terminator
        writer.push_slice(&[null::<u8>()]);
        // envp pointers
        writer.push_slice(envs.as_slice());
        // argv null terminator
        writer.push_slice(&[null::<u8>()]);
        // argv pointers
        writer.push_slice(argv.as_slice());
        // argc
        writer.push_slice(&[argv.len()]);
        writer.sp
    }
}

// Auxiliary vector entry types.
pub const AT_PHDR: u8 = 3;
pub const AT_PHENT: u8 = 4;
pub const AT_PHNUM: u8 = 5;
pub const AT_PAGESZ: u8 = 6;
pub const AT_BASE: u8 = 7;
pub const AT_ENTRY: u8 = 9;
