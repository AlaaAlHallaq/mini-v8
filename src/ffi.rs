use crate::*;
use std::ffi::c_void;
use std::mem::ManuallyDrop;

extern "C" {
    pub(crate) fn mv8_interface_new() -> Interface;
    pub(crate) fn mv8_interface_drop(_: Interface);
    pub(crate) fn mv8_interface_eval(_: Interface, data: *const u8, length: usize) -> TryCatchDesc;
    pub(crate) fn mv8_interface_global(_: Interface) -> ValuePtr;
    pub(crate) fn mv8_interface_set_data(_: Interface, slot: u32, data: *mut c_void);
    pub(crate) fn mv8_interface_get_data(_: Interface, slot: u32) -> *mut c_void;
    pub(crate) fn mv8_value_ptr_clone(_: Interface, value: ValuePtr) -> ValuePtr;
    pub(crate) fn mv8_value_ptr_drop(value_ptr: ValuePtr);
    pub(crate) fn mv8_string_new(_: Interface, data: *const u8, length: usize) -> ValuePtr;
    pub(crate) fn mv8_string_to_utf8_value(_: Interface, value: ValuePtr) -> Utf8Value;
    pub(crate) fn mv8_utf8_value_drop(utf8_value: Utf8Value);
    pub(crate) fn mv8_array_new(_: Interface) -> ValuePtr;
    pub(crate) fn mv8_array_len(_: Interface, array: ValuePtr) -> u32;
    pub(crate) fn mv8_object_new(_: Interface) -> ValuePtr;
    pub(crate) fn mv8_object_get(_: Interface, object: ValuePtr, key: ValueDesc) -> TryCatchDesc;
    pub(crate) fn mv8_object_set(_: Interface, object: ValuePtr, key: ValueDesc, value: ValueDesc)
        -> TryCatchDesc;
    pub(crate) fn mv8_object_remove(_: Interface, object: ValuePtr, key: ValueDesc) -> TryCatchDesc;
    pub(crate) fn mv8_object_has(_: Interface, object: ValuePtr, key: ValueDesc) -> TryCatchDesc;
    pub(crate) fn mv8_coerce_boolean(_: Interface, value: ValueDesc) -> u8;
    pub(crate) fn mv8_coerce_number(_: Interface, value: ValueDesc) -> TryCatchDesc;
    pub(crate) fn mv8_coerce_string(_: Interface, value: ValueDesc) -> TryCatchDesc;
}

pub(crate) type Interface = *const c_void;
pub(crate) type ValuePtr = *const c_void;

#[repr(u8)]
pub(crate) enum ValueDescTag {
    Null,
    Undefined,
    Number,
    Boolean,
    Array,
    Function,
    Date,
    Object,
    String,
}

#[repr(C)]
pub(crate) union ValueDescPayload {
    pub(crate) byte: u8,
    pub(crate) number: f64,
    pub(crate) value_ptr: ValuePtr,
}

#[repr(C)]
pub(crate) struct ValueDesc {
    pub(crate) payload: ValueDescPayload,
    pub(crate) tag: ValueDescTag,
}

impl Drop for ValueDesc {
    fn drop(&mut self) {
        match self.tag {
            ValueDescTag::String |
            ValueDescTag::Array |
            ValueDescTag::Function |
            ValueDescTag::Object => unsafe { mv8_value_ptr_drop(self.payload.value_ptr) },
            _ => {},
        }
    }
}

impl ValueDesc {
    pub(crate) fn new(tag: ValueDescTag, payload: ValueDescPayload) -> ValueDesc {
        ValueDesc { tag, payload }
    }
}

#[repr(C)]
pub(crate) struct TryCatchDesc {
    pub(crate) value_desc: ValueDesc,
    pub(crate) is_exception: u8,
}

#[repr(C)]
pub(crate) struct Utf8Value {
    pub(crate) data: *const u8,
    pub(crate) length: i32,
    src: *const c_void,
}

// A reference to a V8-owned value.
pub(crate) struct Ref<'mv8> {
    pub(crate) mv8: &'mv8 MiniV8,
    pub(crate) value_ptr: ValuePtr,
}

