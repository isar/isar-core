use crate::async_txn::IsarAsyncTxn;
use crate::raw_object_set::{RawObject, RawObjectSend};
use crate::{BoolSend, IntSend};
use isar_core::collection::IsarCollection;
use isar_core::error::Result;
use isar_core::txn::IsarTxn;
use serde_json::Value;

#[no_mangle]
pub unsafe extern "C" fn isar_get(
    collection: &IsarCollection,
    txn: &mut IsarTxn,
    object: &mut RawObject,
) -> i32 {
    isar_try! {
        let object_id = object.get_object_id().unwrap();
        let result = collection.get(txn, object_id)?;
        if let Some(result) = result {
            object.set_object(result);
        } else {
            object.clear();
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn isar_get_async(
    collection: &'static IsarCollection,
    txn: &IsarAsyncTxn,
    object: &'static mut RawObject,
) {
    let object = RawObjectSend(object);
    let oid = object.0.get_object_id().unwrap();
    txn.exec(move |txn| -> Result<()> {
        let result = collection.get(txn, oid)?;
        if let Some(result) = result {
            object.0.set_object(result);
        } else {
            object.0.clear();
        }
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn isar_put(
    collection: &mut IsarCollection,
    txn: &mut IsarTxn,
    object: &mut RawObject,
) -> i32 {
    isar_try! {
        let oid = object.get_object_id();
        let data = object.object_as_slice();
        let oid = collection.put(txn, oid, data)?;
        object.set_object_id(oid);
    }
}

#[no_mangle]
pub unsafe extern "C" fn isar_put_async(
    collection: &'static IsarCollection,
    txn: &IsarAsyncTxn,
    object: &'static mut RawObject,
) {
    let object = RawObjectSend(object);
    txn.exec(move |txn| -> Result<()> {
        let oid = object.0.get_object_id();
        let data = object.0.object_as_slice();
        let oid = collection.put(txn, oid, data)?;
        object.0.set_object_id(oid);
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn isar_delete(
    collection: &IsarCollection,
    txn: &mut IsarTxn,
    object: &RawObject,
    deleted: &mut bool,
) -> i32 {
    isar_try! {
    let oid = object.get_object_id().unwrap();
        *deleted = collection.delete(txn, oid)?;
    }
}

#[no_mangle]
pub unsafe extern "C" fn isar_delete_async(
    collection: &'static IsarCollection,
    txn: &IsarAsyncTxn,
    object: &RawObject,
    deleted: &'static mut bool,
) {
    let oid = object.get_object_id().unwrap();
    let deleted = BoolSend(deleted);
    txn.exec(move |txn| {
        *deleted.0 = collection.delete(txn, oid)?;
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn isar_delete_all(
    collection: &IsarCollection,
    txn: &mut IsarTxn,
    count: &mut i64,
) -> i32 {
    isar_try! {
        *count = collection.delete_all(txn)? as i64;
    }
}

#[no_mangle]
pub unsafe extern "C" fn isar_delete_all_async(
    collection: &'static IsarCollection,
    txn: &IsarAsyncTxn,
    count: &'static mut i64,
) {
    let count = IntSend(count);
    txn.exec(move |txn| -> Result<()> {
        *(count.0) = collection.delete_all(txn)? as i64;
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn isar_json_import_async(
    collection: &'static IsarCollection,
    txn: &IsarAsyncTxn,
    json_bytes: *const u8,
    json_length: u32,
) {
    let bytes = std::slice::from_raw_parts(json_bytes, json_length as usize);
    let json: Value = serde_json::from_slice(bytes).unwrap();
    txn.exec(move |txn| -> Result<()> { collection.import_json(txn, json) });
}

struct JsonBytes(*mut *mut u8);
unsafe impl Send for JsonBytes {}

struct JsonLen(*mut u32);
unsafe impl Send for JsonLen {}

#[no_mangle]
pub unsafe extern "C" fn isar_json_export_async(
    collection: &'static IsarCollection,
    txn: &IsarAsyncTxn,
    primitive_null: bool,
    json_bytes: *mut *mut u8,
    json_length: *mut u32,
) {
    let json = JsonBytes(json_bytes);
    let json_length = JsonLen(json_length);
    txn.exec(move |txn| -> Result<()> {
        let exported_json = collection.export_json(txn, primitive_null, true)?;
        let bytes = serde_json::to_vec(&exported_json).unwrap();
        let mut bytes = bytes.into_boxed_slice();
        json_length.0.write(bytes.len() as u32);
        json.0.write(bytes.as_mut_ptr());
        std::mem::forget(bytes);
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn isar_free_json(json_bytes: *mut u8, json_length: u32) {
    Vec::from_raw_parts(json_bytes, json_length as usize, json_length as usize);
}
