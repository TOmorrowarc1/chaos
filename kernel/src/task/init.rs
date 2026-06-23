//! Process-thread creation-time helpers: ELF loading (was `structs.rs`) and
//! initial user-stack setup (was `abi.rs`). Both happen when a new
//! process-thread is created, so they are merged here.

use crate::memory::{MemorySet, Read};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use rcore_fs::vfs::INode;
use xmas_elf::ElfFile;

// ===========================================================================
// ELF loading  (was structs.rs)
// ===========================================================================

/// Helper functions to process an ELF file.
pub trait ElfExt {
    /// Set up a `MemorySet` according to the ELF file. Returns the program break.
    fn make_memory_set(&self, ms: &mut MemorySet, inode: &Arc<dyn INode>) -> usize;

    /// Get the interpreter (dynamic linker) path, if any.
    fn get_interpreter(&self) -> Result<&str, &str>;

    /// Append this ELF as an interpreter into the given memory set at `bias`.
    fn append_as_interpreter(&self, inode: &Arc<dyn INode>, memory_set: &mut MemorySet, bias: usize);

    /// Get the virtual address of the PHDR section, if any.
    fn get_phdr_vaddr(&self) -> Option<u64>;
}

impl ElfExt for ElfFile<'_> {
    fn make_memory_set(&self, ms: &mut MemorySet, inode: &Arc<dyn INode>) -> usize {
        todo!()
    }

    fn get_interpreter(&self) -> Result<&str, &str> {
        todo!()
    }

    fn append_as_interpreter(&self, inode: &Arc<dyn INode>, ms: &mut MemorySet, bias: usize) {
        todo!()
    }

    fn get_phdr_vaddr(&self) -> Option<u64> {
        todo!()
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
// Initial user stack setup  (was abi.rs)
// ===========================================================================

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
        todo!()
    }
}

// Auxiliary vector entry types.
pub const AT_PHDR: u8 = 3;
pub const AT_PHENT: u8 = 4;
pub const AT_PHNUM: u8 = 5;
pub const AT_PAGESZ: u8 = 6;
pub const AT_BASE: u8 = 7;
pub const AT_ENTRY: u8 = 9;
