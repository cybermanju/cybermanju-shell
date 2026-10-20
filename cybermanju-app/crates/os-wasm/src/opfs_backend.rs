// CyberManju OS — OPFS `StorageBackend` for redb (WASM demo build)
//
// redb's only filesystem assumption is the 5-method `StorageBackend` trait
// (`len/read/set_len/sync_data/write`), all synchronous. In a browser the
// synchronous file primitive is `FileSystemSyncAccessHandle`, which exists
// **only inside Dedicated Workers** — so the database lives in `db-worker`
// and the handle below is opened there (async open, sync I/O after).
//
// Deliberately `web-sys`-free like the rest of this crate: every JS call
// goes through `js_sys::Reflect`, mirroring `os.rs`.
//
// Durability honesty (see redb#1101): OPFS `flush()` is not `fsync()` —
// notably on iOS Safari, where evicted/truncated files have corrupted
// databases. The dispatcher therefore opens the database with paranoid
// two-phase commits and keeps periodic whole-file snapshots; see `db.rs`.

use js_sys::{Object, Reflect, Uint8Array};
use redb::StorageBackend;
use std::fmt;
use std::io::Error;
use wasm_bindgen::prelude::JsValue;
use wasm_bindgen::JsCast;

/// An already-open OPFS sync access handle for one database file.
#[derive(Clone)]
pub struct OpfsBackend {
    handle: JsValue,
    file_name: String,
}

impl fmt::Debug for OpfsBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpfsBackend")
            .field("file_name", &self.file_name)
            .finish_non_exhaustive()
    }
}

fn io_err(what: &str, detail: String) -> Error {
    Error::other(format!("opfs:{what}: {detail}"))
}

fn get(obj: &JsValue, name: &str) -> Result<JsValue, Error> {
    Reflect::get(obj, &JsValue::from_str(name)).map_err(|e| io_err("reflect", format!("{e:?}")))
}

fn call0(obj: &JsValue, name: &str) -> Result<JsValue, Error> {
    let func: js_sys::Function = get(obj, name)?
        .dyn_into()
        .map_err(|_| io_err(name, "not a function".to_string()))?;
    Reflect::apply(&func, obj, &js_sys::Array::new()).map_err(|e| io_err(name, format!("{e:?}")))
}

fn call1(obj: &JsValue, name: &str, arg: &JsValue) -> Result<JsValue, Error> {
    let func: js_sys::Function = get(obj, name)?
        .dyn_into()
        .map_err(|_| io_err(name, "not a function".to_string()))?;
    let argv = js_sys::Array::new();
    argv.push(arg);
    Reflect::apply(&func, obj, &argv).map_err(|e| io_err(name, format!("{e:?}")))
}

fn call2(obj: &JsValue, name: &str, a: &JsValue, b: &JsValue) -> Result<JsValue, Error> {
    let func: js_sys::Function = get(obj, name)?
        .dyn_into()
        .map_err(|_| io_err(name, "not a function".to_string()))?;
    let argv = js_sys::Array::new();
    argv.push(a);
    argv.push(b);
    Reflect::apply(&func, obj, &argv).map_err(|e| io_err(name, format!("{e:?}")))
}

fn opts_at(offset: u64) -> Result<JsValue, Error> {
    let opts = Object::new();
    Reflect::set(
        &opts,
        &JsValue::from_str("at"),
        &JsValue::from_f64(offset as f64),
    )
    .map_err(|e| io_err("opts", format!("{e:?}")))?;
    Ok(opts.into())
}

impl OpfsBackend {
    pub fn new(handle: JsValue, file_name: &str) -> Self {
        Self {
            handle,
            file_name: file_name.to_string(),
        }
    }
}

impl StorageBackend for OpfsBackend {
    fn len(&self) -> Result<u64, Error> {
        let size = call0(&self.handle, "getSize")
            .map_err(|_| io_err("getSize", "sync handle unavailable".to_string()))?;
        size.as_f64()
            .map(|v| v as u64)
            .ok_or_else(|| io_err("getSize", "non-numeric size".to_string()))
    }

    fn read(&self, offset: u64, len: usize) -> Result<Vec<u8>, Error> {
        let total = self.len()?;
        if offset.saturating_add(len as u64) > total {
            return Err(io_err(
                "read",
                format!("{len} bytes at {offset} exceed length {total}"),
            ));
        }
        // One positioned read into a fresh view, then copy out. A short
        // read inside the known length means the handle is unusable.
        // (`view` is cloned into a JsValue first — `into()` moves.)
        let view = Uint8Array::new_with_length(len as u32);
        let js_view: JsValue = view.clone().into();
        let n = call2(&self.handle, "read", &js_view, &opts_at(offset)?)
            .map_err(|_| io_err("read", "sync handle unavailable".to_string()))?
            .as_f64()
            .ok_or_else(|| io_err("read", "non-numeric byte count".to_string()))?
            as usize;
        if n != len {
            return Err(io_err(
                "read",
                format!("short read: {n}/{len} bytes at {offset}"),
            ));
        }
        let mut exact = vec![0u8; len];
        view.copy_to(&mut exact);
        Ok(exact)
    }

    fn set_len(&self, len: u64) -> Result<(), Error> {
        call1(&self.handle, "truncate", &JsValue::from_f64(len as f64))
            .map_err(|_| io_err("truncate", "sync handle unavailable".to_string()))?;
        Ok(())
    }

    fn sync_data(&self, _eventual: bool) -> Result<(), Error> {
        // OPFS flush is best-effort durability (not fsync — see module docs).
        // The dispatcher pairs this with paranoid two-phase commits.
        call0(&self.handle, "flush")
            .map_err(|_| io_err("flush", "sync handle unavailable".to_string()))?;
        Ok(())
    }

    fn write(&self, offset: u64, data: &[u8]) -> Result<(), Error> {
        let mut cursor = 0usize;
        while cursor < data.len() {
            let view = Uint8Array::new_with_length((data.len() - cursor) as u32);
            view.copy_from(&data[cursor..]);
            let n = call2(
                &self.handle,
                "write",
                &view.into(),
                &opts_at(offset + cursor as u64)?,
            )
            .map_err(|_| io_err("write", "sync handle unavailable".to_string()))?
            .as_f64()
            .ok_or_else(|| io_err("write", "non-numeric byte count".to_string()))?
                as usize;
            if n == 0 {
                return Err(io_err("write", "wrote 0 bytes".to_string()));
            }
            cursor += n;
        }
        Ok(())
    }
}
