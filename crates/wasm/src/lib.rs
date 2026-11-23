//! Raw WASM bindings (no wasm-bindgen): the host passes buffers through `alloc`
//! and provides `host_read_at` for on-demand reads from the pack files in OPFS.

use lex_core::{Engine, Reader};
use std::cell::RefCell;

#[link(wasm_import_module = "env")]
extern "C" {
    fn host_read_at(file: u32, off: f64, ptr: *mut u8, len: u32) -> u32;
}

struct Host;

impl Reader for Host {
    fn read_at(&self, file: u8, off: u64, buf: &mut [u8]) -> usize {
        unsafe { host_read_at(file as u32, off as f64, buf.as_mut_ptr(), buf.len() as u32) as usize }
    }
}

thread_local! {
    static ENGINE: RefCell<Option<Engine<Host>>> = const { RefCell::new(None) };
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

unsafe fn take(ptr: *mut u8, len: usize) -> Vec<u8> {
    Vec::from_raw_parts(ptr, len, len.max(1))
}

/// Takes ownership of the three buffers. Returns the entry count, or -1 on a bad pack.
#[no_mangle]
pub unsafe extern "C" fn init(
    fst: *mut u8,
    fst_len: usize,
    forms: *mut u8,
    forms_len: usize,
    blocks: *mut u8,
    blocks_len: usize,
) -> i32 {
    let blocks = take(blocks, blocks_len);
    match Engine::new(take(fst, fst_len), take(forms, forms_len), &blocks, Host) {
        Ok(e) => {
            let n = e.entry_count() as i32;
            ENGINE.with(|c| *c.borrow_mut() = Some(e));
            n
        }
        Err(_) => -1,
    }
}

/// Takes ownership of the query buffer. Returns a pointer to [u32 length][JSON bytes],
/// valid until the next call.
#[no_mangle]
pub unsafe extern "C" fn search(q: *mut u8, q_len: usize, filter: i32, limit: u32) -> *const u8 {
    let q = take(q, q_len);
    let json = ENGINE.with(|c| match c.borrow().as_ref() {
        Some(e) => e.search_json(&String::from_utf8_lossy(&q), filter, limit as usize),
        None => String::from("{\"groups\":[]}"),
    });
    RESULT.with(|r| {
        let mut r = r.borrow_mut();
        r.clear();
        r.extend((json.len() as u32).to_le_bytes());
        r.extend(json.as_bytes());
        r.as_ptr()
    })
}