impl<'mv8> Ref<'mv8> {
    pub(crate) fn new(mv8: &MiniV8, value_ptr: ValuePtr) -> Ref {
        Ref { mv8, value_ptr }
    }

    pub(crate) fn from_value_desc(mv8: &MiniV8, desc: ValueDesc) -> Ref {
        let value_ptr = unsafe { desc.payload.value_ptr };
        // `Ref` has taken ownership of the `value_ptr`, so there's no need to run `ValueDesc`'s
        // drop:
        ManuallyDrop::new(desc);
        Ref { mv8, value_ptr }
    }
}

impl<'mv8> Clone for Ref<'mv8> {
    fn clone(&self) -> Ref<'mv8> {
        let value_ptr = unsafe { mv8_value_ptr_clone(self.mv8.interface, self.value_ptr) };
        Ref { mv8: self.mv8, value_ptr }
    }
}

impl<'mv8> Drop for Ref<'mv8> {
    fn drop(&mut self) {
        unsafe { mv8_value_ptr_drop(self.value_ptr); }
    }
}

pub(crate) fn desc_to_result(mv8: &MiniV8, desc: TryCatchDesc) -> Result<Value> {
    let value = desc_to_value(mv8, desc.value_desc);
    if desc.is_exception == 0 { Ok(value) } else { Err(Error::Value(value)) }
}

pub(crate) fn desc_to_result_noval(mv8: &MiniV8, desc: TryCatchDesc) -> Result<()> {
    let is_exception = desc.is_exception == 1;
    if !is_exception { Ok(()) } else { Err(Error::Value(desc_to_value(mv8, desc.value_desc))) }
}

pub(crate) fn desc_to_result_val(mv8: &MiniV8, desc: TryCatchDesc) -> Result<ValueDesc> {
    let is_exception = desc.is_exception == 1;
    let desc = desc.value_desc;
    if !is_exception { Ok(desc) } else { Err(Error::Value(desc_to_value(mv8, desc))) }
}

pub(crate) fn desc_to_value(mv8: &MiniV8, desc: ValueDesc) -> Value {
    use ValueDescTag as VT;
    let value = match desc.tag {
        VT::Null => Value::Null,
        VT::Undefined => Value::Undefined,
        VT::Boolean => Value::Boolean(unsafe { desc.payload.byte != 0 }),
        VT::Number => Value::Number(unsafe { desc.payload.number }),
        VT::Date => Value::Date(unsafe { desc.payload.number }),
        VT::Array => Value::Array(Array(Ref::from_value_desc(mv8, desc))),
        VT::Function => Value::Function(Function(Ref::from_value_desc(mv8, desc))),
        VT::Object => Value::Object(Object(Ref::from_value_desc(mv8, desc))),
        VT::String => Value::String(String(Ref::from_value_desc(mv8, desc))),
    };

    value
}

pub(crate) fn value_to_desc<'mv8, 'a>(mv8: &'mv8 MiniV8, value: &'a Value<'mv8>) -> ValueDesc {
    fn ref_val(r: &Ref) -> ValuePtr {
        unsafe { mv8_value_ptr_clone(r.mv8.interface, r.value_ptr) }
    }

    use ValueDesc as V;
    use ValueDescTag as VT;
    use ValueDescPayload as VP;

    if let Some(r) = value.inner_ref() {
        if r.mv8.interface != mv8.interface {
            panic!("`Value` passed from one `MiniV8` instance to another");
        }
    }

    match *value {
        Value::Undefined => V::new(VT::Undefined, VP { byte: 0 }),
        Value::Null => V::new(VT::Null, VP { byte: 0 }),
        Value::Boolean(b) => V::new(VT::Boolean, VP { byte: if b { 1 } else { 0 } }),
        Value::Number(f) => V::new(VT::Number, VP { number: f }),
        Value::Date(f) => V::new(VT::Date, VP { number: f }),
        Value::Array(ref r) => V::new(VT::Array, VP { value_ptr: ref_val(&r.0) }),
        Value::Function(ref r) => V::new(VT::Function, VP { value_ptr: ref_val(&r.0) }),
        Value::Object(ref r) => V::new(VT::Object, VP { value_ptr: ref_val(&r.0) }),
        Value::String(ref r) => V::new(VT::String, VP { value_ptr: ref_val(&r.0) }),
    }
}
